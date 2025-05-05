use base64::prelude::*;
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::{
    client::conn::http1::Builder, server::conn::http1, service::service_fn, upgrade::Upgraded,
    Method, Request, Response,
};
use hyper_util::rt::TokioIo;
use log::{debug, error, info};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rustls_pki_types::ServerName;
use serde::Serialize;
use serde_json::json;
use std::{
    collections::HashMap,
    fmt::Debug,
    net::{Ipv6Addr, SocketAddr},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{watch, Semaphore},
};
use tokio_rustls::{
    client::TlsStream,
    rustls::{ClientConfig, RootCertStore},
    TlsConnector,
};
use tonnelle_core::{cidr, create_ipv6_socket, SockAddr, Socket};

const TUNNEL_CIDR: &str = "2001:470:8a72::/48";

static TLS_CONNECTOR: Lazy<TlsConnector> = Lazy::new(|| {
    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();
    TlsConnector::from(Arc::new(config))
});

static CONCURRENCY_SEM: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(16));
static CIDR: Lazy<cidr::Ipv6Cidr> = Lazy::new(|| cidr::Ipv6Cidr::parse(TUNNEL_CIDR));
static SHUTDOWN_CHANNEL: Lazy<Mutex<(watch::Sender<()>, watch::Receiver<()>)>> =
    Lazy::new(|| Mutex::new(watch::channel(())));

static ADDR_CACHE: Lazy<Mutex<HashMap<String, SocketAddr>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static WARM_SOCKETS: Mutex<Vec<(Socket, Ipv6Addr)>> = Mutex::new(Vec::new());
static WARM_SOCKETS_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Serialize)]
struct Status {
    warm_sockets: usize,
}

async fn to_ipv6_socket_addr_async<T: tokio::net::ToSocketAddrs + Debug + Clone>(
    input: T,
) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let key = format!("{:?}", &input);
    let addr = {
        let cache = ADDR_CACHE.lock();
        cache.get(&key).cloned()
    };

    if let Some(addr) = addr {
        debug!("Resolved from cache: {}", addr);
        Ok(addr)
    } else {
        let resolved = tokio::net::lookup_host(&input)
            .await?
            .find(|addr| addr.is_ipv6())
            .ok_or("No IPv6 address found")?;
        ADDR_CACHE.lock().insert(key, resolved);
        debug!("Resolved: {}", resolved);
        Ok(resolved)
    }
}

async fn warmup_sockets(num: usize) {
    debug!("Warming up {} sockets", num);

    let mut sockets = Vec::new();
    for _ in 0..num {
        let addr = CIDR.generate_random_ipv6_in_subnet();
        let socket = unsafe { create_ipv6_socket(addr).unwrap() };
        sockets.push((socket, addr));
    }

    WARM_SOCKETS_COUNT.fetch_add(num, Ordering::Relaxed);
    WARM_SOCKETS.lock().extend(sockets);

    debug!("Warmed up {} sockets", num);
}

async fn get_socket() -> Socket {
    let num_sockets = WARM_SOCKETS_COUNT.load(Ordering::Relaxed);
    if num_sockets > 1 {
        debug!("Using warm socket ({} available)", num_sockets);
        let socket = WARM_SOCKETS.lock().pop().unwrap();
        WARM_SOCKETS_COUNT.fetch_sub(1, Ordering::Relaxed);
        debug!("Socket addr: {:?}", socket.1);
        socket.0
    } else {
        tokio::task::spawn(async {
            warmup_sockets(16).await;
        });

        let addr = CIDR.generate_random_ipv6_in_subnet();
        let socket = unsafe { create_ipv6_socket(addr).unwrap() };
        socket.into()
    }
}

fn parse_credentials(header_val: &str) -> (Option<String>, Option<String>) {
    let parts: Vec<&str> = header_val.split_whitespace().collect();
    if parts.len() == 2 && parts[0] == "Basic" {
        let decoded = BASE64_STANDARD.decode(parts[1]).ok().unwrap();
        let decoded = String::from_utf8(decoded).ok().unwrap();
        let parts: Vec<&str> = decoded.split(':').collect();
        if parts.len() == 2 {
            (Some(parts[0].to_string()), Some(parts[1].to_string()))
        } else if parts.len() == 1 {
            (Some(parts[0].to_string()), None)
        } else {
            (None, None)
        }
    } else {
        (None, None)
    }
}

fn build_https_uri(req: &mut http::Request<hyper::body::Incoming>) {
    let uri = req.uri_mut();
    let scheme = uri.scheme_str().unwrap_or("https");
    let authority = uri.authority().unwrap();
    let path_and_query = uri.path_and_query().unwrap();
    *uri = http::Uri::builder()
        .scheme(scheme)
        .authority(authority.to_string())
        .path_and_query(path_and_query.to_string())
        .build()
        .unwrap();
}

fn debug_or_prod_addr() -> SocketAddr {
    if std::env::var("DEBUG").unwrap_or_else(|_| "false".to_string()) == "true" {
        SocketAddr::from(([127, 0, 0, 1], 1080))
    } else {
        SocketAddr::from(([0, 0, 0, 0], 1080))
    }
}

async fn mgmt_service(
    req: Request<hyper::body::Incoming>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    debug!("Received management request: {:?}", req);
    if req.uri().path() == "/shutdown" {
        SHUTDOWN_CHANNEL.lock().0.send(()).unwrap();
        let mut resp = Response::new(full("Shutting down"));
        *resp.status_mut() = http::StatusCode::OK;
        return Ok(resp);
    } else if req.uri().path() == "/info" {
        let crate_version = env!("CARGO_PKG_VERSION");
        let mut resp = Response::new(full(format!("tonnelle-proxy v{}", crate_version)));
        *resp.status_mut() = http::StatusCode::OK;
        return Ok(resp);
    } else if req.uri().path() == "/status" {
        let status = Status {
            warm_sockets: WARM_SOCKETS_COUNT.load(Ordering::Relaxed),
        };
        let body = full(json!(status).to_string());
        let mut resp = Response::new(body);
        *resp.status_mut() = http::StatusCode::OK;
        resp.headers_mut()
            .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
        return Ok(resp);
    }
    // Return 404 if not matched
    let mut resp = Response::new(full("Not Found"));
    *resp.status_mut() = http::StatusCode::NOT_FOUND;
    Ok(resp)
}

pub async fn run_http_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!("Starting management server");
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    info!("Management server listening on http://0.0.0.0:8080");
    let mut shutdown_rx = { SHUTDOWN_CHANNEL.lock().1.clone() };

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        tokio::select! {
            _ = shutdown_rx.changed() => {
                info!("Shutdown signal received, stopping HTTP server");
                break;
            },
            _ = tokio::task::spawn(async move {
                if let Err(err) = http1::Builder::new()
                    .preserve_header_case(true)
                    .title_case_headers(true)
                    .serve_connection(io, service_fn(mgmt_service))
                    .with_upgrades()
                    .await
                {
                    info!("Error serving mgmt connection: {:?}", err);
                }
            }) => {},
        }
    }
    Ok(())
}

pub fn start_bench_proxy() -> SocketAddr {
    let addr = debug_or_prod_addr();
    tokio::spawn(async move {
        // Spawn both servers
        let (proxy_result, server_result) = tokio::join!(run_tonnelle_proxy(), run_http_server());
        if let Err(e) = proxy_result {
            error!("Error running tonnelle proxy: {:?}", e);
        }
        if let Err(e) = server_result {
            error!("Error running HTTP server: {:?}", e);
        }
    });
    addr
}

pub async fn run_tonnelle_proxy() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pretty_env_logger::init();
    debug!("Starting tonnelle-proxy");
    let addr = debug_or_prod_addr();

    debug!("Warmup sockets");
    tokio::task::spawn(async {
        warmup_sockets(16).await;
    });

    let listener = TcpListener::bind(addr).await?;
    info!("Listening on http://{}", addr);
    let mut shutdown_rx = { SHUTDOWN_CHANNEL.lock().1.clone() };

    loop {
        let (stream, _) = listener.accept().await?;
        debug!("Accepted a new connection, acquiring permit next");
        let io = TokioIo::new(stream);

        let permit = CONCURRENCY_SEM.acquire().await.unwrap();
        debug!("Concurrency permit acquired, starting request handling");

        tokio::select! {
            _ = shutdown_rx.changed() => {
                info!("Shutdown signal received");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                break;
            },
            _ = tokio::task::spawn(async move {
                debug!("Entering select block for shutdown or connection task");
                if let Err(err) = http1::Builder::new()
                    .preserve_header_case(true)
                    .title_case_headers(true)
                    .serve_connection(io, service_fn(proxy))
                    .with_upgrades()
                    .await
                {
                    info!("Failed to serve connection: {:?}", err);
                }
                drop(permit);
            }) => {},
        }
    }

    Ok(())
}

async fn proxy(
    req: Request<hyper::body::Incoming>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    debug!("req: {:?}", req);
    debug!("Starting proxy for request");

    let (username, password) = req
        .headers()
        .get("Proxy-Authorization")
        .map(|header| parse_credentials(header.to_str().ok().unwrap()))
        .unwrap_or((None, None));

    let options = username
        .as_deref()
        .map(|u| u.split('-').collect::<Vec<&str>>())
        .unwrap_or_default();

    debug!("Username: {:?}, Password: {:?}", username, password);

    // HTTPS (CONNECT) handling
    if Method::CONNECT == req.method() {
        // Handle CONNECT method
        if let Some(addr) = host_addr(req.uri()) {
            tokio::task::spawn(async move {
                match hyper::upgrade::on(req).await {
                    Ok(upgraded) => {
                        if let Err(e) = tunnel(upgraded, addr).await {
                            info!("server io error: {}", e);
                        };
                    }
                    Err(e) => info!("upgrade error: {}", e),
                }
            });

            Ok(Response::new(empty()))
        } else {
            info!("CONNECT host is not socket addr: {:?}", req.uri());
            let mut resp = Response::new(full("CONNECT must be to a socket address"));
            *resp.status_mut() = http::StatusCode::BAD_REQUEST;

            Ok(resp)
        }
    } else {
        // Handle other requests (HTTP)
        let host = req.uri().host().expect("uri has no host").to_string();
        info!("Connecting to: {}", host);
        let port = req.uri().port_u16().unwrap_or(80);

        let socket = get_socket().await;
        debug!("Created new socket");
        let socket_addr = to_ipv6_socket_addr_async((host.as_str(), port))
            .await
            .unwrap();

        socket.connect(&SockAddr::from(socket_addr)).unwrap();

        let stream_std: std::net::TcpStream = socket.into();
        let stream = TcpStream::from_std(stream_std).unwrap();

        if options.iter().any(|&opt| opt == "rewrite") {
            // Rewrite the request to use HTTPS instead of HTTP (bypasses using CONNECT)
            debug!("Rewriting request to use HTTPS");
            let server_name = ServerName::try_from(host).unwrap();
            let stream = TLS_CONNECTOR.connect(server_name, stream).await.unwrap();

            let mut req = req;
            req.headers_mut().remove("Proxy-Authorization");

            build_https_uri(&mut req);

            let io: TokioIo<TlsStream<TcpStream>> = TokioIo::new(stream);

            let (mut sender, conn) = Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .handshake(io)
                .await?;

            tokio::task::spawn(async move {
                if let Err(err) = conn.await {
                    info!("Connection failed: {:?}", err);
                }
            });

            let resp = sender.send_request(req).await?;
            debug!("Completed handling proxy logic, preparing response");
            Ok(resp.map(|b| b.boxed()))
        } else {
            // HTTP request
            let io: TokioIo<TcpStream> = TokioIo::new(stream);

            let (mut sender, conn) = Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .handshake(io)
                .await?;

            tokio::task::spawn(async move {
                if let Err(err) = conn.await {
                    info!("Connection failed: {:?}", err);
                }
            });

            let resp = sender.send_request(req).await?;
            debug!("Completed handling proxy logic, preparing response");
            Ok(resp.map(|b| b.boxed()))
        }
    }
}

fn host_addr(uri: &http::Uri) -> Option<String> {
    uri.authority().map(|auth| auth.to_string())
}

fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

// Create a TCP connection to host:port, build a tunnel between the connection and
// the upgraded connection
async fn tunnel(upgraded: Upgraded, addr: String) -> std::io::Result<()> {
    // Connect tunnel to remote server
    debug!("Establishing tunnel connection");
    let socket = get_socket().await;
    let socket_addr = to_ipv6_socket_addr_async(&addr).await.unwrap();
    socket.connect(&SockAddr::from(socket_addr)).unwrap();
    debug!("Bound new socket to: {}", addr);
    let stream_std: std::net::TcpStream = socket.into();
    let mut stream = TcpStream::from_std(stream_std).unwrap();

    let mut upgraded = TokioIo::new(upgraded);

    // Proxying data
    debug!("Tunnel established, starting bidirectional copy");
    let (from_client, from_server) =
        tokio::io::copy_bidirectional(&mut upgraded, &mut stream).await?;
    debug!("Tunnel data copy completed, returning success");

    // Print message when done
    info!(
        "Client wrote {} bytes and received {} bytes",
        from_client, from_server
    );

    Ok(())
}

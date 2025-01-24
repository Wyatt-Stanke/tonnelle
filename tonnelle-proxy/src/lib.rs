use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full};
use hyper::{
    client::conn::http1::Builder, server::conn::http1, service::service_fn, upgrade::Upgraded,
    Method, Request, Response,
};
use hyper_util::rt::TokioIo;
use log::{debug, info};
use once_cell::sync::Lazy;
use std::{collections::HashMap, fmt::Debug, net::SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::sync::{watch, Semaphore};
use tonnelle_core::{cidr, create_ipv6_socket, SockAddr};

const TUNNEL_CIDR: &str = "2001:470:8a72::/48";

static CONCURRENCY_SEM: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(10));
static CIDR: Lazy<cidr::Ipv6Cidr> = Lazy::new(|| cidr::Ipv6Cidr::parse(TUNNEL_CIDR));
static SHUTDOWN_CHANNEL: Lazy<Mutex<(watch::Sender<()>, watch::Receiver<()>)>> =
    Lazy::new(|| Mutex::new(watch::channel(())));

static ADDR_CACHE: Lazy<Mutex<HashMap<String, SocketAddr>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

async fn to_ipv6_socket_addr_async<T: tokio::net::ToSocketAddrs + Debug + Clone>(
    input: T,
) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let key = format!("{:?}", &input);
    let addr = {
        let cache = ADDR_CACHE.lock().await;
        cache.get(&key).cloned()
    };

    if let Some(addr) = addr {
        Ok(addr)
    } else {
        let resolved = tokio::net::lookup_host(&input)
            .await?
            .find(|addr| addr.is_ipv6())
            .ok_or("No IPv6 address found")?;
        ADDR_CACHE.lock().await.insert(key, resolved);
        Ok(resolved)
    }
}

pub fn start_bench_proxy() -> SocketAddr {
    let addr = if std::env::var("DEBUG").unwrap_or_else(|_| "false".to_string()) == "true" {
        SocketAddr::from(([127, 0, 0, 1], 1080))
    } else {
        SocketAddr::from(([0, 0, 0, 0], 1080))
    };
    tokio::spawn(async move {
        let _ = run_tonnelle_proxy().await;
    });
    addr
}

pub async fn run_tonnelle_proxy() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();
    debug!("Starting tonnelle-proxy");
    let addr = if std::env::var("DEBUG").unwrap_or_else(|_| "false".to_string()) == "true" {
        SocketAddr::from(([127, 0, 0, 1], 1080))
    } else {
        SocketAddr::from(([0, 0, 0, 0], 1080))
    };

    let listener = TcpListener::bind(addr).await?;
    info!("Listening on http://{}", addr);
    let mut shutdown_rx = SHUTDOWN_CHANNEL.lock().await.1.clone();

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

    if req.uri().path() == "/shutdown" {
        // Handle shutdown request
        info!("Shutdown request received");
        let mut resp = Response::new(full("Shutting down"));
        *resp.status_mut() = http::StatusCode::OK;

        // Send shutdown signal
        SHUTDOWN_CHANNEL.lock().await.0.send(()).unwrap();
        return Ok(resp);
    }

    if req.uri().path() == "/info" {
        // Handle info request
        let crate_version = env!("CARGO_PKG_VERSION");
        let mut resp = Response::new(full(format!("tonnelle-proxy v{}", crate_version)));
        *resp.status_mut() = http::StatusCode::OK;
        return Ok(resp);
    }

    if Method::CONNECT == req.method() {
        // Handle CONNECT method
        if let Some(addr) = host_addr(req.uri()) {
            tokio::task::spawn(async move {
                match hyper::upgrade::on(req).await {
                    Ok(upgraded) => {
                        if let Err(e) = tunnel(upgraded, addr).await {
                            eprintln!("server io error: {}", e);
                        };
                    }
                    Err(e) => eprintln!("upgrade error: {}", e),
                }
            });

            Ok(Response::new(empty()))
        } else {
            eprintln!("CONNECT host is not socket addr: {:?}", req.uri());
            let mut resp = Response::new(full("CONNECT must be to a socket address"));
            *resp.status_mut() = http::StatusCode::BAD_REQUEST;

            Ok(resp)
        }
    } else {
        // Handle other requests
        let host = req.uri().host().expect("uri has no host");
        info!("Connecting to: {}", host);
        let port = req.uri().port_u16().unwrap_or(80);

        let addr = CIDR.generate_random_ipv6_in_subnet();
        debug!("Generated random address: {}", addr);
        let socket = unsafe { create_ipv6_socket(addr).unwrap() };
        debug!("Created new socket");
        let socket_addr = to_ipv6_socket_addr_async((host, port)).await.unwrap();

        debug!("Resolved to: {}", socket_addr);
        socket.connect(&SockAddr::from(socket_addr)).unwrap();
        debug!("Bound new socket to: {}", addr);

        let stream_std: std::net::TcpStream = socket.into();
        let stream = TcpStream::from_std(stream_std).unwrap();
        let io = TokioIo::new(stream);

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
    let tunnel_addr = CIDR.generate_random_ipv6_in_subnet();
    let socket = unsafe { create_ipv6_socket(tunnel_addr).unwrap() };
    let socket_addr = to_ipv6_socket_addr_async(&addr).await.unwrap();
    debug!("Resolved to: {}", socket_addr);
    socket.connect(&SockAddr::from(socket_addr)).unwrap();
    debug!("Bound new socket to: {}", addr);
    let stream_std: std::net::TcpStream = socket.into();
    let mut stream = TcpStream::from_std(stream_std).unwrap();

    let mut upgraded = TokioIo::new(upgraded);

    // Proxying data
    info!("Proxying {} via tunnel {}", addr, tunnel_addr);
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

use log::error;
use std::env;
use tonnelle_proxy::{run_http_server, run_tonnelle_proxy};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Spawn both servers and wait for them to complete
    let (proxy_result, server_result) = tokio::join!(run_tonnelle_proxy(), run_http_server());
    if let Err(e) = proxy_result {
        error!("Error running tonnelle proxy: {:?}", e);
    }
    if let Err(e) = server_result {
        error!("Error running HTTP server: {:?}", e);
    }
    Ok(())
}

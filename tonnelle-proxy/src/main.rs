use tonnelle_proxy::run_tonnelle_proxy;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_tonnelle_proxy().await
}

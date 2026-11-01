#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    divoom_ditoo_controller::run_cli().await
}

#[tokio::main]
async fn main() {
    let code = ksforge::cli::run().await;
    std::process::exit(code);
}

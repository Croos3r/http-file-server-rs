use clap::Parser;

use crate::{cli::Arguments, server::HttpServer};

mod cli;
mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    simple_logger::init_with_env().unwrap();
    let args = Arguments::parse();

    let server = HttpServer::new(
        args.host,
        args.port,
        args.directory,
        args.password,
        args.upload.then(|| args.upload_endpoint),
    )
    .await?;

    server.start().await?;

    Ok(())
}

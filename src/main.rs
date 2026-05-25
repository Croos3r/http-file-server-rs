use clap::Parser;

use crate::{authenticator::Authenticator, cli::Arguments, server::HttpServer};

mod authenticator;
mod cli;
mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    simple_logger::init_with_env().unwrap();
    let args = Arguments::parse();

    let authenticator = Authenticator::new(args.bearer_tokens, args.basic_auth_combos);

    let server = HttpServer::new(
        args.host,
        args.port,
        args.directory,
        args.upload.then_some(args.upload_endpoint),
        args.max_upload_size,
        authenticator,
    )
    .await?;

    server.start().await?;

    Ok(())
}

use std::{net::Ipv4Addr, path::PathBuf, str::FromStr};

use clap::Parser;

fn parse_ip_address(ip: &str) -> Result<[u8; 4], String> {
    let ip = Ipv4Addr::from_str(ip).map_err(|err| err.to_string())?;

    Ok(ip.octets())
}

fn parse_directory_pathbuf(path: &str) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(path);

    if !path_buf.is_dir() {
        return Err(format!("{path} is not a directory"));
    }

    Ok(path_buf)
}

#[derive(Debug, Parser)]
pub struct Arguments {
    /// Host to serve the files on
    #[arg(short = 'H', long, value_parser = parse_ip_address, default_value = "127.0.0.1")]
    pub host: [u8; 4],

    /// Port to serve the files on
    #[arg(short = 'P', long, value_parser = clap::value_parser!(u16), default_value = "8080")]
    pub port: u16,

    /// Root directory of the files to serve
    #[arg(value_parser = parse_directory_pathbuf, default_value = ".")]
    pub directory: PathBuf,

    /// Simple authentification password to be used to access to endpoints
    #[arg(short = 'p', long = "password")]
    pub password: Option<String>,
}

use std::{
    net::Ipv4Addr,
    path::{Component, PathBuf},
    str::FromStr,
};

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

fn parse_absolute_path(path: &str) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(path);

    if !path_buf.is_absolute()
        || path_buf
            .components()
            .any(|c| matches!(c, Component::Prefix(_) | Component::ParentDir))
    {
        return Err(format!("{path} is not a valid endpoint"));
    }

    Ok(path_buf)
}

fn parse_usize_bigger_than_zero(size: &str) -> Result<usize, String> {
    let value = size.parse::<usize>().map_err(|err| err.to_string())?;

    if value < 1 {
        return Err(format!("{size} must be greater than zero"));
    }
    Ok(value)
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

    /// Enable the ability to upload files
    #[arg(short, long)]
    pub upload: bool,

    /// Actual endpoint to use for file upload
    #[arg(long = "upload-endpoint", value_parser = parse_absolute_path, default_value = "/upload", requires = "upload")]
    pub upload_endpoint: PathBuf,

    /// Maximum size of a file that can be uploaded (in bytes)
    #[arg(long = "max-upload-size", value_parser = parse_usize_bigger_than_zero, default_value = "50000", requires = "upload")]
    pub max_upload_size: usize,
}

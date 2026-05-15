use std::collections::HashMap;
use std::io::ErrorKind;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::{Request, Response, body::Bytes};
use hyper::{StatusCode, header};
use hyper_util::rt::TokioIo;
use log::{debug, error, info, trace};
use tokio::net::TcpListener;

use hyper::server::conn::http1;
use hyper::service::Service;

#[derive(Debug)]
pub struct HttpServer {
    file_server: FileServer,
    listener: TcpListener,
}

impl HttpServer {
    pub async fn new(host: [u8; 4], port: u16, directory: PathBuf) -> anyhow::Result<Self> {
        let addr = SocketAddr::from((host, port));

        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("Could not bind to {addr}"))?;

        info!("Bound on {addr}");

        Ok(Self {
            file_server: FileServer::new(directory)?,
            listener,
        })
    }

    pub async fn start(self) -> anyhow::Result<()> {
        loop {
            let (stream, addr) = self
                .listener
                .accept()
                .await
                .context("Could not accept a new connection")?;
            trace!("Received connection from {addr}");

            let io = TokioIo::new(stream);
            let file_server = self.file_server.clone();

            tokio::task::spawn(async move {
                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, file_server)
                    .await
                {
                    error!("Error serving connection {err}");
                }
            });
        }
    }
}

#[derive(Debug, Clone)]
struct FileServer {
    directory: PathBuf,
    cache: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
}

impl FileServer {
    pub fn new(directory: PathBuf) -> Result<Self, std::io::Error> {
        Ok(Self {
            directory: directory.canonicalize()?,
            cache: Arc::new(Mutex::new(HashMap::default())),
        })
    }

    async fn get_file_content(&self, file_path: PathBuf) -> Result<Vec<u8>, std::io::Error> {
        let mut path = self.directory.clone();
        path.push(&file_path);
        let path = tokio::fs::canonicalize(path).await?;
        if let Some(cached_content) = self.cache.lock().expect("lock poisoned").get(&path) {
            debug!("Cache hit for {}", path.display());
            return Ok(cached_content.clone());
        }

        if !path.starts_with(&self.directory) {
            debug!(
                "Filtered out path traversal attempt with {} ({})",
                path.display(),
                file_path.display()
            );
            return Err(std::io::Error::from_raw_os_error(2));
        }

        tokio::fs::read(&path).await.inspect(|content| {
            debug!("Cache miss for {}, memoizing...", path.display());
            self.cache
                .lock()
                .expect("lock poisoned")
                .entry(path)
                .or_insert(content.clone());
        })
    }
}

impl Service<Request<Incoming>> for FileServer {
    type Response = Response<Full<Bytes>>;
    type Error = hyper::http::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let path = PathBuf::from(&req.uri().path()[1..]);
        let cloned_self = self.clone();

        Box::pin(async move {
            let body = cloned_self.get_file_content(path).await;
            let response = match body {
                Ok(body) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/octet-stream")
                    .header(header::CONTENT_LENGTH, body.len())
                    .body(Full::new(Bytes::from(body))),
                Err(err) => Response::builder()
                    .status(match err.kind() {
                        ErrorKind::NotFound => StatusCode::NOT_FOUND,
                        _ => StatusCode::INTERNAL_SERVER_ERROR,
                    })
                    .header(header::CONTENT_TYPE, "text/plain")
                    .header(header::CONTENT_LENGTH, err.to_string().len())
                    .body(Full::new(Bytes::from(err.to_string()))),
            }
            .unwrap();

            info!(
                "{} {} => {}",
                req.method(),
                req.uri().path(),
                response.status(),
            );

            Ok(response)
        })
    }
}

use std::collections::HashMap;
use std::fs::FileType;
use std::io::{self, ErrorKind};
use std::ops::Deref;
use std::path::{Component, Path};
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::{net::SocketAddr, path::PathBuf};

use anyhow::Context;
use futures_util::TryStreamExt;
use http_body_util::{BodyStream, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{header, StatusCode};
use hyper::{Method, Request, Response};
use hyper_util::rt::TokioIo;
use log::{debug, error, info, trace, warn};
use maud::{html, DOCTYPE};
use mime_guess::mime;
use multer::{Field, Multipart};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use hyper::server::conn::http1;
use hyper::service::Service;

#[derive(Debug)]
pub struct HttpServer {
    file_server: FileServer,
    listener: TcpListener,
}

impl HttpServer {
    pub async fn new(
        host: [u8; 4],
        port: u16,
        directory: PathBuf,
        password: Option<String>,
    ) -> anyhow::Result<Self> {
        let addr = SocketAddr::from((host, port));

        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("Could not bind to {addr}"))?;

        info!("Bound on {addr}");

        Ok(Self {
            file_server: FileServer::new(directory, password)?,
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
    password: Option<String>,
    directory: Arc<PathBuf>,
    cache: Arc<RwLock<HashMap<PathBuf, Bytes>>>,
}

impl FileServer {
    pub fn new(directory: PathBuf, password: Option<String>) -> Result<Self, std::io::Error> {
        Ok(Self {
            password,
            directory: Arc::new(directory.canonicalize()?),
            cache: Arc::new(RwLock::new(HashMap::default())),
        })
    }

    async fn sanitize_file_path(&self, file_path: &Path) -> io::Result<PathBuf> {
        let mut path = self.directory.deref().clone();
        path.push(file_path);
        let path = tokio::fs::canonicalize(path).await?;

        if !path.starts_with(self.directory.as_path()) {
            warn!(
                "Filtered out path traversal attempt with {} ({})",
                path.display(),
                file_path.display()
            );
            return Err(std::io::Error::from_raw_os_error(2));
        }

        Ok(path)
    }

    async fn get_file_content(&self, file_path: &Path) -> Result<Bytes, std::io::Error> {
        if let Some(cached_content) = self.cache.read().expect("lock poisoned").get(file_path) {
            debug!("Cache hit for {}", file_path.display());
            return Ok(cached_content.clone());
        }

        tokio::fs::read(file_path).await.map(|content| {
            debug!("Cache miss for {}, memoizing...", file_path.display());
            let content = Bytes::from(content);
            self.cache
                .write()
                .expect("lock poisoned")
                .entry(file_path.to_path_buf())
                .or_insert(content.clone());
            content
        })
    }

    fn is_valid_password(&self, password: Option<&[u8]>) -> bool {
        let Some(ref server_password) = self.password else {
            return true;
        };

        password
            .map(|password| password == server_password.as_bytes())
            .unwrap_or(false)
    }

    fn construct_invalid_password_response(endpoint: &str) -> Response<Full<Bytes>> {
        Self::construct_simple_response(
            format!("You are not authorized to access {}", endpoint),
            StatusCode::UNAUTHORIZED,
        )
    }

    fn construct_html_directory_listing_response(
        cur_path: &Path,
        entries: &[String],
    ) -> Response<Full<Bytes>> {
        let html = html! {
            (DOCTYPE)
            html {
                head {
                    title {
                        "HTTP File Server - " ({
                            cur_path.display()
                        }) "/"
                    }
                }
                body {
                    h1 {
                        (cur_path.display()) "/"
                    }
                    ul {
                        @if let Some(parent) = cur_path.parent() {
                            li {
                                a href={
                                    "/" (parent.display())
                                } {
                                    ".."
                                }
                            }
                        }
                        @for entry in entries {
                            @let prefix = if cur_path != Path::new("") {
                                "/".to_string() + &cur_path.to_string_lossy()
                            } else {
                                "".to_string()
                            };
                            li {
                                a href={
                                    (prefix) "/" (entry)
                                } {
                                    (entry)
                                }
                            }
                        }
                    }
                }
            }
        };
        Response::builder()
            .header(header::CONTENT_TYPE, mime::TEXT_HTML.to_string())
            .body(Full::new(Bytes::from(html.into_string())))
            .unwrap()
    }

    async fn handle_file_retrieve(&self, file_path: &Path) -> Response<Full<Bytes>> {
        let Ok(body) = self.get_file_content(file_path).await else {
            return Self::construct_not_found_response(&file_path.to_string_lossy());
        };
        Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                mime_guess::from_path(file_path)
                    .first_or_octet_stream()
                    .to_string(),
            )
            .header(header::CONTENT_LENGTH, body.len())
            .body(Full::new(Bytes::from(body)))
            .unwrap()
    }

    fn construct_simple_response(message: String, status: StatusCode) -> Response<Full<Bytes>> {
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, mime::TEXT_PLAIN.to_string())
            .header(header::CONTENT_LENGTH, message.len())
            .body(Full::new(Bytes::from(message)))
            .unwrap()
    }

    fn construct_not_found_response(endpoint: &str) -> Response<Full<Bytes>> {
        Self::construct_simple_response(format!("{} Not Found", endpoint), StatusCode::NOT_FOUND)
    }

    async fn write_field_chunks(path: &Path, field: &mut Field<'_>) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(path)
            .await?;
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|err| io::Error::new(ErrorKind::InvalidData, err))?
        {
            file.write_all(&chunk).await?;
        }
        file.flush().await
    }

    async fn handle_file_upload(&self, req: Request<Incoming>) -> Response<Full<Bytes>> {
        let Some(Ok(boundary)) = req.headers().get(header::CONTENT_TYPE).map(|content_type| {
            multer::parse_boundary(String::from_utf8_lossy(content_type.as_bytes()))
        }) else {
            debug!("Could not get the boundary value from the content type for file upload");
            return Self::construct_simple_response(
                "Invalid multipart data content type".to_string(),
                StatusCode::BAD_REQUEST,
            );
        };
        let stream = BodyStream::new(req.into_body())
            .try_filter_map(|frame| async move { Ok(frame.into_data().ok()) });

        let mut multipart_stream = Multipart::new(stream, boundary);

        while let Some(mut field) = match multipart_stream.next_field().await {
            Ok(field) => field,
            Err(err) => {
                return Self::construct_simple_response(
                    format!("Could not get next field: {err}"),
                    StatusCode::UNAUTHORIZED,
                );
            }
        } {
            let Some(file_name) = field.file_name() else {
                continue;
            };
            if Path::new(file_name).components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            }) {
                warn!("Filtered out a path traversal attempt: {file_name}");
                return Self::construct_simple_response(
                    "You are not allowed to write to this directory".to_string(),
                    StatusCode::FORBIDDEN,
                );
            }
            let path = self.directory.join(file_name);
            if let Err(err) = Self::write_field_chunks(&path, &mut field).await {
                debug!("Could not write multipart data on disk: {err}");
                return Self::construct_simple_response(
                    format!("Could not write data on disk: {err}"),
                    StatusCode::INTERNAL_SERVER_ERROR,
                );
            }
            self.cache.write().expect("lock poisoned").remove(&path);
        }
        Self::construct_simple_response("Created".to_string(), StatusCode::CREATED)
    }

    async fn get_directory_listing(directory_path: &Path) -> io::Result<Vec<String>> {
        let mut entry_stream = tokio::fs::read_dir(directory_path).await?;
        let mut names = Vec::new();

        while let Some(entry) = entry_stream.next_entry().await? {
            names.push(
                entry.file_name().to_string_lossy().into_owned()
                    + if entry.file_type().await?.is_dir() {
                        "/"
                    } else {
                        ""
                    },
            );
        }

        Ok(names)
    }

    async fn handle_directory_display(&self, directory_path: &Path) -> Response<Full<Bytes>> {
        let Ok(entries) = Self::get_directory_listing(directory_path).await else {
            return Self::construct_simple_response(
                format!(
                    "Could not get the directory listing of {}",
                    directory_path.display()
                ),
                StatusCode::INTERNAL_SERVER_ERROR,
            );
        };

        Self::construct_html_directory_listing_response(
            directory_path
                .strip_prefix(self.directory.to_path_buf())
                .unwrap(),
            &entries,
        )
    }

    async fn handle_request(&self, req: Request<Incoming>) -> Response<Full<Bytes>> {
        let endpoint = req.uri().path();
        if !self.is_valid_password(
            req.headers()
                .get(header::AUTHORIZATION)
                .map(|password| password.as_bytes()),
        ) {
            trace!("Got an invalid password authentication attempt");
            return Self::construct_invalid_password_response(endpoint);
        }

        match (req.method(), endpoint) {
            (&Method::GET, _) => {
                let file_path = &PathBuf::from(&endpoint[1..]);
                let Ok(path) = self.sanitize_file_path(file_path).await else {
                    return Self::construct_not_found_response(endpoint);
                };

                if path.is_file() {
                    self.handle_file_retrieve(&path).await
                } else if path.is_dir() {
                    self.handle_directory_display(&path).await
                } else {
                    Self::construct_simple_response(
                        format!("Could not display the resource {endpoint}",),
                        StatusCode::INTERNAL_SERVER_ERROR,
                    )
                }
            }
            (&Method::POST, "/upload") => self.handle_file_upload(req).await,
            (_, endpoint) => Self::construct_not_found_response(endpoint),
        }
    }
}

impl Service<Request<Incoming>> for FileServer {
    type Response = Response<Full<Bytes>>;
    type Error = hyper::http::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let last = std::time::Instant::now();
        let endpoint = req.uri().path().to_string();
        let method = req.method().to_string();
        let cloned_self = self.clone();

        Box::pin(async move {
            let response = cloned_self.handle_request(req).await;
            let duration = std::time::Instant::now().duration_since(last).as_secs_f64() * 1000.;
            info!(
                "{} {} => {} ({:.2}ms)",
                method,
                endpoint,
                response.status(),
                duration,
            );
            Ok(response)
        })
    }
}

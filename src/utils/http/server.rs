use crate::utils::Logger;
use anyhow::Result;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Body, Bytes, Frame, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tower::{Service, ServiceBuilder};

#[derive(Debug, Clone)]
pub struct ServiceLogger<S> {
    inner: S,
}
impl<S> ServiceLogger<S> {
    pub fn new(inner: S) -> Self {
        ServiceLogger { inner }
    }
}
type Req = Request<Incoming>;

impl<S> Service<Req> for ServiceLogger<S>
where
    S: Service<Req> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&mut self, req: Req) -> Self::Future {
        println!("processing request: {} {}", req.method(), req.uri().path());
        self.inner.call(req)
    }

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }
}

async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

async fn echo(
    req: Request<Incoming>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => Ok(Response::new(full("Try POSTing data to /echo"))),
        (&Method::POST, "/echo") => Ok(Response::new(req.into_body().boxed())),
        (&Method::POST, "/echo/uppercase") => {
            // Convert every byte in every Data frame to uppercase
            let frame_stream = req.into_body().map_frame(|frame| {
                let data = match frame.into_data() {
                    Ok(data) => data.iter().map(|byte| byte.to_ascii_uppercase()).collect(),
                    Err(_) => Bytes::new(),
                };

                Frame::data(data)
            });

            Ok(Response::new(frame_stream.boxed()))
        }
        (&Method::POST, "/echo/reversed") => {
            // Set an upper bound of 64kb for body size
            let upper_bound = req.body().size_hint().upper().unwrap_or(u64::MAX);
            if upper_bound > 1024 * 64 {
                let mut resp = Response::new(full("Body too big"));
                *resp.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
                return Ok(resp);
            }

            // Await the whole body to be collected.
            let body = req.collect().await?.to_bytes();

            // Reverse the body and respond.
            let reversed_body = body.iter().rev().cloned().collect::<Vec<u8>>();

            Ok(Response::new(full(reversed_body)))
        }
        _ => {
            // Return 404 Not Found for other routes.
            let mut not_found = Response::new(empty());
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

pub type RequestSender = mpsc::Sender<Request<hyper::body::Incoming>>;

/// Helper function to create an Empty body.
fn empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::new().map_err(|never| match never {}).boxed()
}

/// Helper function to create a Full body.
fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

/// In the event of an error, returns a heap-allocatedw thread-safe error of any type that implements Error.
pub async fn run_server_example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Address of localhost on port 3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // Create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;
    println!("Listening on http://{}", addr);

    // Create a HTTP connection builder
    let http = http1::Builder::new();

    // Create a graceful shutdown watcher
    let graceful = GracefulShutdown::new();

    // Create a watcher for the shutdown signal to complete
    let mut signal = std::pin::pin!(shutdown_signal());

    // Start an event loop to continuously accept incoming connections
    loop {
        tokio::select! {
            Ok((stream, _remote_addr)) = listener.accept() => {
                let io = TokioIo::new(stream);

                // Create the service
                let echo_svc = tower::service_fn(echo);
                let tower_svc = ServiceBuilder::new()
                    .layer_fn(ServiceLogger::new)
                    .service(echo_svc);
                let svc = TowerToHyperService::new(tower_svc);

                // Bind the connection with the service
                let conn = http.serve_connection(io, svc);

                // Wrap the connection with a graceful shutdown watcher
                let watcher = graceful.watch(conn);

                tokio::task::spawn(async move {
                    if let Err(err) = watcher.await {
                        eprint!("Error serving connection: {:?}", err);
                    }
                });
            },
            _ = &mut signal => {
                // Shutdown signal completed, stop the accept event loop
                drop(listener);
                eprintln!("\nGraceful shutdown signal received.");
                break;
            }
        }
    }

    // Start the shutdown procedure and wait for all connection to complete
    // with a timeout to limit how long to wait.
    tokio::select! {
        _ = graceful.shutdown() => {
            eprintln!("All connections gracefully closed.");
        },
        _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
            eprintln!("Timed out waiting for all connections to close.");
        }
    }

    Ok(())
}

pub async fn run_server(tx: RequestSender, logger: Arc<dyn Logger>) -> Result<()> {
    // Address of localhost on port 3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // Create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;
    logger.info(&format!("Listening on http://{}", addr));

    // Create a HTTP connection builder
    let http = http1::Builder::new();

    // Create a graceful shutdown watcher
    let graceful = GracefulShutdown::new();

    // Create a watcher for the shutdown signal to complete
    let mut signal = std::pin::pin!(shutdown_signal());

    // Start an event loop to continuously accept incoming connections
    loop {
        tokio::select! {
            Ok((stream, _remote_addr)) = listener.accept() => {
                let io = TokioIo::new(stream);
                let tx = tx.clone();
                let logger = logger.clone();
                let http = http.clone();

                tokio::spawn(async move {
                    let service = service_fn(move |req: Request<Incoming>| {
                        let tx = tx.clone();
                        async move {
                            let _ = tx.send(req).await;
                            Ok::<_, hyper::Error>(Response::new(full("OK")))
                        }
                    });

                    if let Err(err) = http.serve_connection(io, service).await {
                        logger.error(&format!("Connection error: {:?}", err));
                    }
                });
            },
            _ = &mut signal => {
                // Shutdown signal completed, stop the accept event loop
                drop(listener);
                logger.info("Graceful shutdown signal received");
                break;
            }
        }
    }

    // Start the shutdown procedure and wait for all connection to complete
    // with a timeout to limit how long to wait.
    tokio::select! {
        _ = graceful.shutdown() => {
            logger.info("All connections gracefully closed");
        },
        _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {
            logger.error("Timed out waiting for all connections to close");
        }
    }

    Ok(())
}

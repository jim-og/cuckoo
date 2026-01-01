use crate::utils::Logger;
use anyhow::Result;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::{Body, Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

pub struct HttpRequest {
    pub method: http::Method,
    pub path: String,
    pub body: Bytes,
}

pub type RequestSender = mpsc::Sender<HttpRequest>;

/// Helper function to create an Empty body.
fn _empty() -> BoxBody<Bytes, hyper::Error> {
    Empty::new().map_err(|never| match never {}).boxed()
}

/// Helper function to create a Full body.
fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

/// Runs the HTTP server responsible for accepting external timer requests.
///
/// This server listens on a TCP socket, accepts incoming HTTP/1 connections,
/// and handles requests using a Tower-compatible service. Incoming requests
/// are decoded and forwarded over an asynchronous channel to the rest of the
/// application for processing (for example, creating or managing timers).
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
                            // Set an upper bound of 64kb for body size
                            let upper_bound = req.body().size_hint().upper().unwrap_or(u64::MAX);
                            if upper_bound > 1024 * 64 {
                                let mut resp = Response::new(full("Body too big"));
                                *resp.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
                                return Ok(resp);
                            }

                            // Await the whole body to be collected
                            let request = HttpRequest {
                                method: req.method().clone(),
                                path: req.uri().path().to_string(),
                                body: req.collect().await?.to_bytes()
                            };

                            let _ = tx.send(request).await;
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

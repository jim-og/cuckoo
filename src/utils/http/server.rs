use crate::utils::Logger;
use anyhow::Result;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};

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

pub async fn handle_request(
    req: Request<Incoming>,
    request_sender: RequestSender,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Set an upper bound of 64kb for body size
    let limited_body = Limited::new(req.into_body(), 1024 * 64);

    // Await the whole body to be collected
    let body = match limited_body.collect().await {
        Ok(collected_body) => collected_body.to_bytes(),
        Err(_) => {
            let mut resp = Response::new(full("Body too big"));
            *resp.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
            return Ok(resp);
        }
    };

    let request = HttpRequest { method, path, body };

    let _ = request_sender.send(request).await;
    Ok::<_, hyper::Error>(Response::new(full("OK")))
}

/// Runs the HTTP server responsible for accepting external timer requests.
///
/// This server listens on a TCP socket, accepts incoming HTTP/1 connections,
/// and handles requests using a Tower-compatible service. Incoming requests
/// are decoded and forwarded over an asynchronous channel to the rest of the
/// application for processing (for example, creating or managing timers).
pub async fn run_server(
    request_sender: RequestSender,
    logger: Arc<dyn Logger>,
    port: u16,
    ready_sender: oneshot::Sender<()>,
) -> Result<()> {
    // Address of localhost on port 3000
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    // Create a TcpListener and bind it to 127.0.0.1:port
    let listener = TcpListener::bind(addr).await?;
    logger.info(&format!("Listening on http://{}", addr));

    // Signal that we're ready
    let _ = ready_sender.send(());

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
                let request_sender = request_sender.clone();
                let logger = logger.clone();
                let http = http.clone();

                tokio::spawn(async move {
                    let service = service_fn(move |req: Request<Incoming>| {
                        let request_sender = request_sender.clone();
                        handle_request(req, request_sender)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::StdoutLogger;
    use http::Method;
    use std::sync::atomic::{AtomicU16, Ordering};
    use tokio::sync::mpsc::error::TryRecvError;

    struct TestServerHarness {
        port: u16,
        client: reqwest::Client,
        request_receiver: mpsc::Receiver<HttpRequest>,
    }

    impl TestServerHarness {
        async fn new() -> Self {
            // Generate a different port number for each test to avoid conflicts
            static PORT_GENERATOR: AtomicU16 = AtomicU16::new(3001);
            let port = PORT_GENERATOR.fetch_add(1, Ordering::SeqCst);

            let (request_sender, request_receiver) = mpsc::channel(10);
            let (ready_sender, ready_receiver) = oneshot::channel();
            let logger = Arc::new(StdoutLogger::new().with_receiver());
            let server_logger = logger.clone();

            // Spawn server
            tokio::spawn(async move {
                run_server(request_sender, server_logger, port, ready_sender)
                    .await
                    .unwrap()
            });

            // Wait for the server to be ready
            ready_receiver.await.unwrap();

            Self {
                port,
                client: reqwest::Client::new(),
                request_receiver,
            }
        }

        fn url(&self, path: &str) -> String {
            format!("http://127.0.0.1:{}{}", self.port, path)
        }

        async fn recv(&mut self) -> Option<HttpRequest> {
            self.request_receiver.recv().await
        }

        fn try_recv(&mut self) -> Result<HttpRequest, TryRecvError> {
            self.request_receiver.try_recv()
        }
    }

    #[tokio::test]
    async fn get_request_succeeds() -> Result<()> {
        let mut harness = TestServerHarness::new().await;
        let path = "/status";
        let resp = harness.client.get(harness.url(path)).send().await?;

        assert_eq!(resp.status(), StatusCode::OK);

        let req = harness.recv().await.unwrap();
        assert_eq!(req.method, Method::GET);
        assert_eq!(req.path, path);
        assert_eq!(req.body, Bytes::from(""));

        Ok(())
    }

    #[tokio::test]
    async fn post_request_succeeds() -> Result<()> {
        let mut harness = TestServerHarness::new().await;
        let path = "/test";
        let body = "hello world";
        let resp = harness
            .client
            .post(harness.url(path))
            .body("hello world")
            .send()
            .await?;

        assert_eq!(resp.status(), StatusCode::OK);

        let req = harness.recv().await.unwrap();
        assert_eq!(req.method, Method::POST);
        assert_eq!(req.path, path);
        assert_eq!(req.body, Bytes::from(body));

        Ok(())
    }

    #[tokio::test]
    async fn multiple_requests_suceed() -> Result<()> {
        let mut harness = TestServerHarness::new().await;
        let path = "/timer";

        for i in 0..5 {
            let resp = harness
                .client
                .post(harness.url(path))
                .body(format!("{}", i))
                .send()
                .await?;

            assert_eq!(resp.status(), StatusCode::OK);
        }

        for i in 0..5 {
            let req = harness.recv().await.unwrap();
            assert_eq!(req.path, path);
            assert_eq!(req.body, Bytes::from(format!("{}", i)));
        }

        Ok(())
    }

    #[tokio::test]
    async fn large_body_request_rejected() -> Result<()> {
        let mut harness = TestServerHarness::new().await;

        let large_body = vec![b'a'; 65 * 1024];

        let resp = harness
            .client
            .post(harness.url("/timer"))
            .body(large_body)
            .send()
            .await?;

        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert!(harness.try_recv().is_err());

        Ok(())
    }
}

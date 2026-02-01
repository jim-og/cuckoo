use crate::utils::http::router::Router;
use crate::utils::http::{HttpResponse, full};
use crate::utils::{HttpRequest, Logger};
use anyhow::Result;
use http_body_util::{BodyExt, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::timeout;

pub async fn handle_request(
    req: Request<Incoming>,
    router: Arc<Router>,
) -> Result<HttpResponse, hyper::Error> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // Set an upper bound of 64kb for body size
    let limited_body = Limited::new(req.into_body(), 1024 * 64);

    // Timeout on body collection to prevent slowloris attack.
    let request_timout = Duration::from_secs(5);

    // Await the whole body to be collected.
    let body = match timeout(request_timout, limited_body.collect()).await {
        Ok(Ok(collected_body)) => collected_body.to_bytes(),
        Ok(Err(_)) => {
            let mut resp = Response::new(full("Body too big"));
            *resp.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
            return Ok(resp);
        }
        Err(_) => {
            let mut resp = Response::new(full("Request timed out"));
            *resp.status_mut() = StatusCode::REQUEST_TIMEOUT;
            return Ok(resp);
        }
    };

    Ok(router.route(HttpRequest { method, path, body }).await)
}

/// Runs the HTTP server responsible for accepting external timer requests.
///
/// This server listens on a TCP socket, accepts incoming HTTP/1 connections,
/// and handles requests using a Tower-compatible service. Incoming requests
/// are decoded and forwarded over an asynchronous channel to the rest of the
/// application for processing (for example, creating or managing timers).
pub async fn run_server<F>(
    router: Arc<Router>,
    logger: Arc<dyn Logger>,
    port: u16,
    ready_sender: oneshot::Sender<()>,
    shutdown_signal: F,
) -> Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
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
    let graceful = Arc::new(GracefulShutdown::new());

    // Create a watcher for the shutdown signal to complete
    let mut signal = std::pin::pin!(shutdown_signal);

    // Start an event loop to continuously accept incoming connections
    loop {
        tokio::select! {
            Ok((stream, _remote_addr)) = listener.accept() => {
                let io = TokioIo::new(stream);
                let logger = logger.clone();
                let http = http.clone();
                let graceful = graceful.clone();
                let router = router.clone();

                tokio::spawn(async move {
                    let service = service_fn(move |req: Request<Incoming>| {
                        handle_request(req, router.clone())
                    });

                    let conn = graceful.watch(http.serve_connection(io, service));

                    if let Err(err) = conn.await {
                        logger.error(&format!("Connection error: {:?}", err));
                    }
                });
            },
            _ = &mut signal => {
                // Shutdown signal completed, stop the accept event loop
                logger.info("Graceful shutdown signal received");
                break;
            }
        }
    }

    let graceful = match Arc::try_unwrap(graceful) {
        Ok(graceful) => graceful,
        Err(_) => {
            logger.error("Failed to unwrap GracefulShutdown (connections still active)");
            return Ok(());
        }
    };

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
    use crate::utils::{RouteHandler, StdoutLogger};
    use async_trait::async_trait;
    use http::Method;
    use hyper::body::Bytes;
    use std::sync::atomic::{AtomicU16, Ordering};
    use tokio::io::AsyncReadExt;
    use tokio::sync::mpsc;
    use tokio::time::{Duration, advance, sleep};
    use tokio::{io::AsyncWriteExt, net::TcpStream, sync::mpsc::error::TryRecvError};

    async fn shutdown_signal_testable(shutdown_receiver: oneshot::Receiver<()>) {
        let _ = shutdown_receiver.await;
    }

    pub struct TestHandler {
        sender: mpsc::Sender<HttpRequest>,
    }

    impl TestHandler {
        pub fn new(sender: mpsc::Sender<HttpRequest>) -> Self {
            Self { sender }
        }
    }

    #[async_trait]
    impl RouteHandler for TestHandler {
        async fn handle(&self, req: HttpRequest) -> Result<HttpResponse, HttpResponse> {
            let _ = self.sender.send(req).await;
            Ok(Response::new(full("OK")))
        }
    }

    struct TestServerHarness {
        port: u16,
        client: reqwest::Client,
        request_receiver: mpsc::Receiver<HttpRequest>,
        shutdown_sender: oneshot::Sender<()>,
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
            let (shutdown_sender, shutdown_receiver) = oneshot::channel();
            let router = Arc::new(
                Router::new()
                    .add(
                        Method::GET,
                        "/test",
                        TestHandler::new(request_sender.clone()),
                    )
                    .add(
                        Method::POST,
                        "/test",
                        TestHandler::new(request_sender.clone()),
                    ),
            );

            // Spawn server
            tokio::spawn(async move {
                run_server(
                    router,
                    server_logger,
                    port,
                    ready_sender,
                    shutdown_signal_testable(shutdown_receiver),
                )
                .await
                .unwrap()
            });

            // Wait for the server to be ready
            ready_receiver.await.unwrap();

            Self {
                port,
                client: reqwest::Client::new(),
                request_receiver,
                shutdown_sender,
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
        let path = "/test";
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
        let path = "/test";

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
            .post(harness.url("/test"))
            .body(large_body)
            .send()
            .await?;

        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert!(harness.try_recv().is_err());

        Ok(())
    }

    #[ignore = "needs fixing"]
    #[tokio::test]
    async fn overloaded_server_returns_service_unavailable() -> Result<()> {
        let TestServerHarness {
            port,
            client,
            request_receiver,
            shutdown_sender: _shutdown_sender,
        } = TestServerHarness::new().await;

        // Drop the request receiver to simulate downstream failure
        drop(request_receiver);

        let resp = client
            .post(format!("http://127.0.0.1:{}/test", port))
            .body("hello")
            .send()
            .await?;

        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn slowloris_request_rejected() -> Result<()> {
        let mut harness = TestServerHarness::new().await;
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", harness.port)).await?;

        stream
            .write_all(b"POST /test HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\n\r\n")
            .await?;
        stream.flush().await?;

        // Drip feed the body
        for _ in 0..5 {
            let _ = stream.write_all(b"a").await;
            stream.flush().await?;
            sleep(Duration::from_secs(1)).await;
        }

        // Advance time past the 5s request timeout
        advance(Duration::from_secs(6)).await;

        // Server should have timed out and closed the connection
        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        let resp = String::from_utf8_lossy(&buf[..n]);

        assert!(resp.contains("408"), "expected HTTP 408, got:\n{}", resp);
        assert!(
            harness.try_recv().is_err(),
            "slowloris request must not reach handler"
        );

        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn timely_request_succeeds() -> Result<()> {
        let harness = TestServerHarness::new().await;
        let mut stream = TcpStream::connect(format!("127.0.0.1:{}", harness.port)).await?;

        stream
            .write_all(b"POST /test HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\n\r\n")
            .await?;
        stream.flush().await?;

        // Drip feed the body
        for _ in 0..100 {
            let _ = stream.write_all(b"a").await;
            stream.flush().await?;
            sleep(Duration::from_millis(45)).await; // 4.5 seconds total for 100 bytes
        }

        advance(Duration::from_secs(6)).await;

        let mut response = vec![0u8; 1024];
        let n = stream.read(&mut response).await.unwrap();
        let resp = String::from_utf8_lossy(&response[..n]);

        assert!(resp.contains("200 OK"));

        Ok(())
    }
}

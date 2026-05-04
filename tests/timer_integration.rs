mod common;

use async_trait::async_trait;
use common::TestHarness;
use cuckoo::utils::{
    HttpRequest, HttpResponse, RouteHandler, Router, StdoutLogger, full, run_server,
};
use http::Method;
use hyper::Response;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Duration, timeout};

struct CallbackRecorder {
    sender: mpsc::Sender<String>,
}

#[async_trait]
impl RouteHandler for CallbackRecorder {
    async fn handle(&self, req: HttpRequest) -> Result<HttpResponse, HttpResponse> {
        let body = String::from_utf8_lossy(&req.body).to_string();
        let _ = self.sender.send(body).await;
        Ok(Response::new(full("OK")))
    }
}

struct CallbackReceiver {
    pub port: u16,
    receiver: mpsc::Receiver<String>,
    _shutdown_sender: oneshot::Sender<()>,
}

static CALLBACK_PORT: AtomicU16 = AtomicU16::new(5000);

impl CallbackReceiver {
    async fn start() -> Self {
        let port = CALLBACK_PORT.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = mpsc::channel::<String>(64);
        let (ready_tx, ready_rx) = oneshot::channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let router =
            Arc::new(Router::new().add(Method::POST, "/callback", CallbackRecorder { sender }));

        tokio::spawn(run_server(
            router,
            Arc::new(StdoutLogger::new()),
            port,
            ready_tx,
            async {
                let _ = shutdown_rx.await;
            },
        ));

        ready_rx.await.unwrap();

        Self {
            port,
            receiver,
            _shutdown_sender: shutdown_tx,
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/callback", self.port)
    }

    async fn recv(&mut self) -> Option<String> {
        timeout(Duration::from_secs(2), self.receiver.recv())
            .await
            .ok()
            .flatten()
    }
}

#[tokio::test]
async fn health_check() {
    let harness = TestHarness::start().await;
    let resp = harness.client.get(harness.url("/")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "OK");
}

#[tokio::test]
async fn create_timer_returns_id() {
    let harness = TestHarness::start().await;
    let resp = harness.create_timer(5000, None).await;
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(
        body.starts_with("id: "),
        "expected 'id: <uuid>', got: {body}"
    );
}

#[tokio::test]
async fn invalid_request_rejected() {
    let harness = TestHarness::start().await;
    let resp = harness
        .client
        .post(harness.url("/timer"))
        .body("not json")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn timer_fires_callback_after_interval() {
    let mut cb = CallbackReceiver::start().await;
    let harness = TestHarness::start().await;

    let resp = harness.create_timer(5000, Some(&cb.url())).await;
    assert_eq!(resp.status(), 200);

    harness.clock.advance(5000 + 16);

    let body = cb.recv().await.expect("expected callback to be received");
    assert!(
        body.contains("timer_id"),
        "callback body should contain timer_id: {body}"
    );
}

#[tokio::test]
async fn timer_does_not_fire_before_interval() {
    let mut cb = CallbackReceiver::start().await;
    let harness = TestHarness::start().await;

    harness.create_timer(5000, Some(&cb.url())).await;

    // Advance clock to just before the timer should fire
    harness.clock.advance(5000 - 16);

    // Short wait — if the timer incorrectly fires, the callback would arrive quickly
    let result = timeout(Duration::from_millis(50), cb.receiver.recv()).await;
    assert!(result.is_err(), "timer should not have fired yet");

    // Now advance past the interval
    harness.clock.advance(32);

    let body = cb
        .recv()
        .await
        .expect("expected callback after advancing past interval");
    assert!(body.contains("timer_id"));
}

#[tokio::test]
async fn multiple_timers_fire_at_different_times() {
    let mut cb = CallbackReceiver::start().await;
    let harness = TestHarness::start().await;

    let resp1 = harness.create_timer(100, Some(&cb.url())).await;
    assert_eq!(resp1.status(), 200);

    let resp2 = harness.create_timer(500, Some(&cb.url())).await;
    assert_eq!(resp2.status(), 200);

    // Advance past first timer but not second
    harness.clock.advance(100 + 16);

    let body1 = cb.recv().await.expect("first timer should have fired");
    assert!(body1.contains("timer_id"));

    // Second timer should not have fired yet
    let result = timeout(Duration::from_millis(50), cb.receiver.recv()).await;
    assert!(result.is_err(), "second timer should not have fired yet");

    // Advance past second timer
    harness.clock.advance(400 + 16);

    let body2 = cb.recv().await.expect("second timer should have fired");
    assert!(body2.contains("timer_id"));
}

#[tokio::test]
async fn timer_without_callback_does_not_send_request() {
    let mut cb = CallbackReceiver::start().await;
    let harness = TestHarness::start().await;

    // Create timer without callback
    harness.create_timer(100, None).await;

    harness.clock.advance(100 + 16);

    // Verify the timer expired via logs but no callback was sent
    assert!(harness.logger.contains("Timer expired").await);

    let result = timeout(Duration::from_millis(50), cb.receiver.recv()).await;
    assert!(
        result.is_err(),
        "no callback should be sent for timer without callback_url"
    );
}

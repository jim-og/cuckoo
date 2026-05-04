use cuckoo::core::{Clock, TimeT};
use cuckoo::infra::App;
use cuckoo::utils::{Logger, StdoutLogger};
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use tokio::sync::oneshot;

pub struct FakeClock {
    now: AtomicU64,
}

impl FakeClock {
    pub fn new(start: TimeT) -> Arc<Self> {
        Arc::new(Self {
            now: AtomicU64::new(start),
        })
    }

    pub fn advance(&self, millis: TimeT) {
        self.now.fetch_add(millis, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> TimeT {
        self.now.load(Ordering::SeqCst)
    }
}

pub struct TestHarness {
    pub port: u16,
    pub clock: Arc<FakeClock>,
    pub client: reqwest::Client,
    pub logger: Arc<StdoutLogger>,
    termination_sender: Option<oneshot::Sender<()>>,
    shutdown_sender: Option<oneshot::Sender<()>>,
}

static PORT: AtomicU16 = AtomicU16::new(4000);

impl TestHarness {
    pub async fn start() -> Self {
        let port = PORT.fetch_add(1, Ordering::SeqCst);

        let clock = FakeClock::new(1_000_000);
        let logger = Arc::new(StdoutLogger::new().with_receiver());

        let (termination_sender, termination_receiver) = oneshot::channel();
        let (readiness_sender, readiness_receiver) = oneshot::channel();
        let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

        let app_clock = clock.clone() as Arc<dyn Clock>;
        let app_logger = logger.clone() as Arc<dyn Logger>;

        tokio::spawn(async move {
            let mut app = App::new(app_logger, app_clock, port, async {
                let _ = shutdown_receiver.await;
            })
            .await
            .unwrap();
            app.run(termination_receiver, readiness_sender)
                .await
                .unwrap();
        });

        readiness_receiver.await.unwrap();

        Self {
            port,
            clock,
            client: reqwest::Client::new(),
            logger,
            termination_sender: Some(termination_sender),
            shutdown_sender: Some(shutdown_sender),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    pub async fn create_timer(
        &self,
        interval_ms: u64,
        callback_url: Option<&str>,
    ) -> reqwest::Response {
        let mut payload = serde_json::json!({ "interval_ms": interval_ms });
        if let Some(url) = callback_url {
            payload["callback_url"] = serde_json::json!(url);
        }
        self.client
            .post(self.url("/timer"))
            .json(&payload)
            .send()
            .await
            .unwrap()
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        if let Some(sender) = self.termination_sender.take() {
            let _ = sender.send(());
        }
        if let Some(sender) = self.shutdown_sender.take() {
            let _ = sender.send(());
        }
    }
}

use anyhow::Context;
use tokio::{sync::oneshot, task::JoinHandle};

use crate::utils::App;

pub struct Lifecycle<T: App> {
    app: Option<T>,
    termination_tx: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<anyhow::Result<()>>>,
}

impl<T: App + 'static> Lifecycle<T> {
    pub fn new(app: T) -> Self {
        Self {
            app: Some(app),
            termination_tx: None,
            handle: None,
        }
    }

    pub async fn start(&mut self) -> anyhow::Result<oneshot::Receiver<()>> {
        let mut app = self.app.take().context("Application has already started")?;

        let (termination_tx, termination_rx) = oneshot::channel();
        let (readiness_tx, readiness_rx) = oneshot::channel();

        let handle =
            tokio::task::spawn_local(async move { app.run(termination_rx, readiness_tx).await });

        self.termination_tx = Some(termination_tx);
        self.handle = Some(handle);

        Ok(readiness_rx)
    }

    pub async fn stop(&mut self) {
        if let Some(termination_tx) = self.termination_tx.take() {
            let _ = termination_tx.send(());
        }
    }
}

use crate::{
    infra::timer_app::TimerApp,
    utils::{App, Logger, StdoutLogger},
};
use anyhow::{Context, Result};
use std::{cell::Cell, sync::Arc};
use tokio::sync::oneshot;

pub struct MainProgram {
    // TODO config
    logger: Arc<dyn Logger>,
}

impl MainProgram {
    pub async fn new() -> Result<Self> {
        // load config

        let logger = Arc::new(StdoutLogger);

        Ok(Self { logger })
    }

    pub async fn run(&mut self) -> Result<()> {
        let (termination_sender, termination_receiver) = oneshot::channel();
        let (readiness_sender, readiness_receiver) = oneshot::channel();

        let _ = self.set_ctrlc_handler(termination_sender);

        let logger = self.logger.clone();
        tokio::spawn(async move {
            let _ = readiness_receiver.await;
            logger.info("Application is ready");
        });

        self.run_inner(termination_receiver, readiness_sender).await
    }

    async fn run_inner(
        &mut self,
        termination_receiver: oneshot::Receiver<()>,
        readiness_sender: oneshot::Sender<()>,
    ) -> Result<()> {
        let _ = self.log_startup_banner();

        let mut app = TimerApp::new(self.logger.clone())
            .await
            .context("Failed to create app")?;

        app.run(termination_receiver, readiness_sender)
            .await
            .context("Processor application run failed")?;

        Ok(())
    }

    fn log_startup_banner(&self) -> anyhow::Result<()> {
        self.logger
            .info(&format!("cuckoo - version {}", env!("CARGO_PKG_VERSION")));
        Ok(())
    }

    fn set_ctrlc_handler(&self, termination_sender: oneshot::Sender<()>) -> Result<()> {
        let termination_sender = Cell::new(Some(termination_sender));
        ctrlc::set_handler(move || {
            if let Some(sender) = termination_sender.take() {
                let _ = sender.send(());
            }
        })
        .context("Error setting Ctrl-C handler")?;

        self.logger.info("Press CTRL-C to terminate program");

        Ok(())
    }
}

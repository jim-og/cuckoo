use crate::{
    core::{Clock, EventHandler, Timer},
    infra::event_receiver::EventReceiver,
    utils::Logger,
};
use anyhow::{Context, Result};
use futures::StreamExt;
use std::{future::Future, sync::Arc};
use tokio::sync::{
    mpsc::{self, Receiver},
    oneshot,
};

pub struct App {
    event_handler: EventHandler,
    event_receiver: EventReceiver,
    timer_receiver: Receiver<Timer>,
    logger: Arc<dyn Logger>,
    http_client: reqwest::Client,
}

impl App {
    pub async fn new(
        logger: Arc<dyn Logger>,
        clock: Arc<dyn Clock>,
        port: u16,
        shutdown_signal: impl Future<Output = ()> + Send + 'static,
    ) -> Result<Self> {
        let event_receiver =
            EventReceiver::new(logger.clone(), clock.clone(), port, shutdown_signal).await?;

        let (timer_sender, timer_receiver) = mpsc::channel::<Timer>(1024);

        let event_handler = EventHandler::new(timer_sender, logger.clone(), clock);

        Ok(Self {
            event_handler,
            event_receiver,
            timer_receiver,
            logger,
            http_client: reqwest::Client::new(),
        })
    }

    pub async fn run(
        &mut self,
        termination_receiver: oneshot::Receiver<()>,
        readiness_sender: oneshot::Sender<()>,
    ) -> Result<()> {
        let mut events = self
            .event_receiver
            .take_stream(termination_receiver)
            .context("Missing termination event stream")?;

        readiness_sender
            .send(())
            .map_err(|_| anyhow::anyhow!("Failed to send readiness signal"))?;

        loop {
            tokio::select! {
                // Incoming event
                event = events.next() => {
                    match event {
                        Some(event) => {
                            if let Err(err) = self.event_handler.handle_event(event).await {
                                self.logger.error(&format!("Error handling event {err}"));
                            }
                        },
                        None => {
                            self.logger.info("Event stream terminated");
                            break;
                        },
                    }
                }

                // Expired timer
                Some(timer) = self.timer_receiver.recv() => {
                    self.logger.info("Timer expired, send to client");
                    if let Some(ref url) = timer.callback_url {
                        let url = url.clone();
                        let logger = self.logger.clone();
                        let client = self.http_client.clone();
                        let timer_id = timer.id.uuid().to_string();
                        tokio::spawn(async move {
                            let payload = serde_json::json!({ "timer_id": timer_id });
                            match client.post(&url).json(&payload).send().await {
                                Ok(_) => logger.info(&format!("Callback sent to {}", url)),
                                Err(e) => logger.error(&format!("Callback failed to {}: {}", url, e)),
                            }
                        });
                    }
                }
            }
        }

        Ok(())
    }
}

use crate::{
    core::{Timer, TimerService},
    infra::events::TimerServiceEventSource,
    utils::Logger,
};
use anyhow::{Context, Result};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::{
    mpsc::{self, Receiver},
    oneshot,
};

pub struct TimerApp {
    service: TimerService,
    event_source: TimerServiceEventSource,
    timer_receiver: Receiver<Timer>,
    logger: Arc<dyn Logger>,
}

impl TimerApp {
    pub async fn new(logger: Arc<dyn Logger>) -> Result<Self> {
        let event_source = TimerServiceEventSource::new(logger.clone()).await?;

        // Setup publisher
        let (timer_sender, timer_receiver) = mpsc::channel::<Timer>(1024);

        let service = TimerService::new(timer_sender, logger.clone());

        Ok(Self {
            service,
            event_source,
            timer_receiver,
            logger,
        })
    }

    pub async fn run(
        &mut self,
        termination_receiver: oneshot::Receiver<()>,
        readiness_sender: oneshot::Sender<()>,
    ) -> Result<()> {
        let mut events = self
            .event_source
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
                            if let Err(err) = self.service.handle_event(event).await {
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
                Some (_timer) = self.timer_receiver.recv() => {
                    self.logger.info("Timer expired, send to client");
                }
            }
        }

        Ok(())
    }
}

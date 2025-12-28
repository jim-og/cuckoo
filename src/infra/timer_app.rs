use crate::{
    core::{TimerService, TimerServiceEvent},
    infra::events::TimerServiceEventSource,
    utils::{App, EventHandler, EventSource, Logger},
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::oneshot;

pub struct TimerApp<S: EventSource> {
    service: TimerService,
    event_source: S,
    logger: Arc<dyn Logger>,
}

impl TimerApp<TimerServiceEventSource> {
    pub async fn new(
        // config
        logger: Arc<dyn Logger>,
    ) -> Result<Self> {
        let event_source = TimerServiceEventSource::new(logger.clone()).await?;

        // Setup publisher

        let service = TimerService::new(logger.clone());

        Ok(Self {
            service,
            event_source,
            logger,
        })
    }
}

#[async_trait(?Send)]
impl<S> App for TimerApp<S>
where
    S: EventSource<Event = TimerServiceEvent>,
{
    async fn run(
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

        while let Some(event) = events.next().await {
            if let Err(err) = self.service.handle_event(event).await {
                self.logger.error(&format!("Error handling event {err}"));
            }
        }

        self.logger.info("Event stream terminated");

        Ok(())
    }
}

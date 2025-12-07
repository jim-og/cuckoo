use crate::{core::TimerServiceEvent, utils::EventSource};
use anyhow::Result;
use futures::{Stream, StreamExt};
use std::pin::Pin;

pub type EventStream<T> = Pin<Box<dyn Stream<Item = T>>>;
pub type TimerServiceEventStream = EventStream<TimerServiceEvent>;

pub struct TimerServiceEventSource {
    event_stream: Option<TimerServiceEventStream>,
}

impl TimerServiceEventSource {
    pub async fn new(// config
        // logger
    ) -> Result<Self> {
        // Setup receiver
        // Create stream which maps receiver to TimerService events
        todo!()
    }
}

impl EventSource for TimerServiceEventSource {
    type Event = TimerServiceEvent;
    type EventStream = TimerServiceEventStream;

    fn take_stream(
        &mut self,
        termination: tokio::sync::oneshot::Receiver<()>,
    ) -> Option<Self::EventStream> {
        let stream = self.event_stream.take()?;
        Some(Box::pin(stream.take_until(async move {
            let _ = termination.await;
        })))
    }
}

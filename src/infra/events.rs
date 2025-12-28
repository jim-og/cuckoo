use crate::{
    core::TimerServiceEvent,
    utils::{EventSource, Logger, run_server},
};
use anyhow::Result;
use futures::{Stream, StreamExt};
use hyper::{Method, Request, body::Incoming, rt::Timer};
use std::{pin::Pin, sync::Arc};
use tokio::sync::mpsc::{self, Sender};
use tokio_stream::wrappers::ReceiverStream;

pub type EventStream<T> = Pin<Box<dyn Stream<Item = T>>>;
pub type TimerServiceEventStream = EventStream<TimerServiceEvent>;

pub struct TimerServiceEventSource {
    event_stream: Option<TimerServiceEventStream>,
    _sender: Sender<Request<Incoming>>, // keep sender alive
}

impl TimerServiceEventSource {
    pub async fn new(
        // config
        logger: Arc<dyn Logger>,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Request<Incoming>>(1024);

        // Spawn server
        tokio::spawn(run_server(tx.clone(), logger.clone()));

        // Event mapper
        let stream = ReceiverStream::new(rx).map(|req| match req.method() {
            &Method::GET => TimerServiceEvent::Get,
            &Method::POST => TimerServiceEvent::Add,
            &Method::PUT => TimerServiceEvent::Update,
            &Method::DELETE => TimerServiceEvent::Delete,
            _ => TimerServiceEvent::Unsupported,
        });

        Ok(Self {
            event_stream: Some(Box::pin(stream)),
            _sender: tx,
        })
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

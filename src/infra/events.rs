use crate::{
    core::{TimeT, Timer, TimerId, TimerServiceEvent},
    utils::{EventSource, Logger, run_server},
};
use anyhow::Result;
use chrono::Utc;
use futures::{Stream, StreamExt};
use hyper::{Method, Request, body::Incoming};
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
        let (request_sender, request_receiver) = mpsc::channel::<Request<Incoming>>(1024);

        // Spawn server
        tokio::spawn(run_server(request_sender.clone(), logger.clone()));

        // Event mapper
        let stream = ReceiverStream::new(request_receiver).map(|req| match req.method() {
            &Method::POST => {
                let timer =
                    Timer::new(TimerId::new(), Utc::now().timestamp_millis() as TimeT, 2000);
                TimerServiceEvent::Insert(timer)
            }
            _ => panic!("HTTP method not supported"),
        });

        Ok(Self {
            event_stream: Some(Box::pin(stream)),
            _sender: request_sender,
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

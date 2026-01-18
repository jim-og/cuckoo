use crate::{
    core::{TimeT, Timer, TimerId, TimerServiceEvent},
    utils::{HttpRequest, Logger, run_server},
};
use anyhow::Result;
use chrono::Utc;
use futures::{Stream, StreamExt};
use hyper::Method;
use serde::Deserialize;
use std::{pin::Pin, sync::Arc};
use tokio::sync::{
    mpsc::{self, Sender},
    oneshot,
};
use tokio_stream::wrappers::ReceiverStream;

pub type EventStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;
pub type TimerServiceEventStream = EventStream<TimerServiceEvent>;

pub struct TimerServiceEventSource {
    event_stream: Option<TimerServiceEventStream>,
    _sender: Sender<HttpRequest>, // keep sender alive
}

#[derive(Deserialize)]
struct RequestPayload {
    interval_ms: usize,
}

impl TimerServiceEventSource {
    pub async fn new(logger: Arc<dyn Logger>) -> Result<Self> {
        let (request_sender, request_receiver) = mpsc::channel::<HttpRequest>(1024);

        // Spawn server
        let (ready_sender, ready_receiver) = oneshot::channel();
        tokio::spawn(run_server(
            request_sender.clone(),
            logger.clone(),
            3000,
            ready_sender,
        ));

        // Wait for the server to be ready
        ready_receiver.await?;

        // Event mapper
        let stream = ReceiverStream::new(request_receiver).map(|req| {
            match (req.method, req.path.as_str()) {
                (Method::POST, "/") => {
                    if let Ok(payload) = serde_json::from_slice::<RequestPayload>(&req.body) {
                        // TODO move timestamp to earliest point
                        let timer = Timer::new(
                            TimerId::new(),
                            Utc::now().timestamp_millis() as TimeT,
                            payload.interval_ms,
                        );
                        TimerServiceEvent::Insert(timer)
                    } else {
                        // TODO handle error
                        println!("{:?}", req.body);
                        panic!("Failed to extract payload")
                    }
                }
                // TODO handle error
                _ => panic!("HTTP method not supported"),
            }
        });

        Ok(Self {
            event_stream: Some(Box::pin(stream)),
            _sender: request_sender,
        })
    }

    pub fn take_stream(
        &mut self,
        termination: tokio::sync::oneshot::Receiver<()>,
    ) -> Option<TimerServiceEventStream> {
        let stream = self.event_stream.take()?;
        Some(Box::pin(stream.take_until(async move {
            let _ = termination.await;
        })))
    }
}

use crate::{
    core::TimerServiceEvent,
    infra::handlers::{StatusHandler, TimerHandler},
    utils::{Logger, Router, run_server},
};
use anyhow::Result;
use futures::{Stream, StreamExt};
use hyper::Method;
use std::{pin::Pin, sync::Arc};
use tokio::sync::{
    mpsc::{self},
    oneshot,
};
use tokio_stream::wrappers::ReceiverStream;

pub type EventStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;
pub type TimerServiceEventStream = EventStream<TimerServiceEvent>;

pub struct EventReceiver {
    event_stream: Option<TimerServiceEventStream>,
}

impl EventReceiver {
    pub async fn new(logger: Arc<dyn Logger>) -> Result<Self> {
        let (event_sender, event_receiver) = mpsc::channel::<TimerServiceEvent>(1024);

        // Build router
        let router = Arc::new(Router::new().add(Method::GET, "/", StatusHandler {}).add(
            Method::POST,
            "/timer",
            TimerHandler::new(event_sender.clone()),
        ));

        // Spawn server
        let (ready_sender, ready_receiver) = oneshot::channel();
        tokio::spawn(run_server(
            router,
            logger.clone(),
            3000,
            ready_sender,
            shutdown_signal(),
        ));

        // Wait for the server to be ready
        ready_receiver.await?;

        let stream = ReceiverStream::new(event_receiver);

        Ok(Self {
            event_stream: Some(Box::pin(stream)),
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

async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

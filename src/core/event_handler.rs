use crate::{
    core::{clock::SystemClock, store::Store, timer::Timer},
    utils::Logger,
};
use anyhow::Result;
use std::sync::Arc;
use tokio::{
    sync::mpsc::{self, Sender},
    time::{Duration, Instant, sleep_until},
};

pub enum TimerEvent {
    Insert(Timer),
}

pub struct EventHandler {
    event_sender: mpsc::Sender<TimerEvent>,
}

impl EventHandler {
    pub fn new(timer_sender: Sender<Timer>, logger: Arc<dyn Logger>) -> Self {
        let clock = Arc::new(SystemClock {});
        let store = Store::new(clock.clone());
        let (event_sender, event_receiver) = mpsc::channel::<TimerEvent>(1024);

        tokio::spawn(Self::run(store, event_receiver, timer_sender, logger));

        Self { event_sender }
    }

    pub async fn run(
        mut store: Store,
        mut event_receiver: mpsc::Receiver<TimerEvent>,
        timer_sender: mpsc::Sender<Timer>,
        logger: Arc<dyn Logger>,
    ) {
        loop {
            // TODO get deadline from store
            let next_deadline = Some(Instant::now() + Duration::from_millis(2));

            tokio::select! {
                // New event arrived
                Some(event) = event_receiver.recv() => {
                    logger.info("new event arrived");
                    match event {
                        TimerEvent::Insert(timer) => store.insert(timer),
                    }
                }
                // Timer fired
                _ = async {
                    if let Some(deadline) = next_deadline {
                        sleep_until(deadline).await;
                    } else {
                        futures::future::pending::<()>().await;
                    }
                } => {
                    let bucket = store.pop();
                    for timer in bucket {
                        let _ = timer_sender.send(timer).await;
                    }
                }
                // TODO shutdown signal
                // _ = &mut self.shutdown => break,
            }
        }
    }

    pub async fn handle_event(&mut self, event: TimerEvent) -> Result<()> {
        // TODO handle error
        let _ = self.event_sender.send(event).await;
        Ok(())
    }
}

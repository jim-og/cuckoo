use crate::{
    core::{clock::SystemClock, store::Store, timer::Timer},
    utils::{EventHandler, Logger},
};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::{
    sync::mpsc::{self, Sender},
    time::{Duration, Instant, sleep_until},
};

pub struct TimerService {
    event_sender: mpsc::Sender<TimerServiceEvent>,
}

impl TimerService {
    pub fn new(timer_sender: Sender<Timer>, logger: Arc<dyn Logger>) -> Self {
        let clock = Arc::new(SystemClock {});
        let store = Store::new(clock.clone());
        let (event_sender, event_receiver) = mpsc::channel::<TimerServiceEvent>(1024);

        tokio::spawn(Self::run(store, event_receiver, timer_sender, logger));

        Self { event_sender }
    }

    pub async fn run(
        mut store: Store,
        mut event_receiver: mpsc::Receiver<TimerServiceEvent>,
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
                        TimerServiceEvent::Insert(timer) => store.insert(timer),
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
}

pub enum TimerServiceEvent {
    Insert(Timer),
}

#[async_trait(?Send)]
impl EventHandler for TimerService {
    type Event = TimerServiceEvent;

    async fn handle_event(&mut self, event: Self::Event) -> Result<()> {
        // TODO handle error
        let _ = self.event_sender.send(event).await;
        Ok(())
    }
}

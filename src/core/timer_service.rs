use crate::{
    core::{clock::SystemClock, store::Store},
    utils::{EventHandler, Logger},
};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub struct TimerService {
    store: Store,
    logger: Arc<dyn Logger>,
}

impl TimerService {
    pub fn new(
        // publisher
        // config
        logger: Arc<dyn Logger>,
    ) -> Self {
        let clock = Arc::new(SystemClock {});
        let store = Store::new(clock.clone());
        Self { store, logger }
    }

    async fn get(&self) -> Result<()> {
        // TODO self.store.get(timer);
        self.logger.info("Get timer");
        Ok(())
    }

    async fn add(&self) -> Result<()> {
        // TODO self.store.insert(timer);
        self.logger.info("Add timer");
        Ok(())
    }

    async fn update(&self) -> Result<()> {
        // TODO self.store.update(timer);
        self.logger.info("Update timer");
        Ok(())
    }

    async fn delete(&self) -> Result<()> {
        // TODO self.store.remove(id)
        self.logger.info("Delete timer");
        Ok(())
    }

    async fn unsupported(&self) -> Result<()> {
        self.logger.info("Unsupported event");
        Ok(())
    }
}

pub enum TimerServiceEvent {
    Get,
    Add,
    Update,
    Delete,
    Unsupported,
}

#[async_trait(?Send)]
impl EventHandler for TimerService {
    type Event = TimerServiceEvent;

    async fn handle_event(&mut self, event: Self::Event) -> Result<()> {
        match event {
            TimerServiceEvent::Get => self.get().await,
            TimerServiceEvent::Add => self.add().await,
            TimerServiceEvent::Update => self.update().await,
            TimerServiceEvent::Delete => self.delete().await,
            TimerServiceEvent::Unsupported => self.unsupported().await,
        }
    }
}

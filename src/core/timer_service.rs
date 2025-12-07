use crate::{
    core::{clock::SystemClock, store::Store},
    utils::EventHandler,
};
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

pub struct TimerService {
    store: Store,
}

impl TimerService {
    pub fn new(// publisher
        // config
        // logger
    ) -> Self {
        let clock = Arc::new(SystemClock {});
        let store = Store::new(clock.clone());
        Self { store }
    }

    async fn add_timer(&self) -> anyhow::Result<()> {
        // TODO self.store.insert(timer);
        println!("Add timer");
        Ok(())
    }

    async fn delete_timer(&self) -> anyhow::Result<()> {
        // TODO self.store.remove(id)
        println!("Delete timer");
        Ok(())
    }
}

pub enum TimerServiceEvent {
    AddTimer,
    DeleteTimer,
}

#[async_trait(?Send)]
impl EventHandler for TimerService {
    type Event = TimerServiceEvent;

    async fn handle_event(&mut self, event: Self::Event) -> Result<()> {
        match event {
            TimerServiceEvent::AddTimer => self.add_timer().await,
            TimerServiceEvent::DeleteTimer => self.delete_timer().await,
        }
    }
}

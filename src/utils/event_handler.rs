use anyhow::Result;
use async_trait::async_trait;

#[async_trait(?Send)]
pub trait EventHandler {
    type Event: Send + Sync;

    async fn handle_event(&mut self, event: Self::Event) -> Result<()>;
}

use async_trait::async_trait;
use tokio::sync::oneshot;

#[async_trait(?Send)]
pub trait App {
    async fn run(
        &mut self,
        termination_receiver: oneshot::Receiver<()>,
        readiness_sender: oneshot::Sender<()>,
    ) -> anyhow::Result<()>;
}

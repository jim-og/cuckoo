use futures::Stream;
use tokio::sync::oneshot;

pub trait EventSource {
    type Event;
    type EventStream: Stream<Item = Self::Event> + Unpin;
    fn take_stream(&mut self, termination: oneshot::Receiver<()>) -> Option<Self::EventStream>;
}

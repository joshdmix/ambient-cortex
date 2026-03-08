use cortex_common::events::CortexEvent;
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<CortexEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn publish(&self, event: CortexEvent) -> usize {
        // Returns number of receivers that got the event.
        // Ignore error when there are no receivers.
        self.sender.send(event).unwrap_or(0)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CortexEvent> {
        self.sender.subscribe()
    }
}

use ldk_node::Event;
use tokio::sync::mpsc::Sender;


/// Publisher sends events to subscribers (listeners).
#[derive(Default)]
pub struct NodeEvents {
    pub listeners: Vec<Sender<Event>>,
}

impl NodeEvents {    
    pub fn subscribe(&mut self, listener: Sender<Event>) {
        self.listeners.push(listener);
    }

    pub fn unsubscribe(&mut self, listener: &Sender<Event>) {
        let index = self.listeners.iter().position(|r| r.same_channel(&listener)).unwrap();
        self.listeners.remove(index);
    }

    pub async fn notify(&self, event: Event) {
        let mut invalid_listeners: Vec<&Sender<Event>> = vec![];
        for listener in self.listeners.iter() {
            match listener.send(event.clone()).await {
                Ok(_) => {},
                Err(_) => invalid_listeners.push(listener),
            }
        }

        // Todo remove invalid listeners
    }
}
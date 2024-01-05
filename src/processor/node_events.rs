use amqprs::{
    callbacks::{DefaultChannelCallback, DefaultConnectionCallback},
    channel::{
        BasicPublishArguments, ExchangeDeclareArguments,
    },
    connection::{Connection, OpenConnectionArguments},
    BasicProperties,
};
use anyhow::Result;
use ldk_node::Event;
use tokio::sync::mpsc::Sender;

/// Publisher sends events to subscribers (listeners).
pub struct NodeEvents {
    pub listeners: Vec<Sender<Event>>,
    pub rabbitmq_connection: Option<Connection>,
}

impl NodeEvents {
    pub async fn new(rabbitmq_config: Option<RabbitMqConfig>) -> Self {
        let connection = match rabbitmq_config {
            Some(config) => Some(
                Self::get_rabbitmq_connection(
                    &config.rabbitmq_host,
                    config.rabbitmq_port,
                    &config.rabbitmq_username,
                    &config.rabbitmq_password,
                )
                .await
                .unwrap(),
            ),
            None => None,
        };

        Self {
            listeners: vec![],
            rabbitmq_connection: connection,
        }
    }

    pub fn subscribe(&mut self, listener: Sender<Event>) {
        self.listeners.push(listener);
    }

    pub fn unsubscribe(&mut self, listener: &Sender<Event>) {
        let index = self
            .listeners
            .iter()
            .position(|r| r.same_channel(&listener))
            .unwrap();
        self.listeners.remove(index);
    }

    pub async fn notify(&self, event: Event) {
        let mut invalid_listeners: Vec<&Sender<Event>> = vec![];

        if let Some(connection) = &self.rabbitmq_connection {
            match event {
                Event::PaymentReceived {
                    payment_hash,
                    amount_msat,
                } => {
                    let args =
                        BasicPublishArguments::new("walletka.node", &payment_hash.to_string());

                    let content = String::from(format!(
                        r#"
                            {{
                                "payment_hash": "{}",
                                "amount_msat": {}
                            }}
                        "#,
                        payment_hash, amount_msat
                    ))
                    .into_bytes();

                    let channel = connection.open_channel(None).await.unwrap();

                    channel
                        .basic_publish(BasicProperties::default(), content, args)
                        .await
                        .unwrap();
                }
                _ => {}
            }
        }

        for listener in self.listeners.iter() {
            match listener.send(event.clone()).await {
                Ok(_) => {}
                Err(_) => invalid_listeners.push(listener),
            }
        }

        // Todo remove invalid listeners
    }

    async fn get_rabbitmq_connection(
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<Connection> {
        // open a connection to RabbitMQ server
        let connection = Connection::open(&OpenConnectionArguments::new(
            host, port, username, password,
        ))
        .await
        .unwrap();

        connection
            .register_callback(DefaultConnectionCallback)
            .await
            .unwrap();

        // open a channel on the connection
        let channel = connection.open_channel(None).await.unwrap();
        channel
            .register_callback(DefaultChannelCallback)
            .await
            .unwrap();

        channel
            .exchange_declare(ExchangeDeclareArguments::new("walletka.node", "fanout"))
            .await
            .unwrap();

        Ok(connection)
    }
}

pub struct RabbitMqConfig {
    pub rabbitmq_host: String,
    pub rabbitmq_port: u16,
    pub rabbitmq_username: String,
    pub rabbitmq_password: String,
}

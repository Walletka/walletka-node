use std::{net::SocketAddr, str::FromStr, sync::Arc};

use api::{node_api::node_api::node_server::NodeServer, node_api::NodeService};
use dotenv::dotenv;
use ldk_node::bip39::Mnemonic;
use log::info;
use processor::{node_events::RabbitMqConfig, node_processor};
use tonic::transport::Server;

pub mod api;
pub mod processor;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    env_logger::init();

    let data_dir = std::env::var("DATA_DIR").expect("DATA_DIR must be set!");
    let esplora_server_url =
        std::env::var("ESPLORA_SERVER_URL").expect("ESPLORA_SERVER must be set!");
    let mnemonic: Option<Mnemonic> = match std::env::var("MNEMONIC") {
        Ok(mnemonic) => Some(Mnemonic::from_str(&mnemonic).unwrap()),
        Err(_) => None,
    };

    let rabbitmq_config = match std::env::var("RABBITMQ_HOST") {
        Ok(host) => {
            let rabbitmq_port = std::env::var("RABBITMQ_PORT").expect("RABBITMQ_PORT must be set!");
            let rabbitmq_username =
                std::env::var("RABBITMQ_USERNAME").expect("RABBITMQ_USERNAME must be set!");
            let rabbitmq_password =
                std::env::var("RABBITMQ_PASSWORD").expect("RABBITMQ_PASSWORD must be set!");

            Some(RabbitMqConfig {
                rabbitmq_host: host,
                rabbitmq_port: u16::from_str(&rabbitmq_port).expect("RABBITMQ_PORT is not u16"),
                rabbitmq_username,
                rabbitmq_password,
            })
        }
        Err(_) => None,
    };

    let node_processor = Arc::new(
        node_processor::NodeProcessor::new(data_dir, esplora_server_url, mnemonic, rabbitmq_config)
            .await?,
    );

    node_processor.start()?;

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let node_service = NodeServer::new(NodeService {
        node: node_processor.clone(),
    });

    info!("Starting grpc server at :3000");

    Server::builder()
        .accept_http1(true)
        .add_service(tonic_web::enable(node_service))
        .serve(addr)
        .await?;
    Ok(())
}

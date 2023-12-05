use std::{net::SocketAddr, str::FromStr, sync::Arc};

use api::{node_api::node_api::node_server::NodeServer, node_api::NodeService};
use dotenv::dotenv;
use ldk_node::bip39::Mnemonic;
use log::info;
use processor::node_processor;
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
    let mnemonic = std::env::var("MNEMONIC");
    let mnemonic: Option<Mnemonic> = if mnemonic.is_ok() {
        Some(Mnemonic::from_str(&mnemonic.unwrap()).unwrap())
    } else {
        None
    };

    let node_processor = Arc::new(node_processor::NodeProcessor::new(
        data_dir,
        esplora_server_url,
        mnemonic,
    )?);
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

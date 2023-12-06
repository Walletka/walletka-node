use std::{str::FromStr, sync::Arc, time::Duration};

use anyhow::{bail, Error, Result};
use ldk_node::{
    bip39::Mnemonic,
    bitcoin::{
        secp256k1::{
            rand::{rngs::OsRng, RngCore},
            PublicKey,
        },
        Address, Network,
    },
    io::sqlite_store::SqliteStore,
    lightning::ln::{msgs::SocketAddress, ChannelId, PaymentHash},
    lightning_invoice::Bolt11Invoice,
    Builder, ChannelConfig, ChannelDetails, Event, Node, NodeError, PeerDetails,
};
use log::info;
use tokio::{sync::Mutex, time::sleep};

use super::node_events::NodeEvents;

pub struct NodeProcessor {
    node: Arc<Node<SqliteStore>>,
    pub events: Arc<Mutex<NodeEvents>>,
}

impl NodeProcessor {
    pub fn new(
        data_dir: String,
        esplora_server: String,
        mnemonic: Option<Mnemonic>,
    ) -> Result<Self, Error> {
        let mut builder = Builder::new();
        builder.set_network(Network::Regtest);
        builder.set_storage_dir_path(format!("{}/ldk_node", data_dir).to_string());
        builder.set_log_dir_path(format!("{}/ldk_node", data_dir).to_string());
        builder.set_log_level(ldk_node::LogLevel::Debug);
        builder.set_listening_addresses(vec![SocketAddress::from_str("0.0.0.0:9876").unwrap()])?;
        if mnemonic.is_some() {
            builder.set_entropy_bip39_mnemonic(mnemonic.unwrap(), None);
        }
        builder.set_esplora_server(esplora_server);
        builder.set_gossip_source_p2p();
        //builder.set_gossip_source_rgs(
        //    "https://rapidsync.lightningdevkit.org/testnet/snapshot".to_string(),
        //);

        let node = Arc::new(builder.build()?);
        let events = Arc::new(Mutex::new(NodeEvents::default()));

        Ok(Self { node, events })
    }

    pub fn get_id(&self) -> PublicKey {
        self.node.node_id()
    }

    pub fn start(&self) -> Result<(), Error> {
        self.subscribe_events();
        info!("Starting lightning node");
        Ok(self.node.start()?)
    }

    pub fn get_channels(&self) -> Vec<ChannelDetails> {
        self.node.list_channels()
    }

    pub fn connect_peer(
        &self,
        node_id: PublicKey,
        address: SocketAddress,
        persist: bool,
    ) -> Result<()> {
        Ok(self.node.connect(node_id, address, persist)?)
    }

    pub fn get_peers(&self) -> Vec<PeerDetails> {
        self.node.list_peers()
    }

    pub fn open_channel(
        &self,
        node_id: PublicKey,
        address: Option<SocketAddress>,
        channel_amount_sats: u64,
        push_to_counterparty_msat: Option<u64>,
        public: bool,
    ) -> Result<()> {
        let channel_config = Arc::new(ChannelConfig::new());
        let address = if address.is_none() {
            let peer = self
                .get_peers()
                .into_iter()
                .find(|p| p.node_id == node_id)
                .expect("Peer is not connected, provide address!");
            peer.address
        } else {
            address.unwrap()
        };

        Ok(self.node.connect_open_channel(
            node_id,
            address,
            channel_amount_sats,
            push_to_counterparty_msat,
            Some(channel_config),
            public,
        )?)
    }

    pub async fn close_channel(&self, channel_id: ChannelId) -> Result<()> {
        match self
            .get_channels()
            .iter()
            .find(|c| c.channel_id == channel_id)
        {
            Some(channel) => Ok(self
                .node
                .close_channel(&channel_id, channel.counterparty_node_id)?),
            None => bail!(NodeError::ChannelClosingFailed),
        }
    }

    pub fn new_onchain_address(&self) -> Result<Address> {
        Ok(self.node.new_onchain_address()?)
    }

    pub fn create_bolt11_invoice(
        &self,
        amount_msat: Option<u64>,
        description: &str,
        expiry_secs: u32,
    ) -> Result<Bolt11Invoice> {
        match amount_msat {
            Some(amount) => Ok(self
                .node
                .receive_payment(amount, description, expiry_secs)?),
            None => Ok(self
                .node
                .receive_variable_amount_payment(description, expiry_secs)?),
        }
    }

    pub fn pay_invoice(
        &self,
        invoice: &Bolt11Invoice,
        amount_msat: Option<u64>,
    ) -> Result<PaymentHash> {
        match amount_msat {
            Some(amount_msat) => {
                match self
                    .node
                    .send_payment_probes_using_amount(invoice, amount_msat)
                {
                    Ok(_) => Ok(self.node.send_payment_using_amount(invoice, amount_msat)?),
                    Err(err) => Err(err.into()),
                }
            }
            None => match self.node.send_payment_probes(invoice) {
                Ok(_) => Ok(self.node.send_payment(invoice)?),
                Err(err) => Err(err.into()),
            },
        }
    }

    pub fn send_keysend_payment(
        &self,
        destination: PublicKey,
        amount_msat: u64,
    ) -> Result<PaymentHash> {
        match self
            .node
            .send_spontaneous_payment_probes(amount_msat, destination)
        {
            Ok(_) => Ok(self
                .node
                .send_spontaneous_payment(amount_msat, destination)?),
            Err(err) => Err(err.into()),
        }
    }

    fn subscribe_events(&self) {
        let node = self.node.clone();
        let events = self.events.clone();
        tokio::spawn(async move {
            loop {
                match node.next_event() {
                    Some(event) => {
                        let events = events.lock().await;
                        println!("New event: {:?}", event);
                        node.event_handled();
                        events.notify(event).await;
                    }
                    None => {
                        sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        });
    }

    pub async fn trigger_payment_event(&self, payment_hash: Option<String>) {
        let events = self.events.lock().await;
        let fake_hash = if payment_hash.is_none() {
            let mut fake_hash = [0; 32];
            OsRng.fill_bytes(&mut fake_hash);
            PaymentHash(fake_hash)
        } else {
            let hash =
                ldk_node::bitcoin::hashes::sha256::Hash::from_str(payment_hash.unwrap().as_str())
                    .unwrap();
            let mut fake_hash = [0; 32];
            fake_hash.copy_from_slice(hash.to_vec().as_slice());
            PaymentHash(fake_hash)
        };
        events
            .notify(Event::PaymentReceived {
                payment_hash: fake_hash,
                amount_msat: 350000,
            })
            .await;
    }
}

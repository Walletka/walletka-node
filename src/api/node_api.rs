use std::{str::FromStr, sync::Arc};

use ldk_node::{
    bitcoin::secp256k1::PublicKey, lightning::ln::msgs::SocketAddress,
    lightning_invoice::Bolt11Invoice,
};
use node_api::*;
use tonic::{Request, Response, Status};

use crate::processor::node_processor::NodeProcessor;

pub mod node_api {
    tonic::include_proto!("node_api_service");
}

pub struct NodeService {
    pub node: Arc<NodeProcessor>,
}

#[tonic::async_trait]
impl node_server::Node for NodeService {
    async fn get_info(&self, _: Request<()>) -> Result<Response<GetInfoResponse>, Status> {
        Ok(Response::new(GetInfoResponse {
            node_id: self.node.get_id().to_string(),
            running: true,
            onchain_address: self.node.new_onchain_address().unwrap().to_string(),
        }))
    }

    async fn get_channels(
        &self,
        _: Request<GetChannelsRequest>,
    ) -> Result<Response<GetChannelsResponse>, Status> {
        let channels = self.node.get_channels();
        let channels: Vec<ChannelDetailsMessage> = channels
            .iter()
            .map(|c| ChannelDetailsMessage {
                channel_id: c.channel_id.to_string(),
                counterparty_node_id: c.counterparty_node_id.to_string(),
                channel_value_sats: c.channel_value_sats,
                unspendable_punishment_reserve: c
                    .unspendable_punishment_reserve
                    .unwrap_or_default(),
                feerate_sat_per_1000_weight: c.feerate_sat_per_1000_weight,
                balance_msat: c.balance_msat,
                outbound_capacity_msat: c.outbound_capacity_msat,
                inbound_capacity_msat: c.inbound_capacity_msat,
                confirmations_required: c.confirmations_required.unwrap_or_default(),
                confirmations: c.confirmations.unwrap_or_default(),
                is_ready: c.is_channel_ready,
                is_usable: c.is_usable,
                is_public: c.is_public,
                inbound_htlc_minimum_msat: c.inbound_htlc_minimum_msat,
                inbound_htlc_maximum_msat: c.inbound_htlc_maximum_msat.unwrap_or_default(),
            })
            .collect();

        Ok(Response::new(GetChannelsResponse { channels }))
    }

    async fn open_channel(
        &self,
        request: Request<OpenChannelRequest>,
    ) -> Result<Response<OpenChannelResponse>, Status> {
        let r: OpenChannelRequest = request.into_inner();

        let address = if r.address.len() > 1 {
            Some(SocketAddress::from_str(&r.address).unwrap())
        } else {
            None
        };

        match self.node.open_channel(
            PublicKey::from_str(&r.node_id).unwrap(),
            address,
            r.channel_amount_sats,
            Some(r.push_to_counterparty_msat),
            r.public,
        ) {
            Ok(_) => Ok(Response::new(OpenChannelResponse {})),
            Err(err) => Err(Status::new(tonic::Code::Unknown, err.to_string())),
        }
    }

    async fn close_channel(
        &self,
        request: Request<CloseChannelRequest>,
    ) -> Result<Response<CloseChannelResponse>, Status> {
        let r: CloseChannelRequest = request.into_inner();

        match self
            .node
            .get_channels()
            .iter()
            .find(|c| c.channel_id.to_string() == r.channel_id)
        {
            Some(channel) => match self.node.close_channel(channel.channel_id).await {
                Ok(_) => Ok(Response::new(CloseChannelResponse {})),
                Err(err) => Err(Status::new(tonic::Code::Unknown, err.to_string())),
            },
            None => todo!(),
        }
    }

    async fn create_bolt11_invoice(
        &self,
        request: Request<CreateBolt11InvoiceRequest>,
    ) -> Result<Response<CreateBolt11InvoiceResponse>, Status> {
        let r = request.into_inner();
        let bolt11_invoice = if r.amount_msat > 0 {
            self.node
                .create_bolt11_invoice(Some(r.amount_msat), &r.description, r.expiry_secs)
                .unwrap()
        } else {
            self.node
                .create_bolt11_invoice(None, &r.description, r.expiry_secs)
                .unwrap()
        };

        Ok(Response::new(CreateBolt11InvoiceResponse {
            invoice: bolt11_invoice.to_string(),
        }))
    }

    async fn pay_invoice(
        &self,
        request: Request<PayInvoiceRequest>,
    ) -> Result<Response<PayInvoiceResponse>, Status> {
        let r = request.into_inner();
        let bolt11_invoice = Bolt11Invoice::from_str(&r.bolt11_invoice).unwrap();

        if bolt11_invoice.amount_milli_satoshis().is_some() {
            self.node.pay_invoice(&bolt11_invoice, None).unwrap()
        } else if r.amount_msat > 0 {
            self.node
                .pay_invoice(&bolt11_invoice, Some(r.amount_msat))
                .unwrap()
        } else {
            return Err(Status::new(tonic::Code::Aborted, "Invalid amount"));
        };

        Ok(Response::new(PayInvoiceResponse {}))
    }

    async fn trigger_payment_event(
        &self,
        request: Request<TriggerPaymentEventRequest>,
    ) -> Result<Response<()>, Status> {
        let r = request.into_inner();
        let payment_hash = if r.payment_hash.len() > 1 {
            Some(r.payment_hash)
        } else {
            None
        };

        self.node.trigger_payment_event(payment_hash).await;
        Ok(Response::new(()))
    }

    async fn send_keysend_payment(
        &self,
        request: Request<SendKeysendPaymentRequest>,
    ) -> Result<Response<SendKeysendPaymentResponse>, Status> {
        let r = request.into_inner();
        match self
            .node
            .send_keysend_payment(PublicKey::from_str(&r.destination).unwrap(), r.amount)
        {
            Ok(payment_hash) => Ok(Response::new(SendKeysendPaymentResponse {
                payment_hash: payment_hash.to_string(),
            })),
            Err(err) => Err(Status::new(tonic::Code::Unknown, err.to_string())),
        }
    }
}

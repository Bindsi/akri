use akri_discovery_utils::discovery::{
    v0::{discovery_handler_server::DiscoveryHandler, Device, DiscoverRequest, DiscoverResponse},
    DiscoverStream,
};
use async_trait::async_trait;
use tokio::sync::mpsc;
use tonic::{Response, Status};
use anyhow::Error;
use reqwest::get;
use std::collections::HashMap;

const BROKER_NAME: &str = "AKRI_KEPSERVEREX";
const DEVICE_ENDPOINT: &str = "AKRI_HTTP_DEVICE_ENDPOINT";
pub struct DiscoveryHandlerImpl {
    register_sender: tokio::sync::mpsc::Sender<()>,
}

impl DiscoveryHandlerImpl {
    pub fn new(register_sender: tokio::sync::mpsc::Sender<()>) -> Self {
        DiscoveryHandlerImpl { register_sender }
    }
}

#[async_trait]
impl DiscoveryHandler for DiscoveryHandlerImpl {
    type DiscoverStream = DiscoverStream;
    async fn discover(
        &self,
        request: tonic::Request<DiscoverRequest>,
    ) -> Result<Response<Self::DiscoverStream>, Status> {
        // Get the discovery url from the `DiscoverRequest`
        let url = request.get_ref().discovery_details.clone();
        // Create a channel for sending and receiving device updates
        let (stream_sender, stream_receiver) = mpsc::channel(4);
        let register_sender = self.register_sender.clone();
        tokio::spawn(async move {
            loop {
                let resp = get(&url).await.unwrap(); 
                // Response is a newline separated list of devices (host:port) or empty
                let device_list = &resp.text().await.unwrap();
                let devices = device_list
                    .lines()
                    .map(|endpoint| {
                        let mut properties = HashMap::new();
                        properties.insert(BROKER_NAME.to_string(), "http".to_string());
                        properties.insert(DEVICE_ENDPOINT.to_string(), endpoint.to_string());
                        Device {
                            id: endpoint.to_string(),
                            properties,
                            mounts: Vec::default(),
                            device_specs: Vec::default(),
                        }
                    })
                    .collect::<Vec<Device>>();
                // Send the Agent the list of devices.
                if let Err(_) = stream_sender.send(Ok(DiscoverResponse { devices })).await {
                    // Agent dropped its end of the stream. Stop discovering and signal to try to re-register.
                    register_sender.send(()).await.unwrap();
                    break;
                }
            }
        });
        // Send the agent one end of the channel to receive device updates
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            stream_receiver,
        )))
    }
}

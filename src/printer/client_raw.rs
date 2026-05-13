use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use serde_json::Value;
use tokio::sync::{broadcast, watch, RwLock};
use tracing::{debug, error, info, warn};

use super::commands::Command;
use super::models::{
    DeviceInfo, RegisterRequest, RegisterResponse, RpcResponse,
    METHOD_GET_DEVICE_INFO,
};
use super::state::PrinterState;
use crate::error::PrinterError;

pub struct MqttRawClient;

impl MqttRawClient {
    pub async fn connect_and_run(
        ip: &str,
        printer_id: &str,
        username: &str,
        password: &str,
        client_id: &str,
        state: Arc<RwLock<PrinterState>>,
        connected_tx: watch::Sender<bool>,
        state_changed_tx: broadcast::Sender<()>,
        cmd_rx: broadcast::Receiver<Command>,
        mut shutdown: watch::Receiver<bool>,
    ) -> Result<(), PrinterError> {
        let mut mqtt_options = MqttOptions::new(client_id, ip, 1883);
        mqtt_options.set_credentials(username, password);
        mqtt_options.set_keep_alive(Duration::from_secs(60));
        mqtt_options.set_clean_session(true);

        let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10);
        let id_counter = Arc::new(AtomicU64::new(1));

        info!("raw client connecting to {ip}:1883");

        Self::run_event_loop(
            client,
            &mut eventloop,
            printer_id,
            client_id,
            state,
            connected_tx,
            state_changed_tx,
            id_counter,
            cmd_rx,
            &mut shutdown,
        )
        .await
    }

    async fn run_event_loop(
        client: AsyncClient,
        eventloop: &mut rumqttc::EventLoop,
        printer_id: &str,
        client_id: &str,
        state: Arc<RwLock<PrinterState>>,
        connected_tx: watch::Sender<bool>,
        state_changed_tx: broadcast::Sender<()>,
        id_counter: Arc<AtomicU64>,
        mut cmd_rx: broadcast::Receiver<Command>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<(), PrinterError> {
        let register_response_topic = format!("elegoo/{printer_id}/{client_id}_req/register_response");
        let api_response_topic = format!("elegoo/{printer_id}/{client_id}/api_response");
        let api_request_topic = format!("elegoo/{printer_id}/{client_id}/api_request");
        let api_register_topic = format!("elegoo/{printer_id}/api_register");

        client.subscribe(&register_response_topic, QoS::AtLeastOnce).await
            .map_err(|e| PrinterError::Registration(format!("subscribe failed: {e}")))?;
        client.subscribe(&api_response_topic, QoS::AtLeastOnce).await
            .map_err(|e| PrinterError::Registration(format!("subscribe failed: {e}")))?;

        let register_payload = serde_json::to_string(&RegisterRequest {
            client_id: client_id.to_string(),
            request_id: format!("{client_id}_req"),
        })
        .map_err(PrinterError::Json)?;

        client
            .publish(&api_register_topic, QoS::AtLeastOnce, false, register_payload)
            .await
            .map_err(|e| PrinterError::Registration(format!("register publish failed: {e}")))?;

        debug!("raw client sent registration, waiting for response");

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("raw client shutting down");
                        client.disconnect().await.ok();
                        connected_tx.send(false).ok();
                        state_changed_tx.send(()).ok();
                        return Ok(());
                    }
                }

                result = cmd_rx.recv() => {
                    match result {
                        Ok(Command { id, method, params }) => {
                            let req = match params {
                                Some(p) => serde_json::json!({"id": id, "method": method, "params": p}),
                                None => serde_json::json!({"id": id, "method": method}),
                            };
                            if let Ok(payload) = serde_json::to_string(&req) {
                                debug!("[raw-cmd] sending method {method}");
                                if let Err(e) = client.publish(&api_request_topic, QoS::AtLeastOnce, false, payload).await {
                                    warn!("[raw-cmd] publish method {method} failed: {e}");
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            debug!("raw cmd channel lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("raw cmd channel closed");
                            break;
                        }
                    }
                }

                notification = eventloop.poll() => {
                    match notification {
                        Ok(Event::Incoming(Incoming::Publish(publish))) => {
                            let topic = &publish.topic;
                            let payload = String::from_utf8_lossy(&publish.payload);

                            if topic == &register_response_topic {
                                if let Ok(resp) = serde_json::from_str::<RegisterResponse>(&payload) {
                                    if resp.error == "ok" {
                                        info!("raw client registered (client_id={client_id})");
                                        if connected_tx.send(true).is_err() {
                                            warn!("[raw] connected_tx watcher dropped on connect");
                                        }
                                        state_changed_tx.send(()).ok();

                                        Self::send_method(&client, &id_counter, &api_request_topic,
                                            METHOD_GET_DEVICE_INFO, Some(serde_json::json!({}))).await;
                                    } else {
                                        error!("raw client registration failed: {}", resp.error);
                                        return Err(PrinterError::Registration(resp.error));
                                    }
                                }

                            } else if topic == &api_response_topic {
                                if let Ok(resp) = serde_json::from_str::<RpcResponse>(&payload) {
                                    debug!("[raw] api_response: method={}, error_code={}",
                                        resp.method, resp.result.error_code);

                                    if resp.method == METHOD_GET_DEVICE_INFO && resp.result.error_code == 0 {
                                        if let Ok(dev) = serde_json::from_value::<DeviceInfo>(resp.result.data.clone()) {
                                            let model = dev.machine_model.clone();
                                            state.write().await.device_info = Some(dev);
                                            info!("[raw] device info: model={model}");
                                        }
                                    }
                                } else {
                                    debug!("[raw] api_response parse failed: {}",
                                        payload.chars().take(200).collect::<String>());
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(e) => {
                            error!("raw client eventloop error: {e}");
                            connected_tx.send(false).ok();
                            state_changed_tx.send(()).ok();
                            return Err(PrinterError::Mqtt(e));
                        }
                    }
                }
            }
        }

        connected_tx.send(false).ok();
        state_changed_tx.send(()).ok();
        Ok(())
    }

    async fn send_method(
        client: &AsyncClient,
        id_counter: &Arc<AtomicU64>,
        topic: &str,
        method: u16,
        params: Option<Value>,
    ) {
        let id = id_counter.fetch_add(1, Ordering::SeqCst);
        let req = match params {
            Some(p) => serde_json::json!({"id": id, "method": method, "params": p}),
            None => serde_json::json!({"id": id, "method": method}),
        };
        if let Ok(payload) = serde_json::to_string(&req) {
            if let Err(e) = client.publish(topic, QoS::AtLeastOnce, false, payload).await {
                warn!("[raw] send_method {method} publish failed: {e}");
            }
        }
    }
}

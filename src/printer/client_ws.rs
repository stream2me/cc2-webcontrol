use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, watch, RwLock};
use tokio::time::{interval, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, trace, warn};

use super::commands::{Command, PendingRpcs};
use super::models::{FullStatus, RpcResponse, METHOD_GET_AMS_INFO, METHOD_GET_FILE_LIST, METHOD_GET_FILE_THUMBNAIL, METHOD_GET_FULL_STATUS, METHOD_STATUS_PUSH};
use super::state::{PrinterState, PrintState};
use crate::error::PrinterError;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct MqttWsClient;

impl MqttWsClient {
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
        pending_rpcs: PendingRpcs,
    ) -> Result<(), PrinterError> {
        info!("ws client connecting to {ip}:9001");

        let url = format!("ws://{ip}:9001/");
        let mut request = url
            .into_client_request()
            .map_err(|e| PrinterError::WebSocket(format!("invalid ws url: {e}")))?;

        let proto_value = "mqttv3.1".parse().map_err(|e| {
            PrinterError::WebSocket(format!("invalid ws subprotocol header: {e}"))
        })?;
        request.headers_mut().insert("Sec-WebSocket-Protocol", proto_value);

        let (ws_stream, _) = tokio_tungstenite::connect_async_with_config(request, None, true)
            .await
            .map_err(|e| PrinterError::WebSocket(format!("ws connection failed: {e}")))?;

        info!("ws client TCP connected, starting MQTT session");

        Self::run_session(
            ws_stream,
            printer_id,
            client_id,
            username,
            password,
            state,
            connected_tx,
            state_changed_tx,
            cmd_rx,
            &mut shutdown,
            pending_rpcs,
        )
        .await
    }

    async fn run_session(
        ws_stream: WsStream,
        printer_id: &str,
        client_id: &str,
        username: &str,
        password: &str,
        state: Arc<RwLock<PrinterState>>,
        connected_tx: watch::Sender<bool>,
        state_changed_tx: broadcast::Sender<()>,
        mut cmd_rx: broadcast::Receiver<Command>,
        shutdown: &mut watch::Receiver<bool>,
        pending_rpcs: PendingRpcs,
    ) -> Result<(), PrinterError> {
        let (mut write, mut read) = ws_stream.split();
        let mqtt = MqttOverWs::new();
        let mut id_seq: u64 = 70;

        mqtt.send_connect(&mut write, client_id, username, password).await?;

        let code = timeout(Duration::from_secs(5), mqtt.wait_for_connack(&mut read))
            .await
            .map_err(|_| PrinterError::WebSocket("CONNACK timeout".to_string()))??;

        if code != 0 {
            let reason = match code {
                1 => "bad credentials",
                2 => "client id not allowed",
                3 => "server unavailable",
                _ => "rejected",
            };
            error!("[ws] CONNACK rejected: code {code} ({reason})");
            return Err(PrinterError::WebSocket(format!("CONNACK rejected: code {code} ({reason})")));
        }
        debug!("ws client CONNACK accepted (code=0)");

        let api_response_topic = format!("elegoo/{printer_id}/{client_id}/api_response");
        let api_status_topic = format!("elegoo/{printer_id}/api_status");
        let api_request_topic = format!("elegoo/{printer_id}/{client_id}/api_request");
        let register_response_topic = format!("elegoo/{printer_id}/{client_id}_req/register_response");
        let api_register_topic = format!("elegoo/{printer_id}/api_register");

        mqtt.subscribe(
            &mut write,
            &[
                (&register_response_topic, 1),
                (&api_response_topic, 1),
                (&api_status_topic, 0),
            ],
        )
        .await?;

        // ws register gate
        let reg_payload = serde_json::json!({
            "client_id": client_id,
            "request_id": format!("{client_id}_req"),
        });
        if let Ok(payload) = serde_json::to_vec(&reg_payload) {
            mqtt.publish(&mut write, &api_register_topic, &payload).await?;
        }
        debug!("ws registration sent, waiting for confirmation");

        let mut registered = false;
        let mut heartbeat = interval(Duration::from_secs(10));
        let mut pre_reg_queue: Vec<Command> = Vec::with_capacity(12);
        let mut last_status_push = tokio::time::Instant::now();
        // one in-flight thumbnail request at a time
        let mut pending_thumb: Option<String> = None;
        let mut last_thumb_req: Option<tokio::time::Instant> = None;

        // reconnect if no frames for 30s
        let watchdog = tokio::time::sleep(Duration::from_secs(30));
        tokio::pin!(watchdog);

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        info!("ws client shutting down");
                        connected_tx.send(false).ok();
                        state_changed_tx.send(()).ok();
                        Self::drain_pending_rpcs(&pending_rpcs).await;
                        return Ok(());
                    }
                }

                _ = &mut watchdog => {
                    warn!("[ws] receive watchdog expired (30s no frames), reconnecting");
                    connected_tx.send(false).ok();
                    state_changed_tx.send(()).ok();
                    Self::drain_pending_rpcs(&pending_rpcs).await;
                    return Err(PrinterError::WebSocket("receive timeout".to_string()));
                }

                _ = heartbeat.tick() => {
                    if registered {
                        if let Ok(payload) = serde_json::to_vec(&serde_json::json!({"type": "PING"})) {
                            if let Err(e) = mqtt.publish(&mut write, &api_request_topic, &payload).await {
                                warn!("[ws] heartbeat publish failed: {e}");
                            }
                        }
                        // status push quiet too long; force full snapshot
                        if last_status_push.elapsed() >= Duration::from_secs(60) {
                            debug!("[ws] no status push in 60s, requesting full status refresh");
                            let req = serde_json::json!({"id": id_seq, "method": METHOD_GET_FULL_STATUS});
                            id_seq += 1;
                            if let Ok(p) = serde_json::to_vec(&req) {
                                if let Err(e) = mqtt.publish(&mut write, &api_request_topic, &p).await {
                                    warn!("[ws] status refresh publish failed: {e}");
                                }
                            }
                        }
                    }
                }

                result = cmd_rx.recv() => {
                    match result {
                        Ok(Command { id, method, params }) => {
                            if !registered {
                                if pre_reg_queue.len() < 12 {
                                    pre_reg_queue.push(Command { id, method, params });
                                    debug!("[ws-cmd] queued method {method} (pre-registration, {} queued)", pre_reg_queue.len());
                                } else {
                                    debug!("[ws-cmd] dropping method {method} - pre-reg queue full");
                                }
                            } else {
                                let req = match params {
                                    Some(p) => serde_json::json!({"id": id, "method": method, "params": p}),
                                    None => serde_json::json!({"id": id, "method": method}),
                                };
                                if let Ok(payload) = serde_json::to_vec(&req) {
                                    debug!("[ws-cmd] sending method {method} id={id}");
                                    if let Err(e) = mqtt.publish(&mut write, &api_request_topic, &payload).await {
                                        warn!("[ws-cmd] publish method {method} failed: {e}");
                                    }
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            debug!("ws cmd channel lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("ws cmd channel closed");
                            break;
                        }
                    }
                }

                msg = read.next() => {
                    match msg {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(data))) => {
                            watchdog.as_mut().reset(tokio::time::Instant::now() + Duration::from_secs(30));
                            if let Some(packet) = mqtt.parse_packet(&data) {
                                match packet {
                                    WsPacket::Publish { topic, payload, ack_id } => {
                                        // must PUBACK qos1 msgs or broker retry queue stalls
                                        // stalled retry queue blocks new responses
                                        if let Some(pid) = ack_id {
                                            if let Err(e) = mqtt.puback(&mut write, pid).await {
                                                warn!("[ws] PUBACK failed: {e}");
                                            }
                                        }
                                        // heartbeat for full-status refresh path
                                        if topic == api_status_topic {
                                            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&payload) {
                                                if v.get("method").and_then(|m| m.as_u64()) == Some(METHOD_STATUS_PUSH as u64) {
                                                    last_status_push = tokio::time::Instant::now();
                                                }
                                            }
                                        }
                                        if topic == register_response_topic {

                                            if let Ok(val) = serde_json::from_slice::<Value>(&payload) {
                                                if val.get("error").and_then(|e| e.as_str()) == Some("ok") {
                                                    registered = true;
                                                    if connected_tx.send(true).is_err() {
                                                        warn!("[ws] connected_tx watcher dropped on connect");
                                                    }
                                                    state_changed_tx.send(()).ok();
                                                    info!("ws client ready (client_id={client_id})");

                                                    let req = serde_json::json!({"id": id_seq, "method": METHOD_GET_FULL_STATUS});
                                                    id_seq += 1;
                                                    if let Ok(p) = serde_json::to_vec(&req) {
                                                        if let Err(e) = mqtt.publish(&mut write, &api_request_topic, &p).await {
                                                            warn!("[ws] initial full-status publish failed: {e}");
                                                        }
                                                    }
                                                    let req_ams = serde_json::json!({"id": id_seq, "method": METHOD_GET_AMS_INFO, "params": {}});
                                                    id_seq += 1;
                                                    if let Ok(p) = serde_json::to_vec(&req_ams) {
                                                        if let Err(e) = mqtt.publish(&mut write, &api_request_topic, &p).await {
                                                            warn!("[ws] initial ams-info publish failed: {e}");
                                                        }
                                                    }
                                                    let req_files = serde_json::json!({"id": id_seq, "method": METHOD_GET_FILE_LIST,
                                                        "params": {"storage_media": "local", "pageNumber": 1, "pageSize": 50}});
                                                    id_seq += 1;
                                                    if let Ok(p) = serde_json::to_vec(&req_files) {
                                                        if let Err(e) = mqtt.publish(&mut write, &api_request_topic, &p).await {
                                                            warn!("[ws] initial file-list publish failed: {e}");
                                                        }
                                                    }
                                                    for queued in pre_reg_queue.drain(..) {
                                                        let req = match queued.params {
                                                            Some(p) => serde_json::json!({"id": queued.id, "method": queued.method, "params": p}),
                                                            None => serde_json::json!({"id": queued.id, "method": queued.method}),
                                                        };
                                                        if let Ok(p) = serde_json::to_vec(&req) {
                                                            if let Err(e) = mqtt.publish(&mut write, &api_request_topic, &p).await {
                                                                warn!("[ws-cmd] pre-reg drain method {} failed: {e}", queued.method);
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    error!("ws registration rejected: {val}");
                                                    return Err(PrinterError::WebSocket("registration rejected".to_string()));
                                                }
                                            }
                                        } else {
                                            if let Some(follow_up) = Self::handle_publish(
                                                &topic,
                                                &payload,
                                                &api_status_topic,
                                                &api_response_topic,
                                                &state,
                                                &state_changed_tx,
                                                &pending_rpcs,
                                                &mut pending_thumb,
                                                &mut last_thumb_req,
                                            )
                                            .await {
                                                let req = serde_json::json!({
                                                    "id": id_seq,
                                                    "method": follow_up["method"],
                                                    "params": follow_up["params"],
                                                });
                                                id_seq += 1;
                                                if let Ok(p) = serde_json::to_vec(&req) {
                                                    if let Err(e) = mqtt.publish(&mut write, &api_request_topic, &p).await {
                                                        warn!("[ws] thumb prefetch failed: {e}");
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    WsPacket::PubAck => trace!("ws PUBACK"),
                                    WsPacket::SubAck => trace!("ws SUBACK"),
                                    WsPacket::ConnAck(c) => trace!("ws extra CONNACK: {c}"),
                                }
                            }
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                            warn!("[ws] server closed connection");
                            connected_tx.send(false).ok();
                            state_changed_tx.send(()).ok();
                            Self::drain_pending_rpcs(&pending_rpcs).await;
                            return Err(PrinterError::WebSocket("connection closed".to_string()));
                        }
                        Some(Ok(_)) => {}
                        Some(Err(e)) => {
                            error!("[ws] stream error: {e}");
                            connected_tx.send(false).ok();
                            state_changed_tx.send(()).ok();
                            Self::drain_pending_rpcs(&pending_rpcs).await;
                            return Err(PrinterError::WebSocket(e.to_string()));
                        }
                        None => {
                            warn!("[ws] stream ended (printer disconnected)");
                            connected_tx.send(false).ok();
                            state_changed_tx.send(()).ok();
                            Self::drain_pending_rpcs(&pending_rpcs).await;
                            return Err(PrinterError::WebSocket("stream ended".to_string()));
                        }
                    }
                }
            }
        }

        connected_tx.send(false).ok();
        state_changed_tx.send(()).ok();
        Self::drain_pending_rpcs(&pending_rpcs).await;
        Ok(())
    }

    // fail all waiting RPC callers on disconnect
    async fn drain_pending_rpcs(rpcs: &PendingRpcs) {
        rpcs.lock().await.clear();
    }

    async fn handle_publish(
        topic: &str,
        payload: &[u8],
        api_status_topic: &str,
        api_response_topic: &str,
        state: &Arc<RwLock<PrinterState>>,
        state_changed_tx: &broadcast::Sender<()>,
        pending_rpcs: &PendingRpcs,
        pending_thumb: &mut Option<String>,
        last_thumb_req: &mut Option<tokio::time::Instant>,
    ) -> Option<serde_json::Value> {
        if topic == api_status_topic {
            let Ok(value) = serde_json::from_slice::<Value>(payload) else { return None };
            let msg_type = value.get("type").and_then(|t| t.as_str());
            if msg_type == Some("PING") || msg_type == Some("PONG") {
                trace!("ws heartbeat msg: {:?}", msg_type);
                return None;
            }
            if value.get("method").and_then(|m| m.as_u64()) == Some(METHOD_STATUS_PUSH as u64) {
                if let Some(result) = value.get("result") {
                    state.write().await.merge_delta(result);
                    state_changed_tx.send(()).ok();
                    trace!("[ws] status delta merged");
                }
            }
            // thumbnail fetch piggybacks status flow
            let s = state.read().await;
            let filename = s.full.print_status.filename.clone();
            let is_active = matches!(s.print_state(), PrintState::Printing | PrintState::Paused);
            let needs_thumb = !filename.is_empty() && !s.thumbnail_cache.contains_key(&filename);
            drop(s);
            if is_active && needs_thumb {
                // new file resets thumbnail backoff
                if pending_thumb.as_deref() != Some(&*filename) {
                    *pending_thumb = Some(filename.clone());
                    *last_thumb_req = None;
                }
                // retry thumbnail at most once per 30s
                let should_request = last_thumb_req
                    .map(|t| t.elapsed() >= Duration::from_secs(30))
                    .unwrap_or(true);
                if should_request {
                    *last_thumb_req = Some(tokio::time::Instant::now());
                    debug!("[ws] requesting thumbnail for {filename}");
                    return Some(serde_json::json!({
                        "method": METHOD_GET_FILE_THUMBNAIL,
                        "params": { "storage_media": "local", "file_name": filename },
                    }));
                }
            }
        } else if topic == api_response_topic {
            let Ok(resp) = serde_json::from_slice::<RpcResponse>(payload) else { return None };

            if resp.id > 0 {
                if let Some(tx) = pending_rpcs.lock().await.remove(&resp.id) {
                    let mut payload = resp.result.data.clone();
                    match &mut payload {
                        serde_json::Value::Object(map) => {
                            map.entry("error_code".to_string())
                                .or_insert_with(|| serde_json::json!(resp.result.error_code));
                        }
                        _ => {
                            payload = serde_json::json!({
                                "error_code": resp.result.error_code,
                                "value": payload,
                            });
                        }
                    }
                    tx.send(payload).ok();
                }
            }

            if resp.method == METHOD_GET_FULL_STATUS && resp.result.error_code == 0 {
                if let Ok(status) = serde_json::from_value::<FullStatus>(resp.result.data.clone()) {
                    state.write().await.seed(status);
                    state_changed_tx.send(()).ok();
                    info!("[ws] full status snapshot loaded");

                    let s = state.read().await;
                    let filename = s.full.print_status.filename.clone();
                    let is_active = matches!(s.print_state(), PrintState::Printing | PrintState::Paused);
                    let needs_thumb = !filename.is_empty() && !s.thumbnail_cache.contains_key(&filename);
                    drop(s);

                    if is_active && needs_thumb {
                        *pending_thumb = Some(filename.clone());
                        *last_thumb_req = Some(tokio::time::Instant::now());
                        return Some(serde_json::json!({
                            "method": METHOD_GET_FILE_THUMBNAIL,
                            "params": { "storage_media": "local", "file_name": filename },
                        }));
                    }
                }
            } else if resp.method == METHOD_GET_FILE_THUMBNAIL {
                let thumb = resp.result.data.get("thumbnail").and_then(|v| v.as_str()).unwrap_or("");
                if !thumb.is_empty() {
                    // cache by requested file, not current possibly-raced filename
                    let filename = if let Some(f) = pending_thumb.clone().filter(|f| !f.is_empty()) {
                        f
                    } else {
                        state.read().await.full.print_status.filename.clone()
                    };
                    if !filename.is_empty() {
                        state.write().await.thumbnail_cache.insert(filename.clone(), thumb.to_string());
                        state_changed_tx.send(()).ok();
                        info!("[ws] thumbnail cached for {filename}");
                    }
                } else {
                    // empty payload likely transient; keep backoff timer
                    debug!("[ws] thumbnail response empty for {:?}", pending_thumb);
                }
            } else if resp.method == METHOD_GET_FILE_LIST && resp.result.error_code == 0 {
                if let Some(arr) = resp.result.data.get("file_list").and_then(|v| v.as_array()) {
                    // printer returns empty list while busy printing; keep cached list
                    if !arr.is_empty() {
                        info!("[ws] file list loaded: {} files", arr.len());
                        state.write().await.files = arr.clone();
                        state_changed_tx.send(()).ok();
                    } else {
                        debug!("[ws] file list response empty, keeping cached list");
                    }
                }
            } else if resp.method == METHOD_GET_AMS_INFO && resp.result.error_code == 0 {
                if let Some(canvas) = resp.result.data.get("canvas_info") {
                    state.write().await.full.canvas_info = Some(canvas.clone());
                    state_changed_tx.send(()).ok();
                    info!("[ws] canvas info loaded");
                }
            }
        }
        None
    }
}

enum WsPacket {
    Publish { topic: String, payload: Vec<u8>, ack_id: Option<u16> },
    ConnAck(u8),
    SubAck,
    PubAck,
}

struct MqttOverWs {
    packet_id: std::sync::atomic::AtomicU16,
}

impl MqttOverWs {
    fn new() -> Self {
        Self {
            packet_id: std::sync::atomic::AtomicU16::new(1),
        }
    }

    async fn send_connect(
        &self,
        write: &mut (impl SinkExt<tokio_tungstenite::tungstenite::Message> + Unpin),
        client_id: &str,
        username: &str,
        password: &str,
    ) -> Result<(), PrinterError> {
        let packet = encode_connect(client_id, username, password);
        write
            .send(tokio_tungstenite::tungstenite::Message::Binary(packet))
            .await
            .map_err(|_| PrinterError::WebSocket("ws CONNECT send failed".to_string()))?;
        debug!("ws sent MQTT CONNECT");
        Ok(())
    }

    async fn wait_for_connack(
        &self,
        read: &mut (impl StreamExt<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin),
    ) -> Result<u8, PrinterError> {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Binary(data)) => {
                    if let Some(WsPacket::ConnAck(code)) = self.parse_packet(&data) {
                        return Ok(code);
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    return Err(PrinterError::WebSocket("closed waiting for CONNACK".to_string()));
                }
                Err(e) => {
                    return Err(PrinterError::WebSocket(format!("read error: {e}")));
                }
                _ => {}
            }
        }
        Err(PrinterError::WebSocket("stream ended waiting for CONNACK".to_string()))
    }

    async fn subscribe(
        &self,
        write: &mut (impl SinkExt<tokio_tungstenite::tungstenite::Message> + Unpin),
        topics: &[(&str, u8)],
    ) -> Result<(), PrinterError> {
        let pid = self.packet_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let packet = encode_subscribe(pid, topics);
        write
            .send(tokio_tungstenite::tungstenite::Message::Binary(packet))
            .await
            .map_err(|_| PrinterError::WebSocket("ws SUBSCRIBE send failed".to_string()))?;
        Ok(())
    }

    async fn puback(
        &self,
        write: &mut (impl SinkExt<tokio_tungstenite::tungstenite::Message> + Unpin),
        packet_id: u16,
    ) -> Result<(), PrinterError> {
        write
            .send(tokio_tungstenite::tungstenite::Message::Binary(encode_puback(packet_id)))
            .await
            .map_err(|_| PrinterError::WebSocket("ws PUBACK send failed".to_string()))
    }

    async fn publish(
        &self,
        write: &mut (impl SinkExt<tokio_tungstenite::tungstenite::Message> + Unpin),
        topic: &str,
        payload: &[u8],
    ) -> Result<(), PrinterError> {
        let pid = self.packet_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let packet = encode_publish(1, pid, topic, payload);
        write
            .send(tokio_tungstenite::tungstenite::Message::Binary(packet))
            .await
            .map_err(|_| PrinterError::WebSocket("ws PUBLISH send failed".to_string()))?;
        Ok(())
    }

    fn parse_packet(&self, data: &[u8]) -> Option<WsPacket> {
        if data.is_empty() {
            return None;
        }
        let packet_type = (data[0] >> 4) & 0x0F;
        match packet_type {
            2 => {
                // CONNACK
                if data.len() < 4 {
                    warn!("[ws] CONNACK too short ({} bytes)", data.len());
                    return None;
                }
                Some(WsPacket::ConnAck(data[3]))
            }
            3 => {
                // PUBLISH
                let mut pos = 1usize;
                let (remaining, br) = decode_remaining_len(data.get(pos..)?)?;
                pos += br;

                // bounds check
                if pos.saturating_add(remaining) > data.len() {
                    warn!("[ws] PUBLISH remaining_len={remaining} exceeds packet size={}", data.len());
                    return None;
                }

                if pos + 2 > data.len() {
                    warn!("[ws] PUBLISH too short for topic length field");
                    return None;
                }
                let topic_len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                pos += 2;

                if pos + topic_len > data.len() {
                    warn!("[ws] PUBLISH topic_len={topic_len} exceeds packet size={}", data.len());
                    return None;
                }
                let topic = String::from_utf8_lossy(&data[pos..pos + topic_len]).to_string();
                pos += topic_len;

                // qos packet id
                let qos = (data[0] >> 1) & 0x03;
                let ack_id = if qos > 0 {
                    if pos + 2 > data.len() {
                        warn!("[ws] PUBLISH QoS={qos} but packet too short for packet-id");
                        return None;
                    }
                    let pid = u16::from_be_bytes([data[pos], data[pos + 1]]);
                    pos += 2;
                    Some(pid)
                } else {
                    None
                };

                Some(WsPacket::Publish { topic, payload: data[pos..].to_vec(), ack_id })
            }
            4 => Some(WsPacket::PubAck),
            9 => Some(WsPacket::SubAck),
            other => {
                trace!("[ws] unhandled packet type {other:#x}");
                None
            }
        }
    }
}

fn encode_connect(client_id: &str, username: &str, password: &str) -> Vec<u8> {
    let cid = client_id.as_bytes();
    let uname = username.as_bytes();
    let pwd = password.as_bytes();
    // CONNECT remaining
    let remaining = 10 + 2 + cid.len() + 2 + uname.len() + 2 + pwd.len();

    let mut buf = vec![0x10]; // CONNECT hdr
    buf.extend_from_slice(&encode_remaining_len(remaining));
    buf.extend_from_slice(&[0x00, 0x04]);
    buf.extend_from_slice(b"MQTT");
    buf.push(0x04); // proto 3.1.1
    buf.push(0xC2); // user+pass+clean
    buf.extend_from_slice(&[0x00, 0x3C]); // keepalive 60s
    buf.extend_from_slice(&(cid.len() as u16).to_be_bytes());
    buf.extend_from_slice(cid);
    buf.extend_from_slice(&(uname.len() as u16).to_be_bytes());
    buf.extend_from_slice(uname);
    buf.extend_from_slice(&(pwd.len() as u16).to_be_bytes());
    buf.extend_from_slice(pwd);
    buf
}

fn encode_puback(packet_id: u16) -> Vec<u8> {
    vec![0x40, 0x02, (packet_id >> 8) as u8, (packet_id & 0xFF) as u8]
}

fn encode_subscribe(packet_id: u16, topics: &[(&str, u8)]) -> Vec<u8> {
    let remaining = 2 + topics.iter().map(|(t, _)| 2 + t.len() + 1).sum::<usize>();
    let mut buf = vec![0x82]; // SUBSCRIBE hdr
    buf.extend_from_slice(&encode_remaining_len(remaining));
    buf.extend_from_slice(&packet_id.to_be_bytes());
    for (topic, qos) in topics {
        buf.extend_from_slice(&(topic.len() as u16).to_be_bytes());
        buf.extend_from_slice(topic.as_bytes());
        buf.push(*qos);
    }
    buf
}

fn encode_publish(qos: u8, packet_id: u16, topic: &str, payload: &[u8]) -> Vec<u8> {
    let topic_bytes = topic.as_bytes();
    let has_pid = qos > 0;
    let remaining = 2 + topic_bytes.len() + if has_pid { 2 } else { 0 } + payload.len();

    let mut buf = vec![0x30 | ((qos & 0x03) << 1)];
    buf.extend_from_slice(&encode_remaining_len(remaining));
    buf.extend_from_slice(&(topic_bytes.len() as u16).to_be_bytes());
    buf.extend_from_slice(topic_bytes);
    if has_pid {
        buf.extend_from_slice(&packet_id.to_be_bytes());
    }
    buf.extend_from_slice(payload);
    buf
}

fn encode_remaining_len(mut len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(4);
    loop {
        let mut byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if len == 0 {
            break;
        }
    }
    out
}

fn decode_remaining_len(data: &[u8]) -> Option<(usize, usize)> {
    let mut multiplier = 1usize;
    let mut value = 0usize;
    for (i, &byte) in data.iter().enumerate() {
        value += (byte & 0x7F) as usize * multiplier;
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
        multiplier *= 128;
        if i >= 3 {
            return None;
        }
    }
    None
}

use std::collections::{HashMap, VecDeque};

use serde_json::Value;
use tokio::sync::broadcast;

use super::models::{DeviceInfo, FullStatus};
use super::status_map::{
    lookup_status,
    normalize_machine_status,
    NormalizedStatus,
};
use crate::detection::obico::Detection;

#[derive(Debug, Clone, PartialEq)]
pub enum PrintState {
    Idle,
    Printing,
    Paused,
}

/// Detection point.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectionPoint {
    pub ts: u64,
    pub score: f64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub print_filename: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub boxes: Vec<crate::detection::obico::Detection>,
}

#[derive(Debug, Clone)]
pub struct PrinterState {
    pub full: FullStatus,
    pub device_info: Option<DeviceInfo>,
    pub printer_ip: String,

    /// Raw + WebSocket connected.
    pub connected: bool,
    pub connected_raw: bool,
    pub connected_ws: bool,

    pub printer_ws_status: String,
    pub camera_connected: bool,
    pub detection_score: f64,
    pub detection_history: VecDeque<DetectionPoint>,

    /// Latest detections.
    pub latest_detections: Vec<Detection>,
    pub latest_detection_ts: u64,

    pub events: VecDeque<PrinterEvent>,

    /// Event total mono.
    pub events_total: u64,

    pub files: Vec<Value>,
    pub thumbnail_cache: HashMap<String, String>,

    // Suppress phase-change event before first seed.
    pub prev_machine_phase: Option<(i64, i64)>,

    event_tx: broadcast::Sender<PrinterEvent>,
}

#[derive(Debug, Clone)]
pub struct PrinterEvent {
    pub timestamp: std::time::SystemTime,
    pub kind: EventKind,
    pub description: String,
    pub snapshot: Option<String>,
}

/// Debug names.
#[derive(Debug, Clone)]
pub enum EventKind {
    Connected,
    Disconnected,
    PrintStarted,
    PrintPaused,
    PrintResumed,
    PrintStopped,
    PrintFinished,
    FailureNotifyThreshold,
    FailurePauseThreshold,
    AutoPaused,
    CommandPause,
    CommandResume,
    CommandStop,
    CommandLed,
    CommandFan,
    CommandSpeedMode,
    CommandTemp,
    CommandStartPrint,
    DetectionLogged,
    CameraLost,
    CameraRestored,
    ErrorOccurred,
    PhaseChanged(i64, String),
    DetectionEngineError,
    /// Loaded event kind.
    Loaded(String),
}

impl PrinterState {
    pub fn new(event_tx: broadcast::Sender<PrinterEvent>) -> Self {
        Self {
            full: FullStatus::default(),
            device_info: None,
            printer_ip: String::new(),
            connected: false,
            connected_raw: false,
            connected_ws: false,
            printer_ws_status: "connecting".to_string(),
            camera_connected: false,
            detection_score: 0.0,
            detection_history: VecDeque::with_capacity(200),
            latest_detections: Vec::new(),
            latest_detection_ts: 0,
            events: VecDeque::with_capacity(100),
            events_total: 0,
            files: Vec::new(),
            thumbnail_cache: HashMap::new(),
            prev_machine_phase: None,
            event_tx,
        }
    }

    pub fn seed(&mut self, status: FullStatus) {
        let old_state = self.print_state();

        let saved_canvas = self.full.canvas_info.take();

        self.prev_machine_phase = Some((
            status.machine_status.status,
            status.machine_status.sub_status,
        ));

        self.full = status;

        if self.full.canvas_info.is_none() {
            self.full.canvas_info = saved_canvas;
        }

        self.clear_print_task_if_idle();

        let new_state = self.print_state();

        if old_state != new_state {
            self.record_state_transition(old_state, new_state);
        }
    }

    pub fn merge_delta(&mut self, delta: &Value) {
        let old_state = self.print_state();
    
        let old_machine_phase = (
            self.full.machine_status.status,
            self.full.machine_status.sub_status,
        );
    
        let old_exception_codes: Vec<i64> = self
            .full
            .machine_status
            .exception_status
            .as_ref()
            .map(|v| v.iter().map(|e| e.code).collect())
            .unwrap_or_default();
    
        if let Ok(current) = serde_json::to_value(&self.full) {
            let merged = recursive_merge(&current, delta);
    
            if let Ok(status) = serde_json::from_value::<FullStatus>(merged) {
                self.full = status;
            }
        }
    
        self.clear_print_task_if_idle();
    
        let new_state = self.print_state();
    
        if old_state != new_state {
            self.record_state_transition(old_state, new_state);
        }
    
        let new_machine_phase = (
            self.full.machine_status.status,
            self.full.machine_status.sub_status,
        );
    
        // Detect changes in either the main status or sub-status.
        if self.prev_machine_phase.is_some()
            && new_machine_phase != old_machine_phase
        {
            self.prev_machine_phase = Some(new_machine_phase);
    
            let phase = build_phase_info(
                new_machine_phase.0,
                new_machine_phase.1,
                &self.full.print_status.state,
            );
    
            let description = match phase.detail {
                Some(detail) => format!(
                    "Phase: {} - {} (code {}, sub-status {})",
                    phase.label,
                    detail,
                    new_machine_phase.0,
                    new_machine_phase.1,
                ),
                None => format!(
                    "Phase: {} (code {}, sub-status {})",
                    phase.label,
                    new_machine_phase.0,
                    new_machine_phase.1,
                ),
            };
    
            self.add_event(
                EventKind::PhaseChanged(
                    new_machine_phase.0,
                    phase.label.to_string(),
                ),
                description,
            );
        } else {
            self.prev_machine_phase = Some(new_machine_phase);
        }
    
        let new_entries: Vec<(i64, Option<String>)> = self
            .full
            .machine_status
            .exception_status
            .as_ref()
            .map(|v| {
                v.iter()
                    .filter(|e| !old_exception_codes.contains(&e.code))
                    .map(|e| (e.code, e.description.clone()))
                    .collect()
            })
            .unwrap_or_default();
    
        for (code, desc) in new_entries {
            let msg = match desc {
                Some(d) => format!("Error {:#x}: {d}", code),
                None => format!("Error code {:#x}", code),
            };
    
            self.add_event(
                EventKind::ErrorOccurred,
                msg,
            );
        }
    }

    fn clear_print_task_if_idle(&mut self) {
        let norm = normalize_machine_status(
            self.full.machine_status.status,
        );
    
        if norm.is_active_print() {
            return;
        }
    
        // Terminal machine statuses always clear the print task.
        if !matches!(
            norm,
            NormalizedStatus::PrintCompleted
                | NormalizedStatus::Stopped
        ) {
            // Transient machine codes during an active print
            // must not clear the print task.
            let pstate = &self.full.print_status.state;
    
            if pstate == "printing" || pstate == "paused" {
                return;
            }
        }
    
        self.full.print_status.filename = String::new();
        self.full.print_status.state = String::new();
        self.full.print_status.current_layer = None;
        self.full.print_status.remaining_time_sec = None;
        self.full.print_status.print_duration = None;
        self.full.print_status.uuid = String::new();
    
        self.detection_score = 0.0;
    }

    pub fn print_state(&self) -> PrintState {
        let norm = normalize_machine_status(
            self.full.machine_status.status,
        );

        if matches!(
            norm,
            NormalizedStatus::PrintCompleted
                | NormalizedStatus::Stopped
        ) {
            return PrintState::Idle;
        }

        match self.full.print_status.state.as_str() {
            "printing" => PrintState::Printing,
            "paused" => PrintState::Paused,
            _ => PrintState::Idle,
        }
    }

    pub fn add_event(
        &mut self,
        kind: EventKind,
        description: String,
    ) {
        // Deduplicate same kind + message.
        if let Some(last) = self.events.back() {
            if std::mem::discriminant(&last.kind)
                == std::mem::discriminant(&kind)
                && last.description == description
            {
                return;
            }
        }

        let e = PrinterEvent {
            timestamp: std::time::SystemTime::now(),
            kind,
            description,
            snapshot: None,
        };

        let _ = self.event_tx.send(e.clone());

        self.events.push_back(e);
        self.events_total += 1;

        if self.events.len() > 100 {
            self.events.pop_front();
        }
    }

    pub fn add_event_with_snapshot(
        &mut self,
        kind: EventKind,
        description: String,
        snapshot: Option<String>,
    ) {
        let e = PrinterEvent {
            timestamp: std::time::SystemTime::now(),
            kind,
            description,
            snapshot,
        };

        let _ = self.event_tx.send(e.clone());

        self.events.push_back(e);
        self.events_total += 1;

        if self.events.len() > 100 {
            self.events.pop_front();
        }
    }

    pub fn clear_on_disconnect(&mut self) {
        self.full.machine_status.status = -1;
        self.full.machine_status.sub_status = 0;
        self.prev_machine_phase = None;
    }

    fn record_state_transition(
        &mut self,
        from: PrintState,
        to: PrintState,
    ) {
        match (&from, &to) {
            (PrintState::Idle, PrintState::Printing) => {
                let filename =
                    self.full.print_status.filename.clone();

                self.add_event(
                    EventKind::PrintStarted,
                    format!(
                        "Print started: {}",
                        truncate_filename(&filename)
                    ),
                );
            }

            (PrintState::Printing, PrintState::Paused) => {
                self.add_event(
                    EventKind::PrintPaused,
                    "Print paused".to_string(),
                );
            }

            (PrintState::Paused, PrintState::Printing) => {
                self.add_event(
                    EventKind::PrintResumed,
                    "Print resumed".to_string(),
                );
            }

            (PrintState::Printing, PrintState::Idle) => {
                self.add_event(
                    EventKind::PrintFinished,
                    "Print finished".to_string(),
                );
            }

            (PrintState::Paused, PrintState::Idle) => {
                self.add_event(
                    EventKind::PrintStopped,
                    "Print stopped".to_string(),
                );
            }

            _ => {}
        }
    }
}

impl Default for PrinterState {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(1);
        Self::new(tx)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhaseInfo {
    /// Main machine status.
    pub label: &'static str,

    /// Detailed status within the main status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<&'static str>,

    /// CC2 protocol name of the detailed status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol_name: Option<&'static str>,

    /// Existing frontend variant.
    pub variant: &'static str,
}

pub fn build_phase_info(
    machine_status: i64,
    sub_status: i64,
    print_state: &str,
) -> PhaseInfo {
    // Connection error always wins.
    if machine_status == -1 {
        return PhaseInfo {
            label: "Offline",
            detail: None,
            protocol_name: None,
            variant: "error",
        };
    }

    // Try the exact CC2 status + sub-status mapping first.
    if let Some(mapping) = lookup_status(machine_status, sub_status) {
        let main_status = normalize_machine_status(machine_status);

        let mut label = main_status.label();
        let mut variant = mapping.normalized.variant();

        // A completed/idle sub-status can legitimately replace
        // the main "Printing" label.
        if matches!(
            mapping.normalized,
            NormalizedStatus::Idle
                | NormalizedStatus::PrintCompleted
        ) {
            label = mapping.normalized.label();
            variant = mapping.normalized.variant();
        }

        return PhaseInfo {
            label,
            detail: Some(mapping.description),
            protocol_name: Some(mapping.name),
            variant,
        };
    }

    // No exact sub-status mapping exists.
    // Fall back to the main machine status.
    let normalized = normalize_machine_status(machine_status);

    // Emergency / error states override print state.
    if matches!(
        normalized,
        NormalizedStatus::EmergencyStop
            | NormalizedStatus::Error
            | NormalizedStatus::IdNotMatch
            | NormalizedStatus::AuthError
    ) {
        return PhaseInfo {
            label: normalized.label(),
            detail: None,
            protocol_name: None,
            variant: normalized.variant(),
        };
    }

    // The print_status state is useful as a fallback when the CC2
    // machine status does not provide a more specific mapping.
    match print_state {
        "paused" => PhaseInfo {
            label: "Paused",
            detail: None,
            protocol_name: None,
            variant: "paused",
        },

        "printing" => PhaseInfo {
            label: "Printing",
            detail: None,
            protocol_name: None,
            variant: "active",
        },

        _ => PhaseInfo {
            label: normalized.label(),
            detail: None,
            protocol_name: None,
            variant: normalized.variant(),
        },
    }
}

fn recursive_merge(
    base: &Value,
    delta: &Value,
) -> Value {
    match (base, delta) {
        (
            Value::Object(base_map),
            Value::Object(delta_map),
        ) => {
            let mut merged = base_map.clone();

            for (key, delta_value) in delta_map {
                let merged_value = match base_map.get(key) {
                    Some(base_value) => {
                        recursive_merge(base_value, delta_value)
                    }
                    None => delta_value.clone(),
                };

                merged.insert(
                    key.clone(),
                    merged_value,
                );
            }

            Value::Object(merged)
        }

        (_, delta_value) => delta_value.clone(),
    }
}

fn truncate_filename(name: &str) -> String {
    if name.len() <= 40 {
        return name.to_string();
    }

    let mut cut = name.len() - 37;

    while !name.is_char_boundary(cut) {
        cut += 1;
    }

    format!("...{}", &name[cut..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_recursive_merge_preserves_nested_fields() {
        let base = json!({
            "fans": {
                "aux_fan": {"speed": 100.0},
                "fan": {"speed": 50.0},
                "box_fan": {"speed": 0.0}
            },
            "extruder": {
                "temperature": 200.0,
                "target": 210
            }
        });

        let delta = json!({
            "fans": {
                "fan": {"speed": 255.0}
            }
        });

        let merged = recursive_merge(&base, &delta);

        assert_eq!(
            merged["fans"]["fan"]["speed"],
            255.0
        );

        assert_eq!(
            merged["fans"]["aux_fan"]["speed"],
            100.0
        );

        assert_eq!(
            merged["fans"]["box_fan"]["speed"],
            0.0
        );

        assert_eq!(
            merged["extruder"]["temperature"],
            200.0
        );
    }

    #[test]
    fn test_recursive_merge_adds_new_keys() {
        let base = json!({
            "fans": {
                "fan": {"speed": 50.0}
            }
        });

        let delta = json!({
            "fans": {
                "aux_fan": {"speed": 100.0}
            },
            "led": {
                "status": 1
            }
        });

        let merged = recursive_merge(&base, &delta);

        assert_eq!(
            merged["fans"]["fan"]["speed"],
            50.0
        );

        assert_eq!(
            merged["fans"]["aux_fan"]["speed"],
            100.0
        );

        assert_eq!(
            merged["led"]["status"],
            1
        );
    }

    #[test]
    fn test_recursive_merge_overwrites_scalars() {
        let base = json!({
            "machine_status": {
                "status": 1,
                "progress": 50
            }
        });

        let delta = json!({
            "machine_status": {
                "status": 2,
                "progress": 75
            }
        });

        let merged = recursive_merge(&base, &delta);

        assert_eq!(
            merged["machine_status"]["status"],
            2
        );

        assert_eq!(
            merged["machine_status"]["progress"],
            75
        );
    }

    #[test]
    fn test_print_state_idle() {
        let mut state = PrinterState::default();

        state.full.print_status.state = String::new();

        assert!(matches!(
            state.print_state(),
            PrintState::Idle
        ));
    }

    #[test]
    fn test_print_state_printing() {
        let mut state = PrinterState::default();

        state.full.machine_status.status = 2;
        state.full.print_status.state =
            "printing".to_string();

        assert!(matches!(
            state.print_state(),
            PrintState::Printing
        ));
    }

    #[test]
    fn test_print_state_paused() {
        let mut state = PrinterState::default();

        state.full.machine_status.status = 2;
        state.full.print_status.state =
            "paused".to_string();

        assert!(matches!(
            state.print_state(),
            PrintState::Paused
        ));
    }

    #[test]
    fn test_merge_delta_updates_state() {
        let mut state = PrinterState::default();

        state.full.machine_status.status = 2;
        state.full.print_status.state =
            "printing".to_string();

        state.full.print_status.filename =
            "test.gcode".to_string();

        state.merge_delta(&json!({
            "print_status": {
                "state": "paused"
            },
            "machine_status": {
                "status": 2,
                "sub_status": 0,
                "progress": 0
            }
        }));

        assert!(matches!(
            state.print_state(),
            PrintState::Paused
        ));
    }

    #[test]
    fn phase_info_detects_sub_status_while_printing() {
        let phase = build_phase_info(2, 1045, "printing");
    
        assert_eq!(phase.label, "Printing");
        assert_eq!(
            phase.detail,
            Some("Nozzle heating up")
        );
        assert_eq!(
            phase.protocol_name,
            Some("EXTRUDER_PREHEATING")
        );
    }

    #[test]
    fn phase_info_reports_canvas_filament_operation() {
        let phase = build_phase_info(2, 1066, "printing");
    
        assert_eq!(phase.label, "Printing");
        assert_eq!(
            phase.detail,
            Some("Canvas Operation - undocumented")
        );
        assert_eq!(
            phase.protocol_name,
            Some("FILAMENT_CHANGE")
        );
    }

    #[test]
    fn test_events_capped_at_100() {
        let mut state = PrinterState::default();

        for i in 0..105 {
            state.add_event(
                EventKind::Connected,
                format!("event {}", i),
            );
        }

        assert_eq!(state.events.len(), 100);
        assert_eq!(
            state.events[0].description,
            "event 5"
        );
    }

    #[test]
    fn normalize_main_status() {
        assert_eq!(
            normalize_machine_status(1),
            NormalizedStatus::Idle
        );

        assert_eq!(
            normalize_machine_status(0),
            NormalizedStatus::Initializing
        );

        assert_eq!(
            normalize_machine_status(999),
            NormalizedStatus::Error
        );

        assert_eq!(
            normalize_machine_status(1000),
            NormalizedStatus::IdNotMatch
        );

        assert_eq!(
            normalize_machine_status(1001),
            NormalizedStatus::AuthError
        );
    }

    #[test]
    fn phase_info_uses_sub_status_mapping() {
        let phase =
            build_phase_info(2, 1045, "printing");

        assert_eq!(
            phase.label,
            "Printing"
        );

        assert_eq!(
            phase.detail,
            Some("Nozzle heating up")
        );

        assert_eq!(
            phase.protocol_name,
            Some("EXTRUDER_PREHEATING")
        );
    }

    #[test]
    fn phase_info_reports_bed_preheating() {
        let phase =
            build_phase_info(2, 1405, "printing");

        assert_eq!(
            phase.label,
            "Printing"
        );

        assert_eq!(
            phase.detail,
            Some("Bed heating up")
        );

        assert_eq!(
            phase.protocol_name,
            Some("BED_PREHEATING")
        );
    }

    #[test]
    fn phase_info_reports_auto_leveling_during_print() {
        let phase =
            build_phase_info(2, 2901, "printing");

        assert_eq!(
            phase.label,
            "Printing"
        );

        assert_eq!(
            phase.protocol_name,
            Some("AUTO_LEVELING")
        );

        assert!(
            phase.detail.is_some()
        );
    }

    #[test]
    fn phase_info_keeps_main_and_detail_status_separate() {
        let phase =
            build_phase_info(2, 1405, "printing");

        // Main status.
        assert_eq!(
            phase.label,
            "Printing"
        );

        // Detailed status.
        assert_eq!(
            phase.detail,
            Some("Bed heating up")
        );

        // Original CC2 protocol name.
        assert_eq!(
            phase.protocol_name,
            Some("BED_PREHEATING")
        );
    }

    #[test]
    fn phase_info_falls_back_to_main_status_for_unknown_sub_status() {
        let phase =
            build_phase_info(2, 999999, "printing");

        assert_eq!(
            phase.label,
            "Printing"
        );

        assert_eq!(
            phase.detail,
            None
        );

        assert_eq!(
            phase.protocol_name,
            None
        );

        assert_eq!(
            phase.variant,
            "active"
        );
    }

    #[test]
    fn phase_info_offline() {
        let phase =
            build_phase_info(-1, 0, "");

        assert_eq!(
            phase.label,
            "Offline"
        );

        assert_eq!(
            phase.detail,
            None
        );

        assert_eq!(
            phase.protocol_name,
            None
        );

        assert_eq!(
            phase.variant,
            "error"
        );
    }
}

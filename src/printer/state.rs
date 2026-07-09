use std::collections::{HashMap, VecDeque};
use serde_json::Value;
use tokio::sync::broadcast;

use super::models::{DeviceInfo, FullStatus};
use crate::detection::obico::Detection;

#[derive(Debug, Clone, PartialEq)]
pub enum PrintState {
    Idle,
    Printing,
    Paused,
}

/// detection point
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

#[derive(Debug, Clone, PartialEq)]
pub enum NormalizedStatus {
    Offline, Idle, Printing, Pausing, Paused, PrintCompleted, Canceled,
    SelfChecking, AutoLeveling, PidCalibrating, ResonanceTesting, Updating,
    FileCopying, FileTransferring, Homing, Preheating, FilamentOperating,
    ExtruderOperating, RfidRecognizing, VideoComposing, EmergencyStop,
    PowerLossRecovery, Initializing, Busy, Error, IdNotMatch, AuthError, Unknown,
}

impl NormalizedStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Offline => "Offline",
            Self::Idle => "Idle",
            Self::Printing => "Printing",
            Self::Pausing => "Pausing",
            Self::Paused => "Paused",
            Self::PrintCompleted => "Print Completed",
            Self::Canceled => "Canceled",
            Self::SelfChecking => "Self Checking",
            Self::AutoLeveling => "Auto Leveling",
            Self::PidCalibrating => "PID Calibrating",
            Self::ResonanceTesting => "Resonance Testing",
            Self::Updating => "Updating",
            Self::FileCopying => "File Copying",
            Self::FileTransferring => "File Transferring",
            Self::Homing => "Homing",
            Self::Preheating => "Preheating",
            Self::FilamentOperating => "Filament Operating",
            Self::ExtruderOperating => "Extruder Operating",
            Self::RfidRecognizing => "RFID Recognizing",
            Self::VideoComposing => "Video Composing",
            Self::EmergencyStop => "Emergency Stop",
            Self::PowerLossRecovery => "Power Loss Recovery",
            Self::Initializing => "Initializing",
            Self::Busy => "Busy",
            Self::Error => "Error",
            Self::IdNotMatch => "ID Not Match",
            Self::AuthError => "Auth Error",
            Self::Unknown => "Unknown",
        }
    }

    pub fn is_active_print(&self) -> bool {
        matches!(self, Self::Printing | Self::Pausing | Self::Paused)
    }
}

pub fn normalize_machine_status(status: i64, sub_status: i64) -> NormalizedStatus {
    match sub_status {
        1 => return NormalizedStatus::Pausing,
        2 => return NormalizedStatus::Paused,
        3 => return NormalizedStatus::PrintCompleted,
        _ => {}
    }
    match status {
        -1  => NormalizedStatus::Offline,
        0   => NormalizedStatus::Idle,
        1   => NormalizedStatus::Printing,
        2   => NormalizedStatus::Paused,
        3   => NormalizedStatus::Pausing,
        4   => NormalizedStatus::Canceled,
        5   => NormalizedStatus::SelfChecking,
        6   => NormalizedStatus::AutoLeveling,
        7   => NormalizedStatus::PidCalibrating,
        8   => NormalizedStatus::ResonanceTesting,
        9   => NormalizedStatus::Updating,
        10  => NormalizedStatus::FileCopying,
        11  => NormalizedStatus::FileTransferring,
        12  => NormalizedStatus::Homing,
        13  => NormalizedStatus::Preheating,
        14  => NormalizedStatus::FilamentOperating,
        15  => NormalizedStatus::ExtruderOperating,
        16  => NormalizedStatus::PrintCompleted,
        17  => NormalizedStatus::RfidRecognizing,
        18  => NormalizedStatus::VideoComposing,
        19  => NormalizedStatus::EmergencyStop,
        20  => NormalizedStatus::PowerLossRecovery,
        21  => NormalizedStatus::Initializing,
        998 => NormalizedStatus::Busy,
        999 => NormalizedStatus::Error,
        1000=> NormalizedStatus::IdNotMatch,
        1001=> NormalizedStatus::AuthError,
        _   => NormalizedStatus::Unknown,
    }
}

#[derive(Debug, Clone)]
pub struct PrinterState {
    pub full: FullStatus,
    pub device_info: Option<DeviceInfo>,
    pub printer_ip: String,
    /// raw+ws connected
    pub connected: bool,
    pub connected_raw: bool,
    pub connected_ws: bool,
    pub printer_ws_status: String,
    pub camera_connected: bool,
    pub detection_score: f64,
    pub detection_history: VecDeque<DetectionPoint>,
    /// latest detections
    pub latest_detections: Vec<Detection>,
    pub latest_detection_ts: u64,
    pub events: Vec<PrinterEvent>,
    /// event total mono
    pub events_total: u64,
    pub files: Vec<Value>,
    pub thumbnail_cache: HashMap<String, String>,
    // suppress phase-change event before first seed
    pub prev_machine_status: Option<i64>,
    event_tx: broadcast::Sender<PrinterEvent>,
}

#[derive(Debug, Clone)]
pub struct PrinterEvent {
    pub timestamp: std::time::SystemTime,
    pub kind: EventKind,
    pub description: String,
    pub snapshot: Option<String>,
}

// debug names
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
    CommandStartPrint,
    DetectionLogged,
    CameraLost,
    CameraRestored,
    ErrorOccurred,
    PhaseChanged(i64, String),
    DetectionEngineError,
    /// loaded event kind
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
            events: Vec::with_capacity(100),
            events_total: 0,
            files: Vec::new(),
            thumbnail_cache: HashMap::new(),
            prev_machine_status: None,
            event_tx,
        }
    }

    pub fn seed(&mut self, status: FullStatus) {
        let old_state = self.print_state();
        let saved_canvas = self.full.canvas_info.take();
        self.prev_machine_status = Some(status.machine_status.status);
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
        let old_machine_code = self.full.machine_status.status;

        let old_exception_codes: Vec<i64> = self.full.machine_status.exception_status.as_ref()
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

        let new_machine_code = self.full.machine_status.status;
        if self.prev_machine_status.is_some() && new_machine_code != old_machine_code {
            self.prev_machine_status = Some(new_machine_code);
            let label = machine_phase_label_ctx(new_machine_code, self.full.machine_status.sub_status, &self.full.print_status.state);
            self.add_event(
                EventKind::PhaseChanged(new_machine_code, label.to_string()),
                format!("Phase: {label} (code {new_machine_code})"),
            );
        } else {
            self.prev_machine_status = Some(new_machine_code);
        }

        let new_entries: Vec<(i64, Option<String>)> = self.full.machine_status.exception_status
            .as_ref()
            .map(|v| v.iter()
                .filter(|e| !old_exception_codes.contains(&e.code))
                .map(|e| (e.code, e.description.clone()))
                .collect())
            .unwrap_or_default();
        for (code, desc) in new_entries {
            let msg = match desc {
                Some(d) => format!("Error {:#x}: {d}", code),
                None => format!("Error code {:#x}", code),
            };
            self.add_event(EventKind::ErrorOccurred, msg);
        }
    }

    fn clear_print_task_if_idle(&mut self) {
        let norm = normalize_machine_status(
            self.full.machine_status.status,
            self.full.machine_status.sub_status,
        );

        if norm.is_active_print() {
            return;
        }

        // terminal machine status always clears print task
        if !matches!(norm, NormalizedStatus::PrintCompleted | NormalizedStatus::Canceled) {
            // transient machine codes during active print must not clear task
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
            self.full.machine_status.sub_status,
        );
        if matches!(norm, NormalizedStatus::PrintCompleted | NormalizedStatus::Canceled) {
            return PrintState::Idle;
        }
        match self.full.print_status.state.as_str() {
            "printing" => PrintState::Printing,
            "paused"   => PrintState::Paused,
            _          => PrintState::Idle,
        }
    }

    pub fn add_event(&mut self, kind: EventKind, description: String) {
        // dedup same kind+msg
        if let Some(last) = self.events.last() {
            if std::mem::discriminant(&last.kind) == std::mem::discriminant(&kind)
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
        self.events.push(e);
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
        self.events.push(e);
        self.events_total += 1;
        if self.events.len() > 100 {
            self.events.remove(0);
        }
    }

    pub fn clear_on_disconnect(&mut self) {
        self.full.machine_status.status = -1;
        self.full.machine_status.sub_status = 0;
    }

    fn record_state_transition(&mut self, from: PrintState, to: PrintState) {
        match (&from, &to) {
            (PrintState::Idle, PrintState::Printing) => {
                let filename = self.full.print_status.filename.clone();
                self.add_event(
                    EventKind::PrintStarted,
                    format!("Print started: {}", truncate_filename(&filename)),
                );
            }
            (PrintState::Printing, PrintState::Paused) => {
                self.add_event(EventKind::PrintPaused, "Print paused".to_string());
            }
            (PrintState::Paused, PrintState::Printing) => {
                self.add_event(EventKind::PrintResumed, "Print resumed".to_string());
            }
            (PrintState::Printing, PrintState::Idle) => {
                self.add_event(EventKind::PrintFinished, "Print finished".to_string());
            }
            (PrintState::Paused, PrintState::Idle) => {
                self.add_event(EventKind::PrintStopped, "Print stopped".to_string());
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

pub fn machine_phase_label_ctx(code: i64, sub_status: i64, print_state: &str) -> &'static str {
    let norm = normalize_machine_status(code, sub_status);
    // firmware reuses these codes during motion; trust only with active print
    match norm {
        NormalizedStatus::Printing | NormalizedStatus::FileCopying
            if print_state != "printing" && print_state != "paused" => "Idle",
        _ => norm.label(),
    }
}


#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhaseInfo {
    pub label: &'static str,
    pub variant: &'static str,
}

pub fn build_phase_info(machine_status: i64, sub_status: i64, print_state: &str) -> PhaseInfo {
    // hard errors override print_state
    if machine_status == -1 {
        return PhaseInfo { label: "Offline", variant: "error" };
    }
    if machine_status == 19 {
        return PhaseInfo { label: "Emergency Stop", variant: "error" };
    }
    if machine_status >= 999 {
        return PhaseInfo { label: "Error", variant: "error" };
    }

    // sub_status refines active-print state
    if sub_status == 1 {
        return PhaseInfo { label: "Pausing", variant: "pausing" };
    }
    if sub_status == 2 {
        return PhaseInfo { label: "Paused", variant: "paused" };
    }

    // print_state wins over transient machine codes
    if print_state == "printing" {
        return PhaseInfo { label: "Printing", variant: "printing" };
    }
    if print_state == "paused" {
        return PhaseInfo { label: "Paused", variant: "paused" };
    }

    PhaseInfo { label: "Idle", variant: "idle" }
}

fn recursive_merge(base: &Value, delta: &Value) -> Value {
    match (base, delta) {
        (Value::Object(base_map), Value::Object(delta_map)) => {
            let mut merged = base_map.clone();
            for (key, delta_value) in delta_map {
                let merged_value = match base_map.get(key) {
                    Some(base_value) => recursive_merge(base_value, delta_value),
                    None => delta_value.clone(),
                };
                merged.insert(key.clone(), merged_value);
            }
            Value::Object(merged)
        }
        (_, delta_value) => delta_value.clone(),
    }
}

fn truncate_filename(name: &str) -> String {
    if name.len() <= 40 {
        name.to_string()
    } else {
        let mut cut = name.len() - 37;
        while !name.is_char_boundary(cut) { cut += 1; }
        format!("...{}", &name[cut..])
    }
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
        let delta = json!({ "fans": { "fan": {"speed": 255.0} } });
        let merged = recursive_merge(&base, &delta);
        assert_eq!(merged["fans"]["fan"]["speed"], 255.0);
        assert_eq!(merged["fans"]["aux_fan"]["speed"], 100.0);
        assert_eq!(merged["fans"]["box_fan"]["speed"], 0.0);
        assert_eq!(merged["extruder"]["temperature"], 200.0);
    }

    #[test]
    fn test_recursive_merge_adds_new_keys() {
        let base = json!({ "fans": { "fan": {"speed": 50.0} } });
        let delta = json!({
            "fans": { "aux_fan": {"speed": 100.0} },
            "led": {"status": 1}
        });
        let merged = recursive_merge(&base, &delta);
        assert_eq!(merged["fans"]["fan"]["speed"], 50.0);
        assert_eq!(merged["fans"]["aux_fan"]["speed"], 100.0);
        assert_eq!(merged["led"]["status"], 1);
    }

    #[test]
    fn test_recursive_merge_overwrites_scalars() {
        let base = json!({ "machine_status": { "status": 1, "progress": 50 } });
        let delta = json!({ "machine_status": { "status": 2, "progress": 75 } });
        let merged = recursive_merge(&base, &delta);
        assert_eq!(merged["machine_status"]["status"], 2);
        assert_eq!(merged["machine_status"]["progress"], 75);
    }

    #[test]
    fn test_print_state_idle() {
        let mut state = PrinterState::default();
        state.full.print_status.state = String::new();
        assert!(matches!(state.print_state(), PrintState::Idle));
    }

    #[test]
    fn test_print_state_printing() {
        let mut state = PrinterState::default();
        state.full.machine_status.status = 1;
        state.full.print_status.state = "printing".to_string();
        assert!(matches!(state.print_state(), PrintState::Printing));
    }

    #[test]
    fn test_print_state_paused() {
        let mut state = PrinterState::default();
        state.full.machine_status.status = 2;
        state.full.print_status.state = "paused".to_string();
        assert!(matches!(state.print_state(), PrintState::Paused));
    }

    #[test]
    fn test_merge_delta_updates_state() {
        let mut state = PrinterState::default();
        state.full.machine_status.status = 1;
        state.full.print_status.state = "printing".to_string();
        state.full.print_status.filename = "test.gcode".to_string();
        state.merge_delta(&json!({ "print_status": { "state": "paused" }, "machine_status": { "status": 2, "sub_status": 0, "progress": 0 } }));
        assert!(matches!(state.print_state(), PrintState::Paused));
    }

    #[test]
    fn phase_label_ctx_idle_machine_avoids_printing() {
        assert_eq!(machine_phase_label_ctx(1, 0, ""), "Idle");
        assert_eq!(machine_phase_label_ctx(1, 0, "idle"), "Idle");
    }

    #[test]
    fn phase_label_ctx_real_print_uses_printing() {
        assert_eq!(machine_phase_label_ctx(1, 0, "printing"), "Printing");
        assert_eq!(machine_phase_label_ctx(1, 0, "paused"), "Printing");
    }

    #[test]
    fn phase_label_ctx_file_copy_during_motion() {
        assert_eq!(machine_phase_label_ctx(10, 0, ""), "Idle");
        assert_eq!(machine_phase_label_ctx(10, 0, "idle"), "Idle");
    }

    #[test]
    fn phase_label_ctx_file_copy_during_print_unchanged() {
        assert_eq!(machine_phase_label_ctx(10, 0, "printing"), "File Copying");
    }

    #[test]
    fn phase_label_ctx_unambiguous_codes_passthrough() {
        assert_eq!(machine_phase_label_ctx(12, 0, ""), "Homing");
        assert_eq!(machine_phase_label_ctx(0, 0, ""), "Idle");
        assert_eq!(machine_phase_label_ctx(19, 0, ""), "Emergency Stop");
    }

    #[test]
    fn test_events_capped_at_100() {
        let mut state = PrinterState::default();
        for i in 0..105 {
            state.add_event(EventKind::Connected, format!("event {}", i));
        }
        assert_eq!(state.events.len(), 100);
        assert_eq!(state.events[0].description, "event 5");
    }

    #[test]
    fn normalize_sub_status_overrides() {
        assert_eq!(normalize_machine_status(1, 1), NormalizedStatus::Pausing);
        assert_eq!(normalize_machine_status(1, 2), NormalizedStatus::Paused);
        assert_eq!(normalize_machine_status(1, 3), NormalizedStatus::PrintCompleted);
    }

    #[test]
    fn normalize_main_status() {
        assert_eq!(normalize_machine_status(0, 0), NormalizedStatus::Idle);
        assert_eq!(normalize_machine_status(16, 0), NormalizedStatus::PrintCompleted);
        assert_eq!(normalize_machine_status(999, 0), NormalizedStatus::Error);
        assert_eq!(normalize_machine_status(1000, 0), NormalizedStatus::IdNotMatch);
        assert_eq!(normalize_machine_status(1001, 0), NormalizedStatus::AuthError);
    }
}

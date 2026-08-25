//! CC2 machine status / sub-status mapping.
//!
//! Protocol reference:
//! https://github.com/danielcherubini/elegoo-homeassistant/blob/main/docs/CC2_PROTOCOL.md
//!
//! Keep CC2-specific numeric codes in this file.  The rest of the
//! application should work with NormalizedStatus instead.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalizedStatus {
    Offline,
    Initializing,
    Idle,

    Printing,
    Pausing,
    Paused,
    Resuming,
    Stopping,
    Stopped,
    PrintCompleted,

    SelfChecking,
    AutoLeveling,
    PidCalibrating,
    ResonanceTesting,
    Updating,
    FileTransferring,
    Homing,
    Preheating,
    FilamentOperating,
    ExtruderOperating,
    VideoComposing,
    EmergencyStop,
    PowerLossRecovery,

    Busy,
    Error,
    IdNotMatch,
    AuthError,
    Unknown,
}

impl NormalizedStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Offline => "Offline",
            Self::Initializing => "Initializing",
            Self::Idle => "Idle",

            Self::Printing => "Printing",
            Self::Pausing => "Pausing",
            Self::Paused => "Paused",
            Self::Resuming => "Resuming",
            Self::Stopping => "Stopping",
            Self::Stopped => "Stopped",
            Self::PrintCompleted => "Print Completed",

            Self::SelfChecking => "Self Checking",
            Self::AutoLeveling => "Auto Leveling",
            Self::PidCalibrating => "PID Calibrating",
            Self::ResonanceTesting => "Resonance Testing",
            Self::Updating => "Updating",
            Self::FileTransferring => "File Transferring",
            Self::Homing => "Homing",
            Self::Preheating => "Preheating",
            Self::FilamentOperating => "Filament Operating",
            Self::ExtruderOperating => "Extruder Operating",
            Self::VideoComposing => "Video Composing",
            Self::EmergencyStop => "Emergency Stop",
            Self::PowerLossRecovery => "Power Loss Recovery",

            Self::Busy => "Busy",
            Self::Error => "Error",
            Self::IdNotMatch => "ID Not Match",
            Self::AuthError => "Auth Error",
            Self::Unknown => "Unknown",
        }
    }

    pub const fn variant(self) -> &'static str {
        match self {
            Self::Offline
            | Self::EmergencyStop
            | Self::Error
            | Self::IdNotMatch
            | Self::AuthError => "error",

            Self::Pausing => "pausing",
            Self::Paused => "paused",
            Self::Resuming => "resuming",
            Self::Stopping => "stopping",

            Self::Printing
            | Self::Preheating
            | Self::AutoLeveling
            | Self::Homing
            | Self::PidCalibrating
            | Self::ResonanceTesting
            | Self::Updating
            | Self::FileTransferring
            | Self::FilamentOperating
            | Self::ExtruderOperating
            | Self::SelfChecking
            | Self::VideoComposing
            | Self::PowerLossRecovery
            | Self::Busy => "active",

            Self::PrintCompleted
            | Self::Stopped => "completed",

            Self::Idle
            | Self::Initializing
            | Self::Unknown => "idle",
        }
    }

    pub const fn is_active_print(self) -> bool {
        matches!(
            self,
            Self::Printing
                | Self::Pausing
                | Self::Paused
                | Self::Resuming
                | Self::Stopping
                | Self::Preheating
                | Self::AutoLeveling
                | Self::Homing
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusMapping {
    pub status: &'static [i64],
    pub sub_status: i64,
    pub name: &'static str,
    pub description: &'static str,
    pub normalized: NormalizedStatus,
}

pub const STATUS_MAPPINGS: &[StatusMapping] = &[
    // status = 2 PRINTING
    StatusMapping {
        status: &[2],
        sub_status: 0,
        name: "NONE",
        description: "No specific sub-status",
        normalized: NormalizedStatus::Printing,
    },
    StatusMapping {
        status: &[2],
        sub_status: 1041,
        name: "IDLE_IN_PRINT",
        description: "Idle within a print",
        normalized: NormalizedStatus::Printing,
    },
    StatusMapping {
        status: &[2, 10],
        sub_status: 1045,
        name: "EXTRUDER_PREHEATING",
        description: "Nozzle heating up",
        normalized: NormalizedStatus::Preheating,
    },
    StatusMapping {
        status: &[2],
        sub_status: 1096,
        name: "EXTRUDER_PREHEATING_2",
        description: "Nozzle heating",
        normalized: NormalizedStatus::Preheating,
    },
    StatusMapping {
        status: &[2],
        sub_status: 1405,
        name: "BED_PREHEATING",
        description: "Bed heating up",
        normalized: NormalizedStatus::Preheating,
    },
    StatusMapping {
        status: &[2],
        sub_status: 1906,
        name: "BED_PREHEATING_2",
        description: "Bed heating",
        normalized: NormalizedStatus::Preheating,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2075,
        name: "PRINTING",
        description: "Printing",
        normalized: NormalizedStatus::Printing,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2077,
        name: "PRINTING_COMPLETED",
        description: "Print completed successfully",
        normalized: NormalizedStatus::PrintCompleted,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2401,
        name: "RESUMING",
        description: "Resuming from pause",
        normalized: NormalizedStatus::Resuming,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2402,
        name: "RESUMING_COMPLETED",
        description: "Resume completed",
        normalized: NormalizedStatus::Printing,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2501,
        name: "PAUSING",
        description: "Pause in progress",
        normalized: NormalizedStatus::Pausing,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2502,
        name: "PAUSED",
        description: "Print paused",
        normalized: NormalizedStatus::Paused,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2505,
        name: "PAUSED_2",
        description: "Paused variant",
        normalized: NormalizedStatus::Paused,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2503,
        name: "STOPPING",
        description: "Stopping the print",
        normalized: NormalizedStatus::Stopping,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2504,
        name: "STOPPED",
        description: "Print stopped or cancelled",
        normalized: NormalizedStatus::Stopped,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2801,
        name: "HOMING",
        description: "Homing during print",
        normalized: NormalizedStatus::Homing,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2802,
        name: "HOMING_COMPLETED",
        description: "Mid-print homing completed",
        normalized: NormalizedStatus::Printing,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2901,
        name: "AUTO_LEVELING",
        description: "Leveling during print",
        normalized: NormalizedStatus::AutoLeveling,
    },
    StatusMapping {
        status: &[2],
        sub_status: 2902,
        name: "AUTO_LEVELING_COMPLETED",
        description: "Mid-print leveling completed",
        normalized: NormalizedStatus::Printing,
    },

    // Observed on real CC2/Canvas systems during operations.
    // This code is not part of the documented CC2 status table.
    StatusMapping {
        status: &[2, 10],
        sub_status: 1066,
        name: "EXTRUDER_TEMP_REACHED",
        description: "Extruder temperature reached",
        normalized: NormalizedStatus::ExtruderOperating,
    },

   // status = 3 / 4 FILAMENT_OPERATING
    StatusMapping {
        status: &[3, 4],
        sub_status: 1133,
        name: "FILAMENT_LOADING",
        description: "Loading filament",
        normalized: NormalizedStatus::FilamentOperating,
    },
    StatusMapping {
        status: &[3, 4],
        sub_status: 1134,
        name: "FILAMENT_LOADING_2",
        description: "Filament loading phase 2",
        normalized: NormalizedStatus::FilamentOperating,
    },
    StatusMapping {
        status: &[3, 4],
        sub_status: 1135,
        name: "FILAMENT_LOADING_3",
        description: "Filament loading phase 3",
        normalized: NormalizedStatus::FilamentOperating,
    },
    StatusMapping {
        status: &[3, 4],
        sub_status: 1136,
        name: "FILAMENT_LOADING_COMPLETED",
        description: "Filament loading completed",
        normalized: NormalizedStatus::Idle,
    },
    StatusMapping {
        status: &[3, 4],
        sub_status: 1143,
        name: "FILAMENT_PRE_UNLOAD",
        description: "Preparing to unload filament",
        normalized: NormalizedStatus::FilamentOperating,
    },
    StatusMapping {
        status: &[3, 4],
        sub_status: 1144,
        name: "FILAMENT_UNLOADING",
        description: "Unloading filament",
        normalized: NormalizedStatus::FilamentOperating,
    },
    StatusMapping {
        status: &[3, 4],
        sub_status: 1145,
        name: "FILAMENT_UNLOADING_COMPLETED",
        description: "Filament unloading completed",
        normalized: NormalizedStatus::Idle,
    },

    // status = 5 AUTO_LEVELING
    StatusMapping {
        status:&[5],
        sub_status: 2901,
        name: "AL_AUTO_LEVELING",
        description: "Bed probing in progress",
        normalized: NormalizedStatus::AutoLeveling,
    },
    StatusMapping {
        status:&[5],
        sub_status: 2902,
        name: "AL_AUTO_LEVELING_COMPLETED",
        description: "Bed leveling completed",
        normalized: NormalizedStatus::Idle,
    },

    // status = 6 PID_CALIBRATING
    StatusMapping {
        status:&[6],
        sub_status: 1503,
        name: "PID_CALIBRATING",
        description: "PID calibration running",
        normalized: NormalizedStatus::PidCalibrating,
    },
    StatusMapping {
        status:&[6],
        sub_status: 1504,
        name: "PID_CALIBRATING_2",
        description: "PID calibration phase 2",
        normalized: NormalizedStatus::PidCalibrating,
    },
    StatusMapping {
        status:&[6],
        sub_status: 1505,
        name: "PID_CALIBRATING_COMPLETED",
        description: "PID calibration completed",
        normalized: NormalizedStatus::Idle,
    },
    StatusMapping {
        status:&[6],
        sub_status: 1506,
        name: "PID_CALIBRATING_FAILED",
        description: "PID calibration failed",
        normalized: NormalizedStatus::Error,
    },

    // status = 7 RESONANCE_TESTING
    StatusMapping {
        status:&[7],
        sub_status: 5934,
        name: "RESONANCE_TEST",
        description: "Resonance test running",
        normalized: NormalizedStatus::ResonanceTesting,
    },
    StatusMapping {
        status:&[7],
        sub_status: 5935,
        name: "RESONANCE_TEST_COMPLETED",
        description: "Resonance test completed",
        normalized: NormalizedStatus::Idle,
    },
    StatusMapping {
        status:&[7],
        sub_status: 5936,
        name: "RESONANCE_TEST_FAILED",
        description: "Resonance test failed",
        normalized: NormalizedStatus::Error,
    },

    // status = 9 UPDATING
    StatusMapping {
        status:&[9],
        sub_status: 2061,
        name: "UPDATING_INIT",
        description: "Firmware update initializing",
        normalized: NormalizedStatus::Updating,
    },
    StatusMapping {
        status:&[9],
        sub_status: 2071,
        name: "UPDATING_1",
        description: "Firmware update phase 1",
        normalized: NormalizedStatus::Updating,
    },
    StatusMapping {
        status:&[9],
        sub_status: 2072,
        name: "UPDATING_2",
        description: "Firmware update phase 2",
        normalized: NormalizedStatus::Updating,
    },
    StatusMapping {
        status:&[9],
        sub_status: 2073,
        name: "UPDATING_3",
        description: "Firmware update phase 3",
        normalized: NormalizedStatus::Updating,
    },
    StatusMapping {
        status:&[9],
        sub_status: 2074,
        name: "UPDATING_COMPLETED",
        description: "Firmware update completed",
        normalized: NormalizedStatus::Idle,
    },
    StatusMapping {
        status:&[9],
        sub_status: 2075,
        name: "UPDATING_FAILED",
        description: "Firmware update failed",
        normalized: NormalizedStatus::Error,
    },

    // status = 10 HOMING
    StatusMapping {
        status:&[10],
        sub_status: 2801,
        name: "H_HOMING",
        description: "Homing in progress",
        normalized: NormalizedStatus::Homing,
    },
    StatusMapping {
        status:&[10],
        sub_status: 2802,
        name: "H_HOMING_COMPLETED",
        description: "Homing completed",
        normalized: NormalizedStatus::Idle,
    },
    StatusMapping {
        status:&[10],
        sub_status: 2803,
        name: "H_HOMING_FAILED",
        description: "Homing failed",
        normalized: NormalizedStatus::Error,
    },

    // status = 11 FILE_TRANSFERRING
    StatusMapping {
        status:&[11],
        sub_status: 3000,
        name: "UPLOADING_FILE",
        description: "File upload in progress",
        normalized: NormalizedStatus::FileTransferring,
    },
    StatusMapping {
        status:&[11],
        sub_status: 3001,
        name: "UPLOADING_FILE_COMPLETED",
        description: "File upload completed",
        normalized: NormalizedStatus::Idle,
    },

    // status = 13 EXTRUDER_OPERATING
    StatusMapping {
        status:&[13],
        sub_status: 1061,
        name: "EXTRUDER_LOADING",
        description: "Extruder filament loading",
        normalized: NormalizedStatus::ExtruderOperating,
    },
    StatusMapping {
        status:&[13],
        sub_status: 1062,
        name: "EXTRUDER_UNLOADING",
        description: "Extruder filament unloading",
        normalized: NormalizedStatus::ExtruderOperating,
    },
    StatusMapping {
        status:&[13],
        sub_status: 1063,
        name: "EXTRUDER_LOADING_COMPLETED",
        description: "Extruder loading completed",
        normalized: NormalizedStatus::Idle,
    },
    StatusMapping {
        status:&[13],
        sub_status: 1064,
        name: "EXTRUDER_UNLOADING_COMPLETED",
        description: "Extruder unloading completed",
        normalized: NormalizedStatus::Idle,
    },
];

pub fn lookup_status(status: i64, sub_status: i64) -> Option<&'static StatusMapping> {
    STATUS_MAPPINGS.iter().find(|mapping| {
        mapping.status.contains(&status) && mapping.sub_status == sub_status
    })
}

pub const fn normalize_machine_status(status: i64) -> NormalizedStatus {
    match status {
        -1 => NormalizedStatus::Offline,
        0 => NormalizedStatus::Initializing,
        1 => NormalizedStatus::Idle,
        2 => NormalizedStatus::Printing,
        3 | 4 => NormalizedStatus::FilamentOperating,
        5 => NormalizedStatus::AutoLeveling,
        6 => NormalizedStatus::PidCalibrating,
        7 => NormalizedStatus::ResonanceTesting,
        8 => NormalizedStatus::SelfChecking,
        9 => NormalizedStatus::Updating,
        10 => NormalizedStatus::Homing,
        11 => NormalizedStatus::FileTransferring,
        12 => NormalizedStatus::VideoComposing,
        13 => NormalizedStatus::ExtruderOperating,
        14 => NormalizedStatus::EmergencyStop,
        15 => NormalizedStatus::PowerLossRecovery,

        998 => NormalizedStatus::Busy,
        999 => NormalizedStatus::Error,
        1000 => NormalizedStatus::IdNotMatch,
        1001 => NormalizedStatus::AuthError,

        _ => NormalizedStatus::Unknown,
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extruder_preheating_is_resolved() {
        let mapping = lookup_status(2, 1045).unwrap();

        assert_eq!(mapping.name, "EXTRUDER_PREHEATING");
        assert_eq!(mapping.description, "Nozzle heating up");
        assert_eq!(
            mapping.normalized,
            NormalizedStatus::Preheating
        );
    }

    #[test]
    fn bed_preheating_is_resolved() {
        let mapping = lookup_status(2, 1405).unwrap();

        assert_eq!(mapping.name, "BED_PREHEATING");
        assert_eq!(mapping.description, "Bed heating up");
        assert_eq!(
            mapping.normalized,
            NormalizedStatus::Preheating
        );
    }

    #[test]
    fn printing_is_resolved() {
        let mapping = lookup_status(2, 2075).unwrap();

        assert_eq!(mapping.name, "PRINTING");
        assert_eq!(
            mapping.normalized,
            NormalizedStatus::Printing
        );
    }

    #[test]
    fn paused_is_resolved() {
        let mapping = lookup_status(2, 2502).unwrap();

        assert_eq!(mapping.name, "PAUSED");
        assert_eq!(
            mapping.normalized,
            NormalizedStatus::Paused
        );
    }

    #[test]
    fn auto_leveling_during_print_is_resolved() {
        let mapping = lookup_status(2, 2901).unwrap();

        assert_eq!(mapping.name, "AUTO_LEVELING");
        assert_eq!(
            mapping.normalized,
            NormalizedStatus::AutoLeveling
        );
    }

    #[test]
    fn homing_during_print_is_resolved() {
        let mapping = lookup_status(2, 2801).unwrap();

        assert_eq!(mapping.name, "HOMING");
        assert_eq!(
            mapping.normalized,
            NormalizedStatus::Homing
        );
    }

    #[test]
    fn canvas_filament_change_is_resolved() {
        let mapping = lookup_status(2, 1066).unwrap();

        assert_eq!(mapping.name, "EXTRUDER_TEMP_WAIT");
        assert_eq!(
            mapping.normalized,
            NormalizedStatus::ExtruderOperating
        );
    }

    #[test]
    fn unknown_sub_status_returns_none() {
        assert!(lookup_status(2, 999999).is_none());
    }

    #[test]
    fn main_status_is_normalized() {
        assert_eq!(
            normalize_machine_status(0),
            NormalizedStatus::Initializing
        );

        assert_eq!(
            normalize_machine_status(1),
            NormalizedStatus::Idle
        );

        assert_eq!(
            normalize_machine_status(2),
            NormalizedStatus::Printing
        );

        assert_eq!(
            normalize_machine_status(5),
            NormalizedStatus::AutoLeveling
        );

        assert_eq!(
            normalize_machine_status(10),
            NormalizedStatus::Homing
        );

        assert_eq!(
            normalize_machine_status(13),
            NormalizedStatus::ExtruderOperating
        );
    }

    #[test]
    fn unknown_main_status_is_unknown() {
        assert_eq!(
            normalize_machine_status(12345),
            NormalizedStatus::Unknown
        );
    }

    #[test]
    fn offline_status_is_normalized() {
        assert_eq!(
            normalize_machine_status(-1),
            NormalizedStatus::Offline
        );
    }
}

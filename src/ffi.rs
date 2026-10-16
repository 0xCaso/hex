//! C ABI (`include/hex.h`) for the native macOS AppKit/SwiftUI front end.
//!
//! Keeps `libhex` as the single source of truth for audio, hotkeys, ASR,
//! model preparation, settings, history, and meetings while delivering
//! coalesced main-actor wakeup ticks and lock-free 60-120 Hz HUD telemetry.

#![allow(non_camel_case_types)]

use std::collections::{BTreeMap, VecDeque};
use std::ffi::{c_char, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::slice;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::app_settings::{
    AppSettings, HotkeyBinding, HotkeyKey, ModifierSide, RecordingAudioBehavior,
};
use crate::commands::CommandScope;
use crate::desktop_activity::DesktopActivity;
use crate::dictation_indicator::{
    self, DictationIndicatorEvent, DictationIndicatorSender, HudTuning,
};
use crate::dictation_processor::ModelCatalog;
use crate::events::{EventReader, VoiceEvent};
use crate::history::{History, HistoryEntry, HistoryRetention};
use crate::meeting::{self, MeetingManifest, MeetingRequest, MeetingSource, MeetingStatus};
use crate::meeting_detection::MeetingCandidate;
use crate::meeting_watcher::{
    ControllerCommand, ControllerEvent, MeetingMenuState, OFFER_TIMEOUT, READY_TIMEOUT,
};
use crate::onboarding::{self, PermissionState};
use crate::personal_commands::{StatusContext, StatusSnapshot};
use crate::recognition::{CommandModelStatus, RecognitionControl};
use crate::status_item::{meeting_menu, model_menu_entries};
use crate::transcription_models::{
    LANGUAGES, MODELS, ModelRuntime, TranscriptionModelId, TranscriptionSelection, is_installed,
};
use crate::transcription_preparation::{PreparationStatus, TranscriptionPreparation};

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct hex_slice_s {
    pub ptr: *const c_char,
    pub len: usize,
}

impl Default for hex_slice_s {
    fn default() -> Self {
        Self {
            ptr: ptr::null(),
            len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum hex_hud_phase_e {
    #[default]
    HEX_HUD_PHASE_HIDDEN = 0,
    HEX_HUD_PHASE_LISTENING = 1,
    HEX_HUD_PHASE_LOCKED = 2,
    HEX_HUD_PHASE_TRANSCRIBING = 3,
    HEX_HUD_PHASE_VOICE_ACTION = 4,
    HEX_HUD_PHASE_COMPLETED = 5,
    HEX_HUD_PHASE_CANCELLED = 6,
    HEX_HUD_PHASE_ERROR = 7,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct hex_hud_telemetry_s {
    pub phase: hex_hud_phase_e,
    pub rms_level: f32,
    pub peak_level: f32,
    pub active_queue_count: u32,
    pub mode_color_rgba: u32,
    pub phase_started_ns: u64,
    pub is_command_mode_awake: bool,
    pub is_voice_action: bool,
    pub is_post_processing: bool,
    pub is_capturing: bool,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum hex_event_tag_e {
    HEX_EVENT_READINESS_CHANGED = 1,
    HEX_EVENT_SETTINGS_CHANGED = 2,
    HEX_EVENT_MODEL_PREP_UPDATED = 3,
    HEX_EVENT_SHORTCUT_CAPTURE_UPDATED = 4,
    HEX_EVENT_HISTORY_UPDATED = 5,
    HEX_EVENT_ACTIVITY_APPENDED = 6,
    HEX_EVENT_MEETING_STATE_CHANGED = 7,
    HEX_EVENT_MEETING_TRANSCRIPT_DELTA = 8,
    HEX_EVENT_OPENCODE_CATALOG_UPDATED = 9,
    HEX_EVENT_PERSONAL_COMMANDS_UPDATED = 10,
    HEX_EVENT_DEV_COMMAND_REQUESTED = 11,
    HEX_EVENT_REQUEST_CHECK_FOR_UPDATES = 12,
    HEX_EVENT_HUD_CHANGED = 13,
    HEX_EVENT_MEETING_PANEL_CHANGED = 14,
    HEX_EVENT_QUIT_REQUESTED = 15,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct hex_event_s {
    pub tag: hex_event_tag_e,
    pub generation: u64,
    pub json_payload: hex_slice_s,
}

pub type hex_wakeup_cb = Option<unsafe extern "C" fn(userdata: *mut c_void)>;
pub type hex_event_cb =
    Option<unsafe extern "C" fn(userdata: *mut c_void, event: *const hex_event_s)>;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct hex_runtime_config_s {
    pub userdata: *mut c_void,
    pub wakeup_cb: hex_wakeup_cb,
    pub event_cb: hex_event_cb,
    pub developer_features_enabled: bool,
    pub start_workers: bool,
    pub preview_dictation_hud: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexPermissionSnapshot {
    pub microphone_granted: bool,
    pub microphone_state: String,
    pub accessibility_granted: bool,
    pub input_monitoring_granted: bool,
    pub model_installed: bool,
    pub screen_recording_granted: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexLanguageOption {
    pub code: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HexTranscriptionModelRow {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub realtime: String,
    pub size_label: String,
    pub quality: String,
    pub quality_context: String,
    pub coverage: String,
    pub languages: Vec<String>,
    pub supports_recognition_hints: bool,
    pub installed: bool,
    pub active: bool,
    pub recommended_for_language: bool,
    pub recommendation_label: Option<String>,
    pub can_delete: bool,
    pub preparing: bool,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub progress_fraction: Option<f32>,
    pub status_label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HexTranscriptionCatalogSnapshot {
    pub selected_language: String,
    pub languages: Vec<HexLanguageOption>,
    pub models: Vec<HexTranscriptionModelRow>,
    pub preparation_error: Option<String>,
    pub deletion_error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexOpenCodeModelChoice {
    pub key: String,
    pub name: String,
    pub provider: String,
    pub variants: Vec<String>,
    pub recommended_variant: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexOpenCodeCatalogSnapshot {
    pub state: String,
    pub error: Option<String>,
    pub default_key: Option<String>,
    pub default_name: Option<String>,
    pub default_variant: Option<String>,
    pub models: Vec<HexOpenCodeModelChoice>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexCommandCatalogItem {
    pub id: String,
    pub group: String,
    pub phrase: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub scope: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexTransformationCatalogItem {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexCommandsSnapshot {
    pub command_model_status: String,
    pub workspace_path: String,
    pub config_file_path: String,
    pub workspace_exists: bool,
    pub workspace_provisioning: bool,
    pub last_reload_error: Option<String>,
    pub current_context_label: String,
    pub catalog: Vec<HexCommandCatalogItem>,
    pub transformations: Vec<HexTransformationCatalogItem>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexMeetingRowSnapshot {
    pub id: String,
    pub title: String,
    pub display_title: String,
    pub status: String,
    pub created_at_ms: u64,
    pub duration_ms: u64,
    pub duration_label: String,
    pub summarizing: bool,
    pub notes_title: Option<String>,
    pub notes_summary: Option<String>,
    pub notes_markdown: Option<String>,
    pub export_path: Option<String>,
    pub export_error: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexMeetingTurnSnapshot {
    pub source: String,
    pub clock: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub pause_before_ms: Option<u64>,
    pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexMeetingDetailSnapshot {
    pub manifest: HexMeetingRowSnapshot,
    pub turns: Vec<HexMeetingTurnSnapshot>,
    pub transcript_markdown: String,
    pub transcript_error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexActivityEventSnapshot {
    pub kind: String,
    pub timestamp_ms: u64,
    pub summary: String,
    pub raw_json: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexActivitySnapshot {
    pub listener_state: Option<String>,
    pub device: Option<String>,
    pub error: Option<String>,
    pub events: Vec<HexActivityEventSnapshot>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HexInstalledApplicationSnapshot {
    pub name: String,
    pub bundle_id: Option<String>,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HexMenuModelItem {
    pub model_id: String,
    pub title: String,
    pub selected: bool,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HexMeetingMenuSnapshot {
    pub state: String,
    pub visible: bool,
    pub status_title: Option<String>,
    pub button_title: String,
    pub recording_icon: bool,
    pub can_start: bool,
    pub can_stop: bool,
    pub started_elapsed_ms: u64,
    pub raw_title: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HexStatusMenuSnapshot {
    pub models: Vec<HexMenuModelItem>,
    pub preparing_model_title: Option<String>,
    pub model_switch_error: Option<String>,
    pub meeting: HexMeetingMenuSnapshot,
    pub updates_managed: bool,
    pub managed_updates_label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HexMeetingPanelSnapshot {
    #[default]
    Hidden,
    Offer {
        key: String,
        title: String,
        subtitle: String,
        source_label: String,
        remaining_seconds: u64,
    },
    Recording {
        title: String,
        app_label: String,
        elapsed_text: String,
    },
    CallEnded {
        app_label: String,
        title: String,
        subtitle: String,
    },
    Ready {
        meeting_id: String,
        title: String,
        subtitle: String,
        export_path: Option<String>,
        summarizing: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HexBootstrapSnapshot {
    pub version: String,
    pub developer_features_enabled: bool,
    pub pid: u32,
    pub instance_lock_held: bool,
    pub instance_conflict_error: Option<String>,
    pub setup_ready: bool,
    pub onboarding_completed: bool,
    pub permissions: HexPermissionSnapshot,
    pub status_menu: HexStatusMenuSnapshot,
    pub hud_telemetry: hex_hud_telemetry_s,
    pub meeting_panel: HexMeetingPanelSnapshot,
    pub active_model_id: String,
    pub active_language: String,
    pub commands_enabled: bool,
    pub voice_action_enabled: bool,
    pub meetings_enabled: bool,
    pub release_microphone_while_idle: bool,
    pub show_dock_icon: bool,
    pub sound_volume: f32,
    pub settings: AppSettings,
    pub dictation_hotkey_keycaps: Vec<String>,
    pub dictation_hotkey_side: Option<String>,
    pub edit_hotkey_keycaps: Vec<String>,
    pub edit_hotkey_side: Option<String>,
    pub paste_last_hotkey_keycaps: Vec<String>,
    pub paste_last_hotkey_side: Option<String>,
    pub available_microphones: Vec<String>,
    pub resolved_meeting_output_folder: String,
    pub apple_intelligence_available: bool,
    pub apple_intelligence_status: String,
    pub transcription_catalog: HexTranscriptionCatalogSnapshot,
    pub opencode_catalog: HexOpenCodeCatalogSnapshot,
    pub commands_snapshot: HexCommandsSnapshot,
    pub meetings: Vec<HexMeetingRowSnapshot>,
    pub activity: HexActivitySnapshot,
    pub installed_applications: Vec<HexInstalledApplicationSnapshot>,
}

impl Default for HexBootstrapSnapshot {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            developer_features_enabled: false,
            pid: 0,
            instance_lock_held: true,
            instance_conflict_error: None,
            setup_ready: false,
            onboarding_completed: false,
            permissions: HexPermissionSnapshot::default(),
            status_menu: HexStatusMenuSnapshot {
                models: Vec::new(),
                preparing_model_title: None,
                model_switch_error: None,
                meeting: HexMeetingMenuSnapshot {
                    state: "idle".to_string(),
                    visible: false,
                    status_title: None,
                    button_title: "Start Recording".to_string(),
                    recording_icon: false,
                    can_start: true,
                    can_stop: false,
                    started_elapsed_ms: 0,
                    raw_title: None,
                },
                updates_managed: false,
                managed_updates_label: String::new(),
            },
            hud_telemetry: hex_hud_telemetry_s::default(),
            meeting_panel: HexMeetingPanelSnapshot::Hidden,
            active_model_id: TranscriptionModelId::default().as_str().to_string(),
            active_language: "en".to_string(),
            commands_enabled: false,
            voice_action_enabled: false,
            meetings_enabled: false,
            release_microphone_while_idle: false,
            show_dock_icon: true,
            sound_volume: 0.5,
            settings: AppSettings::default(),
            dictation_hotkey_keycaps: vec!["⌥".to_string()],
            dictation_hotkey_side: Some("either".to_string()),
            edit_hotkey_keycaps: vec!["⌥".to_string(), "⌘".to_string()],
            edit_hotkey_side: Some("either".to_string()),
            paste_last_hotkey_keycaps: vec!["⌥".to_string(), "⇧".to_string(), "V".to_string()],
            paste_last_hotkey_side: None,
            available_microphones: Vec::new(),
            resolved_meeting_output_folder: String::new(),
            apple_intelligence_available: false,
            apple_intelligence_status: String::new(),
            transcription_catalog: HexTranscriptionCatalogSnapshot::default(),
            opencode_catalog: HexOpenCodeCatalogSnapshot::default(),
            commands_snapshot: HexCommandsSnapshot::default(),
            meetings: Vec::new(),
            activity: HexActivitySnapshot::default(),
            installed_applications: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HexEngineCommand {
    RefreshSnapshot,
    SelectInstalledModel {
        model_id: String,
    },
    ChooseTranscriptionModel {
        model_id: String,
        language: String,
        recognition_hints: Option<String>,
    },
    SetTranscriptionLanguage {
        language: String,
    },
    SetRecognitionHints {
        hints: String,
    },
    DeleteTranscriptionModel {
        model_id: String,
    },
    CancelModelPreparation,
    PasteLastDictation,
    PasteMeetingTurns,
    StartMeeting,
    StopMeeting,
    AcceptMeetingOffer,
    DismissMeetingOffer,
    DismissCallEndedPrompt,
    RevealMeetingTranscript {
        path: String,
    },
    RenameMeeting {
        id: String,
        title: String,
    },
    DeleteMeeting {
        id: String,
    },
    SummarizeMeeting {
        id: String,
    },
    RefreshMeetings,
    SetSoundVolume {
        volume: f32,
    },
    SetShowDockIcon {
        visible: bool,
    },
    SetMeetingsEnabled {
        enabled: bool,
    },
    SetMeetingNotesEnabled {
        enabled: bool,
    },
    SetMeetingOutputFolder {
        path: Option<String>,
    },
    SaveSettings {
        settings: AppSettings,
    },
    RequestPermission {
        kind: String,
    },
    OpenPermissionSettings {
        kind: String,
    },
    RecordOnboardingCompletion,
    SetHotkeyCaptureActive {
        active: bool,
    },
    SetHotkeyBinding {
        target: String,
        binding: Option<HotkeyBinding>,
    },
    CaptureHotkey {
        target: String,
        key: Option<String>,
        key_code: Option<u16>,
        modifier_flags: u64,
    },
    SetHotkeySide {
        target: String,
        side: String,
    },
    SetDoubleTapLock {
        enabled: bool,
    },
    SetDoubleTapOnly {
        enabled: bool,
    },
    SetIncrementalTranscription {
        enabled: bool,
    },
    SetRecordingAudioBehavior {
        behavior: RecordingAudioBehavior,
    },
    SetMicrophone {
        device: Option<String>,
    },
    RefreshMicrophones,
    SetReleaseMicrophoneWhileIdle {
        enabled: bool,
        #[serde(default)]
        confirm_disable_commands: bool,
    },
    SetCommandsEnabled {
        enabled: bool,
        #[serde(default)]
        confirm_keep_microphone_ready: bool,
    },
    RetryCommandsModel,
    ProvisionPersonalCommandsWorkspace,
    OpenPersonalCommandsConfig,
    CopyPersonalCommandsAgentPrompt,
    SetVoiceActionEnabled {
        enabled: bool,
    },
    SetVoiceActionModel {
        model: Option<String>,
        variant: Option<String>,
        deadline_seconds: Option<u64>,
    },
    ReloadOpenCodeCatalog,
    OpenOpenCodeSetupDocs,
    DiscoverInstalledApplications,
    SetHistoryRetention {
        retention: HistoryRetention,
    },
    DeleteHistoryEntry {
        id: u64,
    },
    ClearHistory,
    RefreshActivity,
    ConfigureHudTuning {
        style: f32,
        line_count: f32,
        curvature: f32,
        speed: f32,
        sharpness: f32,
        glow: f32,
        depth: f32,
        light_angle: f32,
        outline: f32,
    },
    SimulateHudState {
        state: String,
        rms: Option<f32>,
        peak: Option<f32>,
    },
    SimulateMeetingPanel {
        panel: HexMeetingPanelSnapshot,
    },
    CompleteOnboarding,
    RetryInstanceLock,
    QuitOtherHexInstancesAndRetry,
    RequestCheckForUpdates,
    RequestQuit,
}

pub struct AtomicHudTelemetry {
    phase: AtomicU32,
    rms_bits: AtomicU32,
    peak_bits: AtomicU32,
    active_queue_count: AtomicU32,
    mode_color_rgba: AtomicU32,
    phase_started_ns: AtomicU64,
    flags: AtomicU32,
}

impl Default for AtomicHudTelemetry {
    fn default() -> Self {
        let initial = Self {
            phase: AtomicU32::new(hex_hud_phase_e::HEX_HUD_PHASE_HIDDEN as u32),
            rms_bits: AtomicU32::new(0.0f32.to_bits()),
            peak_bits: AtomicU32::new(0.0f32.to_bits()),
            active_queue_count: AtomicU32::new(0),
            mode_color_rgba: AtomicU32::new(0xFF06_09FF),
            phase_started_ns: AtomicU64::new(0),
            flags: AtomicU32::new(0),
        };
        initial
    }
}

impl AtomicHudTelemetry {
    fn store(&self, t: &hex_hud_telemetry_s) {
        let mut flags = 0u32;
        if t.is_command_mode_awake {
            flags |= 1 << 0;
        }
        if t.is_voice_action {
            flags |= 1 << 1;
        }
        if t.is_post_processing {
            flags |= 1 << 2;
        }
        if t.is_capturing {
            flags |= 1 << 3;
        }
        self.rms_bits
            .store(t.rms_level.to_bits(), Ordering::Relaxed);
        self.peak_bits
            .store(t.peak_level.to_bits(), Ordering::Relaxed);
        self.active_queue_count
            .store(t.active_queue_count, Ordering::Relaxed);
        self.mode_color_rgba
            .store(t.mode_color_rgba, Ordering::Relaxed);
        self.phase_started_ns
            .store(t.phase_started_ns, Ordering::Relaxed);
        self.flags.store(flags, Ordering::Relaxed);
        self.phase.store(t.phase as u32, Ordering::Release);
    }

    fn load(&self) -> hex_hud_telemetry_s {
        let raw_phase = self.phase.load(Ordering::Acquire);
        let phase = match raw_phase {
            1 => hex_hud_phase_e::HEX_HUD_PHASE_LISTENING,
            2 => hex_hud_phase_e::HEX_HUD_PHASE_LOCKED,
            3 => hex_hud_phase_e::HEX_HUD_PHASE_TRANSCRIBING,
            4 => hex_hud_phase_e::HEX_HUD_PHASE_VOICE_ACTION,
            5 => hex_hud_phase_e::HEX_HUD_PHASE_COMPLETED,
            6 => hex_hud_phase_e::HEX_HUD_PHASE_CANCELLED,
            7 => hex_hud_phase_e::HEX_HUD_PHASE_ERROR,
            _ => hex_hud_phase_e::HEX_HUD_PHASE_HIDDEN,
        };
        let flags = self.flags.load(Ordering::Relaxed);
        hex_hud_telemetry_s {
            phase,
            rms_level: f32::from_bits(self.rms_bits.load(Ordering::Relaxed)),
            peak_level: f32::from_bits(self.peak_bits.load(Ordering::Relaxed)),
            active_queue_count: self.active_queue_count.load(Ordering::Relaxed),
            mode_color_rgba: self.mode_color_rgba.load(Ordering::Relaxed),
            phase_started_ns: self.phase_started_ns.load(Ordering::Relaxed),
            is_command_mode_awake: (flags & (1 << 0)) != 0,
            is_voice_action: (flags & (1 << 1)) != 0,
            is_post_processing: (flags & (1 << 2)) != 0,
            is_capturing: (flags & (1 << 3)) != 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrackedJobPhase {
    Queued,
    Transcribing,
    Processing,
}

struct HudTelemetryTracker {
    epoch: Instant,
    telemetry: hex_hud_telemetry_s,
    jobs: BTreeMap<u64, TrackedJobPhase>,
    atomic: Arc<AtomicHudTelemetry>,
}

impl Default for HudTelemetryTracker {
    fn default() -> Self {
        Self::with_atomic(Arc::new(AtomicHudTelemetry::default()))
    }
}

impl HudTelemetryTracker {
    fn with_atomic(atomic: Arc<AtomicHudTelemetry>) -> Self {
        let telemetry = hex_hud_telemetry_s {
            mode_color_rgba: 0xFF06_09FF,
            ..Default::default()
        };
        atomic.store(&telemetry);
        Self {
            epoch: Instant::now(),
            telemetry,
            jobs: BTreeMap::new(),
            atomic,
        }
    }
    fn now_ns(&self) -> u64 {
        self.epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
    }

    fn apply(&mut self, event: DictationIndicatorEvent) -> bool {
        let previous_phase = self.telemetry.phase;
        let previous_post = self.telemetry.is_post_processing;
        let previous_voice_action = self.telemetry.is_voice_action;
        let previous_queue = self.telemetry.active_queue_count;
        let now_ns = self.now_ns();

        match event {
            DictationIndicatorEvent::Reset => {
                self.jobs.clear();
                self.telemetry = hex_hud_telemetry_s {
                    phase: hex_hud_phase_e::HEX_HUD_PHASE_HIDDEN,
                    mode_color_rgba: 0xFF06_09FF,
                    phase_started_ns: now_ns,
                    ..Default::default()
                };
            }
            DictationIndicatorEvent::Configure(_) => {}
            DictationIndicatorEvent::Started => {
                self.telemetry.is_capturing = true;
                self.telemetry.is_voice_action = false;
                self.telemetry.phase = hex_hud_phase_e::HEX_HUD_PHASE_LISTENING;
                self.telemetry.phase_started_ns = now_ns;
                self.telemetry.rms_level = 0.0;
                self.telemetry.peak_level = 0.0;
                self.telemetry.mode_color_rgba = 0xFF06_09FF;
                self.sync_queue_and_post_processing();
            }
            DictationIndicatorEvent::EditingStarted => {
                self.telemetry.is_capturing = true;
                self.telemetry.is_voice_action = true;
                self.telemetry.phase = hex_hud_phase_e::HEX_HUD_PHASE_VOICE_ACTION;
                self.telemetry.phase_started_ns = now_ns;
                self.telemetry.rms_level = 0.0;
                self.telemetry.peak_level = 0.0;
                self.telemetry.mode_color_rgba = 0x0DD9_94FF;
                self.sync_queue_and_post_processing();
            }
            DictationIndicatorEvent::PromotedToVoiceAction => {
                self.telemetry.is_voice_action = true;
                if self.telemetry.is_capturing {
                    self.telemetry.phase = hex_hud_phase_e::HEX_HUD_PHASE_VOICE_ACTION;
                }
                self.telemetry.mode_color_rgba = 0x0DD9_94FF;
            }
            DictationIndicatorEvent::Meter { average, peak } => {
                self.telemetry.rms_level = (average * 9.0).clamp(0.0, 1.0);
                self.telemetry.peak_level = (peak * 3.0).clamp(0.0, 1.0);
            }
            DictationIndicatorEvent::Submitted { job_id } => {
                self.telemetry.is_capturing = false;
                self.telemetry.is_voice_action = false;
                self.jobs.insert(job_id, TrackedJobPhase::Queued);
                self.sync_pipeline(now_ns);
            }
            DictationIndicatorEvent::Transcribing { job_id } => {
                if let Some(phase) = self.jobs.get_mut(&job_id) {
                    *phase = TrackedJobPhase::Transcribing;
                    self.sync_pipeline(now_ns);
                }
            }
            DictationIndicatorEvent::Processing { job_id } => {
                if let Some(phase) = self.jobs.get_mut(&job_id) {
                    *phase = TrackedJobPhase::Processing;
                    self.sync_pipeline(now_ns);
                }
            }
            DictationIndicatorEvent::Discarded => {
                self.finish_capture(now_ns, hex_hud_phase_e::HEX_HUD_PHASE_HIDDEN);
            }
            DictationIndicatorEvent::Cancelled => {
                self.finish_capture(now_ns, hex_hud_phase_e::HEX_HUD_PHASE_CANCELLED);
            }
            DictationIndicatorEvent::JobCompleted { job_id } => {
                self.finish_job(job_id, now_ns, hex_hud_phase_e::HEX_HUD_PHASE_COMPLETED);
            }
            DictationIndicatorEvent::JobCancelled { job_id } => {
                self.finish_job(job_id, now_ns, hex_hud_phase_e::HEX_HUD_PHASE_CANCELLED);
            }
            DictationIndicatorEvent::JobFailed { job_id } => {
                self.finish_job(job_id, now_ns, hex_hud_phase_e::HEX_HUD_PHASE_ERROR);
            }
            DictationIndicatorEvent::Failed => {
                self.finish_capture(now_ns, hex_hud_phase_e::HEX_HUD_PHASE_ERROR);
            }
        }
        self.atomic.store(&self.telemetry);

        previous_phase != self.telemetry.phase
            || previous_post != self.telemetry.is_post_processing
            || previous_voice_action != self.telemetry.is_voice_action
            || previous_queue != self.telemetry.active_queue_count
    }

    fn sync_atomic(&self) {
        self.atomic.store(&self.telemetry);
    }

    fn finish_capture(&mut self, now_ns: u64, empty_phase: hex_hud_phase_e) {
        self.telemetry.is_capturing = false;
        self.telemetry.is_voice_action = false;
        self.telemetry.rms_level = 0.0;
        self.telemetry.peak_level = 0.0;
        if self.jobs.is_empty() {
            self.telemetry.phase = empty_phase;
            self.telemetry.phase_started_ns = now_ns;
            self.sync_queue_and_post_processing();
        } else {
            self.sync_pipeline(now_ns);
        }
    }

    fn finish_job(&mut self, job_id: u64, now_ns: u64, terminal: hex_hud_phase_e) {
        if self.jobs.remove(&job_id).is_none() {
            return;
        }
        if self.telemetry.is_capturing || !self.jobs.is_empty() {
            self.sync_pipeline(now_ns);
            return;
        }
        self.telemetry.phase = terminal;
        self.telemetry.phase_started_ns = now_ns;
        self.sync_queue_and_post_processing();
    }

    fn sync_pipeline(&mut self, now_ns: u64) {
        self.sync_queue_and_post_processing();
        if self.telemetry.is_capturing {
            return;
        }
        if !self.jobs.is_empty() {
            self.telemetry.phase = hex_hud_phase_e::HEX_HUD_PHASE_TRANSCRIBING;
            self.telemetry.phase_started_ns = now_ns;
            self.telemetry.rms_level = 0.0;
            self.telemetry.peak_level = 0.0;
        }
    }

    fn sync_queue_and_post_processing(&mut self) {
        let count = if self.telemetry.is_capturing {
            self.jobs.len()
        } else {
            self.jobs.len().saturating_sub(1)
        };
        self.telemetry.active_queue_count = count.min(u32::MAX as usize) as u32;
        self.telemetry.is_post_processing = self
            .jobs
            .first_key_value()
            .is_some_and(|(_, phase)| matches!(phase, TrackedJobPhase::Processing));
    }
}

struct QueuedEvent {
    tag: hex_event_tag_e,
    generation: u64,
    payload: Vec<u8>,
}

struct CallbackSink {
    userdata: usize,
    wakeup_cb: hex_wakeup_cb,
    event_cb: hex_event_cb,
    wakeup_pending: AtomicBool,
}

impl CallbackSink {
    fn notify_wakeup(&self) {
        if !self.wakeup_pending.swap(true, Ordering::AcqRel)
            && let Some(wakeup) = self.wakeup_cb
        {
            unsafe { wakeup(self.userdata as *mut c_void) };
        }
    }
}

#[derive(Clone, Debug)]
enum OpenCodeCatalogState {
    Idle,
    Loading,
    Loaded(ModelCatalog),
    Missing,
    Failed(String),
}

struct EngineCore {
    developer_features_enabled: bool,
    persist_settings: bool,
    start_workers: bool,
    preview_dictation_hud: bool,
    instance_lock_held: bool,
    instance_conflict_error: Option<String>,
    instance_guard: Option<crate::instance::InstanceLock>,
    local_api: Option<crate::local_api::LocalApi>,
    recognition_worker: Option<JoinHandle<()>>,
    controller_worker: Option<JoinHandle<()>>,
    event_log: Option<crate::events::EventLog>,
    developer_sender: SyncSender<crate::developer_control::DeveloperCall>,
    controller_event_sender: SyncSender<ControllerEvent>,
    settings: AppSettings,
    hud: HudTelemetryTracker,
    meeting_state: MeetingMenuState,
    meeting_panel: HexMeetingPanelSnapshot,
    pending_offer: Option<(MeetingCandidate, Instant)>,
    ready_panel_deadline: Option<Instant>,
    active_meeting_transcript_len: u64,
    transcription_picker_language: Option<String>,
    transcription_preparation: TranscriptionPreparation,
    transcription_deletion_error: Option<String>,
    available_microphones: Vec<String>,
    opencode_catalog: OpenCodeCatalogState,
    opencode_catalog_receiver: Option<Receiver<OpenCodeCatalogState>>,
    last_opencode_missing_check: Instant,
    personal_commands_status: StatusSnapshot,
    personal_workspace_provisioning: bool,
    personal_workspace_receiver: Option<Receiver<Result<StatusSnapshot, String>>>,
    personal_workspace_error: Option<String>,
    installed_applications: Vec<HexInstalledApplicationSnapshot>,
    app_discovery_receiver: Option<Receiver<Vec<HexInstalledApplicationSnapshot>>>,
    meetings_cache: Vec<MeetingManifest>,
    event_reader: Option<EventReader>,
    activity: DesktopActivity,
    recent_events: Vec<VoiceEvent>,
    current_context: (Option<String>, Option<String>),
    last_command_model_status: CommandModelStatus,
    history: Option<History>,
    recognition_controls: Option<SyncSender<RecognitionControl>>,
    meeting_commands: Option<SyncSender<ControllerCommand>>,
    meeting_requests: Option<SyncSender<MeetingRequest>>,
    indicator_sender: DictationIndicatorSender,
    mailbox: VecDeque<QueuedEvent>,
    next_generation: u64,
    callback_sink: Arc<CallbackSink>,
}

const MAX_MAILBOX_EVENTS: usize = 256;

fn standalone_modifier_side(binding: &HotkeyBinding) -> Option<ModifierSide> {
    if binding.key.is_some() {
        return None;
    }
    let mut sides = [
        binding.modifiers.control,
        binding.modifiers.option,
        binding.modifiers.shift,
        binding.modifiers.command,
    ]
    .into_iter()
    .flatten();
    let first = sides.next()?;
    sides.all(|side| side == first).then_some(first)
}

fn modifier_side_str(side: ModifierSide) -> &'static str {
    match side {
        ModifierSide::Left => "left",
        ModifierSide::Either => "either",
        ModifierSide::Right => "right",
    }
}

fn parse_modifier_side(side: &str) -> Option<ModifierSide> {
    match side {
        "left" => Some(ModifierSide::Left),
        "either" => Some(ModifierSide::Either),
        "right" => Some(ModifierSide::Right),
        _ => None,
    }
}

fn set_standalone_modifier_side(binding: &mut HotkeyBinding, side: ModifierSide) {
    if standalone_modifier_side(binding).is_none() {
        return;
    }
    for current in [
        &mut binding.modifiers.control,
        &mut binding.modifiers.option,
        &mut binding.modifiers.shift,
        &mut binding.modifiers.command,
    ] {
        if current.is_some() {
            *current = Some(side);
        }
    }
}

fn hotkey_binding_conflicts_for_target(
    settings: &AppSettings,
    target: &str,
    binding: &HotkeyBinding,
) -> bool {
    let mut others = match target {
        "dictation" => Vec::new(),
        "edit" | "paste_last" => vec![settings.dictation_hotkey.clone()],
        _ => Vec::new(),
    };
    if target != "edit" && settings.voice_action.enabled {
        others.push(settings.edit_hotkey.clone());
    }
    if target != "paste_last"
        && let Some(paste) = &settings.paste_last_hotkey
    {
        others.push(paste.clone());
    }
    if settings.meetings.available() {
        others.push(HotkeyBinding::paste_meeting_default());
    }
    crate::app_settings::hotkey_conflicts(binding, others)
}

fn resolve_hotkey_key(key: &str, key_code: Option<u16>) -> Result<HotkeyKey, &'static str> {
    let normalized = key.trim().to_lowercase();
    let special = match normalized.as_str() {
        "space" | " " => Some((49, "Space")),
        "tab" => Some((48, "Tab")),
        "enter" | "return" => Some((36, "Return")),
        "backspace" | "delete_backward" => Some((51, "Delete")),
        "up" | "arrowup" => Some((126, "Up")),
        "down" | "arrowdown" => Some((125, "Down")),
        "left" | "arrowleft" => Some((123, "Left")),
        "right" | "arrowright" => Some((124, "Right")),
        "pageup" => Some((116, "Page Up")),
        "pagedown" => Some((121, "Page Down")),
        "home" => Some((115, "Home")),
        "end" => Some((119, "End")),
        "delete" | "forwarddelete" => Some((117, "Forward Delete")),
        "insert" | "help" => Some((114, "Help")),
        "f1" => Some((122, "F1")),
        "f2" => Some((120, "F2")),
        "f3" => Some((99, "F3")),
        "f4" => Some((118, "F4")),
        "f5" => Some((96, "F5")),
        "f6" => Some((97, "F6")),
        "f7" => Some((98, "F7")),
        "f8" => Some((100, "F8")),
        "f9" => Some((101, "F9")),
        "f10" => Some((109, "F10")),
        "f11" => Some((103, "F11")),
        "f12" => Some((111, "F12")),
        "f13" => Some((105, "F13")),
        "f14" => Some((107, "F14")),
        "f15" => Some((113, "F15")),
        "f16" => Some((106, "F16")),
        "f17" => Some((64, "F17")),
        "f18" => Some((79, "F18")),
        "f19" => Some((80, "F19")),
        "f20" => Some((90, "F20")),
        _ => None,
    };
    if let Some((code, label)) = special {
        return Ok(HotkeyKey {
            code,
            label: label.into(),
        });
    }
    let mut chars = normalized.chars();
    let Some(ch) = chars.next() else {
        return Err("Unsupported key");
    };
    if chars.next().is_some() {
        return Err("Unsupported key");
    }
    let code = match crate::keyboard::key_code_for(ch) {
        Ok(c) => c,
        Err(_) => key_code.ok_or("Unsupported key")?,
    };
    Ok(HotkeyKey {
        code,
        label: ch.to_uppercase().collect(),
    })
}

fn is_function_key_label(label: &str) -> bool {
    label
        .strip_prefix('F')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=20).contains(&number))
}

fn apple_intelligence_status_copy() -> (bool, String) {
    use crate::apple_intelligence::Availability;
    match crate::apple_intelligence::availability() {
        Availability::Available => (true, "Available on this Mac".into()),
        Availability::AppleIntelligenceNotEnabled => (
            false,
            "Enable Apple Intelligence in System Settings to summarize meetings.".into(),
        ),
        Availability::ModelNotReady => (false, "Apple Intelligence model is downloading…".into()),
        Availability::DeviceNotEligible | Availability::UnsupportedSystem => (
            false,
            "Requires macOS 26 with Apple Intelligence on Apple Silicon.".into(),
        ),
        Availability::Unknown => (false, "Apple Intelligence is currently unavailable.".into()),
    }
}

fn start_opencode_catalog_worker(sink: Arc<CallbackSink>) -> Receiver<OpenCodeCatalogState> {
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let state = if !crate::dictation_processor::opencode_installed() {
            OpenCodeCatalogState::Missing
        } else {
            match crate::dictation_processor::load_model_catalog() {
                Ok(catalog) => OpenCodeCatalogState::Loaded(catalog),
                Err(error) => OpenCodeCatalogState::Failed(error.to_string()),
            }
        };
        let _ = tx.send(state);
        sink.notify_wakeup();
    });
    rx
}

fn start_personal_workspace_worker(
    sink: Arc<CallbackSink>,
) -> Receiver<Result<StatusSnapshot, String>> {
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let result = match crate::personal_commands::initialize_workspace() {
            Ok(_) => {
                let mut status = crate::personal_commands::load_status().unwrap_or_default();
                crate::personal_commands::include_builtin_transformations(&mut status);
                Ok(status)
            }
            Err(error) => Err(error.to_string()),
        };
        let _ = tx.send(result);
        sink.notify_wakeup();
    });
    rx
}

fn start_app_discovery_worker(
    sink: Arc<CallbackSink>,
) -> Receiver<Vec<HexInstalledApplicationSnapshot>> {
    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let apps = crate::application_catalog::discover()
            .into_iter()
            .map(|app| HexInstalledApplicationSnapshot {
                name: app.name,
                bundle_id: app.bundle_id,
                path: app.path.display().to_string(),
            })
            .collect();
        let _ = tx.send(apps);
        sink.notify_wakeup();
    });
    rx
}

fn manifest_to_row_snapshot(manifest: &MeetingManifest) -> HexMeetingRowSnapshot {
    let status = match manifest.status {
        MeetingStatus::Starting => "starting",
        MeetingStatus::Recording => "recording",
        MeetingStatus::Transcribing => "transcribing",
        MeetingStatus::Complete => "complete",
        MeetingStatus::Interrupted => "interrupted",
        MeetingStatus::Failed => "failed",
    };
    let duration_ms = manifest.duration_ms.unwrap_or_default();
    let minutes = duration_ms / 60_000;
    let duration_label = match minutes {
        0 => "< 1 min".to_string(),
        1..=59 => format!("{minutes} min"),
        _ => format!("{} h {:02} min", minutes / 60, minutes % 60),
    };
    HexMeetingRowSnapshot {
        id: manifest.id.clone(),
        title: manifest.title.clone(),
        display_title: meeting::display_title(manifest),
        status: status.to_string(),
        created_at_ms: manifest.created_at_ms,
        duration_ms,
        duration_label,
        summarizing: meeting::notes_in_progress(&manifest.id),
        notes_title: manifest
            .notes
            .as_ref()
            .map(|n| n.title.trim().to_string())
            .filter(|s| !s.is_empty()),
        notes_summary: manifest
            .notes
            .as_ref()
            .map(|n| n.summary.trim().to_string())
            .filter(|s| !s.is_empty()),
        notes_markdown: manifest.notes.as_ref().map(meeting::notes_markdown),
        export_path: manifest
            .export_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        export_error: manifest.export_error.clone(),
        error: manifest.error.clone(),
    }
}

impl EngineCore {
    fn commit_settings(&mut self, mut candidate: AppSettings) -> Result<(), String> {
        if candidate.commands_enabled && candidate.release_microphone_while_idle {
            return Err("voice commands require the microphone to remain ready".into());
        }
        if !candidate.double_tap_lock || candidate.dictation_hotkey.key.is_none() {
            candidate.double_tap_only = false;
        }
        candidate.dictation_processing.default_mode.name = "Global".into();
        crate::transcription_models::validate(&candidate.transcription)
            .map_err(|e| e.to_string())?;
        if self.persist_settings {
            candidate.save().map_err(|e| e.to_string())?;
        } else {
            candidate.apply_runtime();
        }
        self.settings = candidate;
        Ok(())
    }

    fn enqueue_event<T: Serialize>(&mut self, tag: hex_event_tag_e, payload: &T) {
        let Ok(bytes) = serde_json::to_vec(payload) else {
            return;
        };
        self.next_generation = self.next_generation.saturating_add(1);
        if self.mailbox.len() >= MAX_MAILBOX_EVENTS {
            self.mailbox.pop_front();
        }
        self.mailbox.push_back(QueuedEvent {
            tag,
            generation: self.next_generation,
            payload: bytes,
        });
        self.callback_sink.notify_wakeup();
    }

    fn start_runtime_workers_if_needed(&mut self) {
        if !self.start_workers || !self.instance_lock_held || self.preview_dictation_hud {
            return;
        }
        if self.recognition_worker.is_some() || self.controller_worker.is_some() {
            return;
        }

        let (controller_cmd_sender, controller_cmd_receiver) = mpsc::sync_channel(8);
        let (meeting_request_sender, meeting_request_receiver) = mpsc::sync_channel(8);
        let (recognition_control_sender, recognition_control_receiver) = mpsc::sync_channel(8);

        self.meeting_commands = Some(controller_cmd_sender);
        self.meeting_requests = Some(meeting_request_sender.clone());
        self.recognition_controls = Some(recognition_control_sender.clone());

        if self.local_api.is_none() {
            self.local_api = self.event_log.as_ref().and_then(|events| {
                crate::local_api::LocalApi::start(
                    events.clone(),
                    crate::local_api::LocalApiOptions {
                        developer_control: self
                            .developer_features_enabled
                            .then(|| self.developer_sender.clone()),
                        dictation_control: Some(recognition_control_sender),
                    },
                )
                .map_err(|error| {
                    tracing::warn!(%error, "could not start local API");
                    error
                })
                .ok()
            });
        }

        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let controller_events = self.controller_event_sender.clone();
        let worker_root = project_root.clone();
        self.controller_worker = Some(thread::spawn(move || {
            if let Err(error) = crate::meeting_watcher::controller_loop(
                &crate::SHUTDOWN,
                controller_events.clone(),
                controller_cmd_receiver,
                meeting_request_receiver,
                worker_root,
            ) {
                tracing::error!(%error, "meeting controller stopped");
                let _ = controller_events.send(ControllerEvent::Failed {
                    title: "Meeting detection stopped".into(),
                    error: error.to_string(),
                });
            }
        }));

        let setup_ready = onboarding::status(&self.settings.transcription).ready();
        self.recognition_worker = self.event_log.clone().map(|recognition_events| {
            let failure_indicator = self.indicator_sender.clone();
            let recognition_indicator = self.indicator_sender.clone();
            let meeting_requests = Some(meeting_request_sender);
            let recognition_history = self.history.clone();
            let worker_root = project_root;
            thread::spawn(move || {
                if !setup_ready {
                    tracing::info!(
                        "waiting for permissions and local model before starting hotkey listener"
                    );
                    while !crate::SHUTDOWN.load(Ordering::Relaxed) {
                        let (_, selection) = crate::app_settings::transcription_selection();
                        if onboarding::status(&selection).ready() {
                            let _ = onboarding::record_completion();
                            break;
                        }
                        thread::sleep(Duration::from_millis(500));
                    }
                    if crate::SHUTDOWN.load(Ordering::Relaxed) {
                        return;
                    }
                }
                if let Err(error) = crate::recognition::listen(
                    &worker_root,
                    recognition_events,
                    None,
                    crate::config::voice_control(),
                    &crate::SHUTDOWN,
                    Some(recognition_indicator),
                    meeting_requests,
                    recognition_history,
                    Some(recognition_control_receiver),
                ) {
                    tracing::error!(%error, "desktop recognition stopped");
                    failure_indicator.send(DictationIndicatorEvent::Failed);
                }
            })
        });
    }

    fn try_acquire_instance_lock(&mut self) -> Result<(), String> {
        if self.instance_lock_held {
            self.instance_conflict_error = None;
            return Ok(());
        }
        match crate::instance::acquire("listener") {
            Ok(guard) => {
                self.instance_guard = Some(guard);
                self.instance_lock_held = true;
                self.instance_conflict_error = None;
                if self.event_log.is_none() {
                    self.event_log = crate::app_paths::logs_dir()
                        .ok()
                        .map(|dir| dir.join("live.ndjson"))
                        .and_then(|path| crate::events::EventLog::create(&path).ok());
                }
                if self.event_reader.is_none() {
                    self.event_reader = crate::app_paths::logs_dir()
                        .ok()
                        .map(|dir| EventReader::open(dir.join("live.ndjson")));
                }
                if self.history.is_none() {
                    self.history = History::open_default(self.settings.history_retention).ok();
                }
                self.start_runtime_workers_if_needed();
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            Err(error) => {
                let message = format!(
                    "Another HEX instance is already running ({error}). Quit the other HEX process and retry."
                );
                self.instance_conflict_error = Some(message.clone());
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Err(message)
            }
        }
    }

    fn quit_other_instances_and_retry(&mut self) -> Result<(), String> {
        if self.instance_lock_held {
            self.instance_conflict_error = None;
            return Ok(());
        }
        let my_pid = std::process::id();
        if let Ok(support_dir) = crate::app_paths::support_dir() {
            let lock_path = support_dir.join("listener.lock");
            if let Ok(output) = std::process::Command::new("/usr/sbin/lsof")
                .arg("-t")
                .arg(&lock_path)
                .output()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Ok(pid) = line.trim().parse::<u32>()
                        && pid != my_pid
                    {
                        let _ = std::process::Command::new("/bin/kill")
                            .arg("-TERM")
                            .arg(pid.to_string())
                            .status();
                    }
                }
            }
        }
        for _ in 0..25 {
            if crate::instance::acquire("listener").is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(60));
        }
        self.try_acquire_instance_lock()
    }

    fn permission_snapshot(&self) -> (bool, HexPermissionSnapshot) {
        let status = onboarding::status(&self.settings.transcription);
        let ready = status.ready();
        if self.persist_settings
            && self.instance_lock_held
            && ready
            && !onboarding::completion_recorded()
        {
            let _ = onboarding::record_completion();
        }
        let mic_state = match status.microphone {
            PermissionState::Ready => "ready",
            PermissionState::NeedsRequest => "needs_request",
            PermissionState::NeedsSettings => "needs_settings",
        };
        (
            ready && self.instance_lock_held,
            HexPermissionSnapshot {
                microphone_granted: status.microphone == PermissionState::Ready,
                microphone_state: mic_state.to_string(),
                accessibility_granted: status.accessibility == PermissionState::Ready,
                input_monitoring_granted: status.input_monitoring == PermissionState::Ready,
                model_installed: status.transcription_model,
                screen_recording_granted: meeting::screen_recording_permitted(),
            },
        )
    }

    fn transcription_catalog_snapshot(&self) -> HexTranscriptionCatalogSnapshot {
        let selected_language = self
            .transcription_picker_language
            .clone()
            .unwrap_or_else(|| self.settings.transcription.language.clone());
        let prep_status = self.transcription_preparation.status();
        let recommendations = crate::transcription_models::choices_for_runtime(&selected_language);
        let languages = LANGUAGES
            .iter()
            .map(|(code, name)| HexLanguageOption {
                code: (*code).to_string(),
                name: (*name).to_string(),
            })
            .collect();
        let models = MODELS
            .iter()
            .filter(|def| def.available())
            .map(|def| {
                let installed = is_installed(def, &selected_language);
                let active = installed
                    && self.settings.transcription.model == def.id
                    && self.settings.transcription.language == selected_language;
                let rec = recommendations.iter().find(|c| c.model.id == def.id);
                let preparing = prep_status.model == Some(def.id);
                let total_bytes = def.download_bytes();
                let progress_fraction = if preparing {
                    total_bytes.map(|total| {
                        (prep_status.downloaded_bytes as f32 / total as f32).clamp(0.0, 1.0)
                    })
                } else {
                    None
                };
                let status_label = if preparing {
                    if installed {
                        "Loading model…".to_string()
                    } else if let Some(frac) = progress_fraction {
                        format!("Downloading {:.0}%", frac * 100.0)
                    } else {
                        "Preparing…".to_string()
                    }
                } else if active {
                    "Active".to_string()
                } else if installed {
                    "Downloaded".to_string()
                } else {
                    "Not downloaded".to_string()
                };
                let can_delete = installed
                    && matches!(def.runtime, ModelRuntime::Gguf(_))
                    && self.settings.transcription.model != def.id
                    && prep_status.model != Some(def.id);
                HexTranscriptionModelRow {
                    id: def.id.as_str().to_string(),
                    name: def.name.to_string(),
                    summary: format!(
                        "{} · {} WER ({})",
                        def.coverage, def.quality, def.quality_context
                    ),
                    realtime: def.realtime.to_string(),
                    size_label: def.size_label(),
                    quality: def.quality.to_string(),
                    quality_context: def.quality_context.to_string(),
                    coverage: def.coverage.to_string(),
                    languages: def
                        .presentation_languages()
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    supports_recognition_hints: def.supports_recognition_hints,
                    installed,
                    active,
                    recommended_for_language: rec.is_some(),
                    recommendation_label: rec.map(|r| r.recommendation.label().to_string()),
                    can_delete,
                    preparing,
                    downloaded_bytes: if preparing {
                        prep_status.downloaded_bytes
                    } else {
                        0
                    },
                    total_bytes,
                    progress_fraction,
                    status_label,
                }
            })
            .collect();
        HexTranscriptionCatalogSnapshot {
            selected_language,
            languages,
            models,
            preparation_error: prep_status.error,
            deletion_error: self.transcription_deletion_error.clone(),
        }
    }

    fn opencode_catalog_snapshot(&self) -> HexOpenCodeCatalogSnapshot {
        match &self.opencode_catalog {
            OpenCodeCatalogState::Idle | OpenCodeCatalogState::Loading => {
                HexOpenCodeCatalogSnapshot {
                    state: "loading".to_string(),
                    ..Default::default()
                }
            }
            OpenCodeCatalogState::Missing => HexOpenCodeCatalogSnapshot {
                state: "missing".to_string(),
                error: Some(
                    "Install OpenCode v2 (https://v2.opencode.ai/) to use AI rewriting and Voice Action."
                        .to_string(),
                ),
                ..Default::default()
            },
            OpenCodeCatalogState::Failed(err) => HexOpenCodeCatalogSnapshot {
                state: "failed".to_string(),
                error: Some(err.clone()),
                ..Default::default()
            },
            OpenCodeCatalogState::Loaded(catalog) => HexOpenCodeCatalogSnapshot {
                state: "loaded".to_string(),
                error: None,
                default_key: catalog.default_key.clone(),
                default_name: catalog.default_name.clone(),
                default_variant: catalog.default_variant.clone(),
                models: catalog
                    .models
                    .iter()
                    .map(|choice| HexOpenCodeModelChoice {
                        key: choice.key.clone(),
                        name: choice.name.clone(),
                        provider: choice.provider.clone(),
                        variants: choice.variants.clone(),
                        recommended_variant: catalog
                            .recommended_variant_for(&choice.key)
                            .map(str::to_string),
                    })
                    .collect(),
            },
        }
    }

    fn commands_snapshot(&self) -> HexCommandsSnapshot {
        let model_status = match crate::recognition::command_model_status() {
            CommandModelStatus::Disabled => "disabled",
            CommandModelStatus::Loading => "loading",
            CommandModelStatus::Ready => "ready",
            CommandModelStatus::Failed => "failed",
        };
        let workspace = crate::app_paths::personal_commands_workspace()
            .unwrap_or_else(|_| PathBuf::from("~/.config/hex"));
        let config_file = workspace.join("hex.config.ts");
        let workspace_exists = config_file.is_file();
        let last_reload_error = self
            .personal_workspace_error
            .clone()
            .or_else(|| self.personal_commands_status.last_reload_error.clone());
        let current_context_label = match &self.current_context {
            (Some(app), Some(host)) => format!("{app} / {host}"),
            (Some(app), None) => app.clone(),
            _ => "Global".to_string(),
        };

        let mut catalog: Vec<HexCommandCatalogItem> = crate::config::voice_control()
            .catalog()
            .into_iter()
            .map(|info| {
                let scope = match &info.scope {
                    CommandScope::Sleeping | CommandScope::Global => "Global".to_string(),
                    CommandScope::Application(app) => app.clone(),
                    CommandScope::Browser(host) => host.clone(),
                };
                HexCommandCatalogItem {
                    id: info.id,
                    group: info.group.unwrap_or_else(|| scope.clone()),
                    phrase: info.phrase,
                    aliases: info.aliases,
                    description: info.description,
                    scope,
                }
            })
            .collect();

        for personal in &self.personal_commands_status.catalog {
            if let Some((phrase, aliases)) = personal.phrases.split_first() {
                let scope = match &personal.context {
                    StatusContext::Global => "Global".to_string(),
                    StatusContext::Application { application } => application.clone(),
                    StatusContext::BrowserHost { browser_host } => browser_host.clone(),
                };
                catalog.push(HexCommandCatalogItem {
                    id: personal.id.clone(),
                    group: personal.group.clone(),
                    phrase: phrase.clone(),
                    aliases: aliases.to_vec(),
                    description: personal.description.clone(),
                    scope,
                });
            }
        }

        let transformations = self
            .personal_commands_status
            .transformations
            .iter()
            .map(|t| HexTransformationCatalogItem {
                id: t.id.clone(),
                name: t.name.clone(),
                description: t.description.clone().unwrap_or_default(),
            })
            .collect();

        HexCommandsSnapshot {
            command_model_status: model_status.to_string(),
            workspace_path: workspace.display().to_string(),
            config_file_path: config_file.display().to_string(),
            workspace_exists,
            workspace_provisioning: self.personal_workspace_provisioning,
            last_reload_error,
            current_context_label,
            catalog,
            transformations,
        }
    }

    fn activity_snapshot(&self) -> HexActivitySnapshot {
        let events = self
            .recent_events
            .iter()
            .map(|ev| HexActivityEventSnapshot {
                kind: ev.kind().to_string(),
                timestamp_ms: ev.timestamp_ms(),
                summary: format!("{ev:?}"),
                raw_json: serde_json::to_string_pretty(ev).unwrap_or_default(),
            })
            .collect();
        HexActivitySnapshot {
            listener_state: self.activity.state_label().map(str::to_string),
            device: self.activity.device.clone(),
            error: self.activity.error.clone(),
            events,
        }
    }

    fn refresh_meetings_cache(&mut self) {
        if let Ok(list) = meeting::list() {
            self.meetings_cache = list;
        }
    }

    fn refresh_activity_cache(&mut self) -> (bool, bool, bool) {
        let prev_last_ts = self.recent_events.last().map(VoiceEvent::timestamp_ms);
        let prev_len = self.recent_events.len();
        let prev_context = self.current_context.clone();
        let prev_activity = self.activity.clone();
        let mut saw_completed_dictation = false;

        if let Some(reader) = &mut self.event_reader {
            self.activity.refresh(reader);
            self.recent_events = reader.recent(150);
            for ev in self.recent_events.iter().rev() {
                if let VoiceEvent::Context {
                    application,
                    browser_url,
                    ..
                } = ev
                {
                    self.current_context = (application.clone(), browser_url.clone());
                    break;
                }
            }
            if let Some(prev_ts) = prev_last_ts {
                for ev in self.recent_events.iter().rev() {
                    if ev.timestamp_ms() <= prev_ts {
                        break;
                    }
                    if matches!(
                        ev,
                        VoiceEvent::Dictation {
                            phase: crate::events::DictationPhase::Pasted
                                | crate::events::DictationPhase::Edited
                                | crate::events::DictationPhase::VoiceAction
                                | crate::events::DictationPhase::Logged,
                            ..
                        }
                    ) {
                        saw_completed_dictation = true;
                        break;
                    }
                }
            }
        }

        let new_last_ts = self.recent_events.last().map(VoiceEvent::timestamp_ms);
        let activity_changed = new_last_ts != prev_last_ts
            || self.recent_events.len() != prev_len
            || self.activity != prev_activity;
        let context_changed = self.current_context != prev_context;
        (activity_changed, context_changed, saw_completed_dictation)
    }

    fn status_menu_snapshot(&self) -> HexStatusMenuSnapshot {
        let prep_status: PreparationStatus = self.transcription_preparation.status();
        let entries = model_menu_entries(&self.settings, prep_status.model.is_some(), is_installed);
        let models = entries
            .into_iter()
            .map(|entry| HexMenuModelItem {
                model_id: entry.model.as_str().to_string(),
                title: entry.title,
                selected: entry.selected,
                enabled: entry.enabled,
            })
            .collect();
        let preparing_model_title = prep_status.model.map(|model| {
            format!(
                "Preparing {}…",
                crate::transcription_models::definition(model).name
            )
        });
        let meetings_enabled = self.developer_features_enabled || self.settings.meetings.enabled;
        let menu = meeting_menu(&self.meeting_state, meetings_enabled);
        let visible = meetings_enabled || menu.status.is_some();
        let (state_label, started_elapsed_ms, raw_title) = match &self.meeting_state {
            MeetingMenuState::Idle => ("idle".to_string(), 0, None),
            MeetingMenuState::Starting => ("starting".to_string(), 0, None),
            MeetingMenuState::Recording { started, title } => (
                "recording".to_string(),
                started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
                Some(title.clone()),
            ),
            MeetingMenuState::Transcribing => ("transcribing".to_string(), 0, None),
        };
        HexStatusMenuSnapshot {
            models,
            preparing_model_title,
            model_switch_error: prep_status.error,
            meeting: HexMeetingMenuSnapshot {
                state: state_label,
                visible,
                status_title: menu.status,
                button_title: menu.button_title,
                recording_icon: menu.recording,
                can_start: menu.can_start,
                can_stop: menu.can_stop,
                started_elapsed_ms,
                raw_title,
            },
            updates_managed: crate::sparkle::updates_managed(),
            managed_updates_label: crate::sparkle::MANAGED_UPDATES_LABEL.to_string(),
        }
    }

    fn bootstrap_snapshot(&self) -> HexBootstrapSnapshot {
        let (setup_ready, permissions) = self.permission_snapshot();
        let (ai_available, ai_status) = apple_intelligence_status_copy();
        let dictation_hotkey_keycaps = self.settings.dictation_hotkey.keycaps();
        let dictation_hotkey_side = standalone_modifier_side(&self.settings.dictation_hotkey)
            .map(modifier_side_str)
            .map(str::to_string);
        let edit_hotkey_keycaps = self.settings.edit_hotkey.keycaps();
        let edit_hotkey_side = standalone_modifier_side(&self.settings.edit_hotkey)
            .map(modifier_side_str)
            .map(str::to_string);
        let paste_last_hotkey_keycaps = self
            .settings
            .paste_last_hotkey
            .as_ref()
            .map(HotkeyBinding::keycaps)
            .unwrap_or_default();
        let paste_last_hotkey_side = self
            .settings
            .paste_last_hotkey
            .as_ref()
            .and_then(standalone_modifier_side)
            .map(modifier_side_str)
            .map(str::to_string);
        let resolved_meeting_output_folder = self
            .settings
            .meetings
            .resolved_output_folder()
            .display()
            .to_string();
        let meetings = self
            .meetings_cache
            .iter()
            .map(manifest_to_row_snapshot)
            .collect();

        HexBootstrapSnapshot {
            version: env!("CARGO_PKG_VERSION").to_string(),
            developer_features_enabled: self.developer_features_enabled,
            pid: std::process::id(),
            instance_lock_held: self.instance_lock_held,
            instance_conflict_error: self.instance_conflict_error.clone(),
            setup_ready,
            onboarding_completed: onboarding::completion_recorded(),
            permissions,
            status_menu: self.status_menu_snapshot(),
            hud_telemetry: self.hud.telemetry,
            meeting_panel: self.meeting_panel.clone(),
            active_model_id: self.settings.transcription.model.as_str().to_string(),
            active_language: self.settings.transcription.language.clone(),
            commands_enabled: self.settings.commands_enabled,
            voice_action_enabled: self.settings.voice_action.enabled,
            meetings_enabled: self.developer_features_enabled || self.settings.meetings.enabled,
            release_microphone_while_idle: self.settings.release_microphone_while_idle,
            show_dock_icon: self.settings.show_dock_icon,
            sound_volume: if self.settings.sound_effects {
                self.settings.sound_effect_volume
            } else {
                0.0
            },
            settings: self.settings.clone(),
            dictation_hotkey_keycaps,
            dictation_hotkey_side,
            edit_hotkey_keycaps,
            edit_hotkey_side,
            paste_last_hotkey_keycaps,
            paste_last_hotkey_side,
            available_microphones: self.available_microphones.clone(),
            resolved_meeting_output_folder,
            apple_intelligence_available: ai_available,
            apple_intelligence_status: ai_status,
            transcription_catalog: self.transcription_catalog_snapshot(),
            opencode_catalog: self.opencode_catalog_snapshot(),
            commands_snapshot: self.commands_snapshot(),
            meetings,
            activity: self.activity_snapshot(),
            installed_applications: self.installed_applications.clone(),
        }
    }

    fn poll_preparation(&mut self) {
        if let Some(rx) = &self.opencode_catalog_receiver
            && let Ok(state) = rx.try_recv()
        {
            self.opencode_catalog_receiver = None;
            self.opencode_catalog = state;
            self.last_opencode_missing_check = Instant::now();
            let snapshot = self.bootstrap_snapshot();
            self.enqueue_event(
                hex_event_tag_e::HEX_EVENT_OPENCODE_CATALOG_UPDATED,
                &snapshot,
            );
        }

        if let Some(rx) = &self.personal_workspace_receiver
            && let Ok(result) = rx.try_recv()
        {
            self.personal_workspace_receiver = None;
            self.personal_workspace_provisioning = false;
            match result {
                Ok(status) => {
                    self.personal_workspace_error = None;
                    self.personal_commands_status = status;
                }
                Err(error) => {
                    self.personal_workspace_error = Some(error);
                }
            }
            let snapshot = self.bootstrap_snapshot();
            self.enqueue_event(
                hex_event_tag_e::HEX_EVENT_PERSONAL_COMMANDS_UPDATED,
                &snapshot,
            );
        }

        if let Some(rx) = &self.app_discovery_receiver
            && let Ok(apps) = rx.try_recv()
        {
            self.app_discovery_receiver = None;
            self.installed_applications = apps;
            let snapshot = self.bootstrap_snapshot();
            self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
        }

        let Some(result) = self.transcription_preparation.poll() else {
            return;
        };
        match result {
            Ok(selection) => {
                self.transcription_picker_language = None;
                if self.persist_settings {
                    if let Err(error) = self.settings.save_transcription(selection) {
                        tracing::warn!(%error, "could not persist prepared transcription model");
                        self.transcription_preparation
                            .set_error(Some(error.to_string()));
                    } else {
                        self.transcription_preparation.set_error(None);
                    }
                } else {
                    self.settings.remember_transcription(selection);
                    self.settings.apply_runtime();
                    self.transcription_preparation.set_error(None);
                }
            }
            Err(error) => {
                tracing::warn!(%error, "transcription model preparation failed");
                self.transcription_preparation.set_error(Some(error));
            }
        }
        let snapshot = self.bootstrap_snapshot();
        self.enqueue_event(hex_event_tag_e::HEX_EVENT_MODEL_PREP_UPDATED, &snapshot);
        self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
    }

    fn apply_controller_event(&mut self, event: ControllerEvent) {
        match event {
            ControllerEvent::Offer(candidate) => {
                let opened = Instant::now();
                let title = if candidate.named {
                    candidate.title.clone()
                } else {
                    format!("{} call detected", candidate.source.label())
                };
                let seconds = OFFER_TIMEOUT.as_secs();
                self.pending_offer = Some((candidate.clone(), opened));
                self.ready_panel_deadline = None;
                self.meeting_panel = HexMeetingPanelSnapshot::Offer {
                    key: candidate.key,
                    title,
                    subtitle: format!("Record locally? Tell participants · {seconds}s"),
                    source_label: candidate.source.label().to_string(),
                    remaining_seconds: seconds,
                };
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
            }
            ControllerEvent::Starting => {
                self.pending_offer = None;
                self.ready_panel_deadline = None;
                self.meeting_state = MeetingMenuState::Starting;
                self.meeting_panel = HexMeetingPanelSnapshot::Hidden;
                self.refresh_meetings_cache();
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
            }
            ControllerEvent::RecordingStarted { title } => {
                self.pending_offer = None;
                self.ready_panel_deadline = None;
                self.meeting_state = MeetingMenuState::Recording {
                    started: Instant::now(),
                    title: title.clone(),
                };
                self.meeting_panel = HexMeetingPanelSnapshot::Recording {
                    title,
                    app_label: "Meeting".to_string(),
                    elapsed_text: "00:00".to_string(),
                };
                self.refresh_meetings_cache();
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
            }
            ControllerEvent::Retitled { title } => {
                if let MeetingMenuState::Recording {
                    title: active_title,
                    started,
                } = &mut self.meeting_state
                {
                    *active_title = title.clone();
                    let elapsed = meeting::format_duration(started.elapsed().as_millis() as u64);
                    if matches!(
                        self.meeting_panel,
                        HexMeetingPanelSnapshot::Recording { .. }
                    ) {
                        self.meeting_panel = HexMeetingPanelSnapshot::Recording {
                            title,
                            app_label: "Meeting".to_string(),
                            elapsed_text: elapsed,
                        };
                        let panel = self.meeting_panel.clone();
                        self.enqueue_event(
                            hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED,
                            &panel,
                        );
                    }
                }
                self.refresh_meetings_cache();
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
            }
            ControllerEvent::CallEnded { app } => {
                self.pending_offer = None;
                self.ready_panel_deadline = None;
                self.meeting_panel = HexMeetingPanelSnapshot::CallEnded {
                    title: format!("{app} left the call"),
                    subtitle: "HEX is still recording.".to_string(),
                    app_label: app,
                };
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
            }
            ControllerEvent::Transcribing => {
                self.pending_offer = None;
                self.ready_panel_deadline = None;
                self.meeting_state = MeetingMenuState::Transcribing;
                self.meeting_panel = HexMeetingPanelSnapshot::Hidden;
                self.refresh_meetings_cache();
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
            }
            ControllerEvent::Finished(manifest) => {
                self.pending_offer = None;
                self.meeting_state = MeetingMenuState::Idle;
                self.ready_panel_deadline = Some(Instant::now() + READY_TIMEOUT);
                self.meeting_panel = Self::ready_panel_from_manifest(&manifest);
                self.refresh_meetings_cache();
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
            }
            ControllerEvent::NotesStarted => {
                if let HexMeetingPanelSnapshot::Ready {
                    meeting_id,
                    title,
                    export_path,
                    ..
                } = &self.meeting_panel
                {
                    self.meeting_panel = HexMeetingPanelSnapshot::Ready {
                        meeting_id: meeting_id.clone(),
                        title: title.clone(),
                        subtitle: "Summarizing with Apple Intelligence…".to_string(),
                        export_path: export_path.clone(),
                        summarizing: true,
                    };
                    let panel = self.meeting_panel.clone();
                    self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                }
                self.refresh_meetings_cache();
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
            }
            ControllerEvent::NotesFinished(manifest) => {
                if let HexMeetingPanelSnapshot::Ready { meeting_id, .. } = &self.meeting_panel
                    && *meeting_id == manifest.id
                {
                    self.meeting_panel = Self::ready_panel_from_manifest(&manifest);
                    let panel = self.meeting_panel.clone();
                    self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                }
                self.refresh_meetings_cache();
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
            }
            ControllerEvent::Failed { title, error } => {
                tracing::error!(%title, %error, "meeting recording failed");
                self.pending_offer = None;
                self.ready_panel_deadline = None;
                self.meeting_state = MeetingMenuState::Idle;
                self.meeting_panel = HexMeetingPanelSnapshot::Hidden;
                self.refresh_meetings_cache();
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
            }
        }
    }

    fn ready_panel_from_manifest(manifest: &meeting::MeetingManifest) -> HexMeetingPanelSnapshot {
        let summarizing = meeting::notes_in_progress(&manifest.id);
        let summary = manifest
            .notes
            .as_ref()
            .map(|notes| notes.summary.trim().to_string())
            .filter(|s| !s.is_empty());
        let subtitle = match (&manifest.export_path, &manifest.export_error) {
            _ if summarizing => "Summarizing with Apple Intelligence…".to_string(),
            _ if summary.is_some() => summary.unwrap_or_default(),
            (Some(path), _) => path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
            (None, Some(error)) => format!("Markdown not saved: {error}"),
            (None, None) => "Saved in Meetings".to_string(),
        };
        let display = meeting::display_title(manifest);
        let title = if display == manifest.title && manifest.notes.is_none() {
            "Meeting transcript ready".to_string()
        } else {
            format!("Ready · {display}")
        };
        HexMeetingPanelSnapshot::Ready {
            meeting_id: manifest.id.clone(),
            title,
            subtitle,
            export_path: manifest
                .export_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
            summarizing,
        }
    }

    fn maintain_timers(&mut self) {
        if let Some((candidate, opened)) = self.pending_offer.clone() {
            let elapsed = opened.elapsed();
            if elapsed >= OFFER_TIMEOUT {
                self.pending_offer = None;
                if matches!(self.meeting_panel, HexMeetingPanelSnapshot::Offer { .. }) {
                    self.meeting_panel = HexMeetingPanelSnapshot::Hidden;
                    let panel = self.meeting_panel.clone();
                    self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                }
            } else {
                let remaining = OFFER_TIMEOUT.saturating_sub(elapsed).as_secs() + 1;
                if let HexMeetingPanelSnapshot::Offer {
                    remaining_seconds, ..
                } = &self.meeting_panel
                    && *remaining_seconds != remaining
                {
                    let title = if candidate.named {
                        candidate.title.clone()
                    } else {
                        format!("{} call detected", candidate.source.label())
                    };
                    self.meeting_panel = HexMeetingPanelSnapshot::Offer {
                        key: candidate.key,
                        title,
                        subtitle: format!("Record locally? Tell participants · {remaining}s"),
                        source_label: candidate.source.label().to_string(),
                        remaining_seconds: remaining,
                    };
                    let panel = self.meeting_panel.clone();
                    self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                }
            }
        }

        if let Some(deadline) = self.ready_panel_deadline
            && Instant::now() >= deadline
        {
            self.ready_panel_deadline = None;
            if matches!(self.meeting_panel, HexMeetingPanelSnapshot::Ready { .. }) {
                self.meeting_panel = HexMeetingPanelSnapshot::Hidden;
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
            }
        }

        if let MeetingMenuState::Recording { started, title } = &self.meeting_state {
            let elapsed = meeting::format_duration(started.elapsed().as_millis() as u64);
            let mut elapsed_changed = false;
            if let HexMeetingPanelSnapshot::Recording {
                elapsed_text,
                app_label,
                ..
            } = &self.meeting_panel
                && *elapsed_text != elapsed
            {
                elapsed_changed = true;
                self.meeting_panel = HexMeetingPanelSnapshot::Recording {
                    title: title.clone(),
                    app_label: app_label.clone(),
                    elapsed_text: elapsed,
                };
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
            }
            let mut transcript_changed = false;
            if let Some(active_manifest) = self.meetings_cache.first()
                && let Ok(support_dir) = crate::app_paths::support_dir()
            {
                let dir = support_dir.join("meetings").join(&active_manifest.id);
                let live_len = std::fs::metadata(dir.join("transcript.live.ndjson"))
                    .map(|m| m.len())
                    .unwrap_or(0);
                let final_len = std::fs::metadata(dir.join("transcript.ndjson"))
                    .map(|m| m.len())
                    .unwrap_or(0);
                let combined_len = live_len.saturating_add(final_len);
                if combined_len != self.active_meeting_transcript_len {
                    self.active_meeting_transcript_len = combined_len;
                    transcript_changed = true;
                }
            }
            if transcript_changed {
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(
                    hex_event_tag_e::HEX_EVENT_MEETING_TRANSCRIPT_DELTA,
                    &snapshot,
                );
            } else if elapsed_changed {
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
            }
        }

        if self.start_workers {
            let (activity_changed, context_changed, saw_completed_dictation) =
                self.refresh_activity_cache();
            let model_status = crate::recognition::command_model_status();
            let model_status_changed = model_status != self.last_command_model_status;
            if model_status_changed {
                self.last_command_model_status = model_status;
            }
            if saw_completed_dictation {
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_HISTORY_UPDATED, &snapshot);
            } else if context_changed || model_status_changed {
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(
                    hex_event_tag_e::HEX_EVENT_PERSONAL_COMMANDS_UPDATED,
                    &snapshot,
                );
            } else if activity_changed && self.developer_features_enabled {
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_ACTIVITY_APPENDED, &snapshot);
            }
            if matches!(self.opencode_catalog, OpenCodeCatalogState::Missing)
                && self.opencode_catalog_receiver.is_none()
                && self.last_opencode_missing_check.elapsed() >= Duration::from_secs(5)
            {
                self.last_opencode_missing_check = Instant::now();
                if crate::dictation_processor::opencode_installed() {
                    self.opencode_catalog = OpenCodeCatalogState::Loading;
                    self.opencode_catalog_receiver =
                        Some(start_opencode_catalog_worker(self.callback_sink.clone()));
                }
            }
        }
    }

    fn dispatch(&mut self, command: HexEngineCommand) -> Result<(), String> {
        match command {
            HexEngineCommand::RefreshSnapshot => {
                if self.persist_settings
                    && let Ok(loaded) = AppSettings::load()
                {
                    self.settings = loaded;
                }
                if let Ok(mut status) = crate::personal_commands::load_status() {
                    crate::personal_commands::include_builtin_transformations(&mut status);
                    self.personal_commands_status = status;
                }
                self.refresh_meetings_cache();
                let _ = self.refresh_activity_cache();
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SelectInstalledModel { model_id } => {
                let model = TranscriptionModelId::from_str(&model_id).map_err(|e| e.to_string())?;
                let definition = crate::transcription_models::definition(model);
                let selection = self.settings.transcription_for_model(model);
                if !is_installed(definition, &selection.language) {
                    let msg = format!(
                        "{} is not downloaded. Open Settings to download it first.",
                        definition.name
                    );
                    self.transcription_preparation.set_error(Some(msg.clone()));
                    let snapshot = self.bootstrap_snapshot();
                    self.enqueue_event(hex_event_tag_e::HEX_EVENT_MODEL_PREP_UPDATED, &snapshot);
                    return Err(msg);
                }
                self.transcription_preparation.start(selection, true);
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MODEL_PREP_UPDATED, &snapshot);
                Ok(())
            }
            HexEngineCommand::ChooseTranscriptionModel {
                model_id,
                language,
                recognition_hints,
            } => {
                let model = TranscriptionModelId::from_str(&model_id).map_err(|e| e.to_string())?;
                let definition = crate::transcription_models::definition(model);
                let hints = if definition.supports_recognition_hints {
                    recognition_hints
                        .unwrap_or_else(|| self.settings.transcription.recognition_hints.clone())
                } else {
                    String::new()
                };
                let selection = TranscriptionSelection {
                    model,
                    language,
                    recognition_hints: hints,
                };
                crate::transcription_models::validate(&selection).map_err(|e| e.to_string())?;
                self.transcription_deletion_error = None;
                self.transcription_preparation.set_error(None);
                self.transcription_picker_language = Some(selection.language.clone());
                if is_installed(definition, &selection.language)
                    && self.settings.transcription.model == model
                    && self.settings.transcription.language == selection.language
                {
                    self.transcription_picker_language = None;
                    let mut candidate = self.settings.clone();
                    candidate.remember_transcription(selection);
                    self.commit_settings(candidate)?;
                    let snapshot = self.bootstrap_snapshot();
                    self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                    return Ok(());
                }
                self.transcription_preparation.start(selection, false);
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MODEL_PREP_UPDATED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetTranscriptionLanguage { language } => {
                if !LANGUAGES.iter().any(|(code, _)| *code == language) {
                    return Err(format!("unsupported transcription language: {language}"));
                }
                self.transcription_picker_language = Some(language.clone());
                let current_def =
                    crate::transcription_models::definition(self.settings.transcription.model);
                if current_def.supports_language(&language)
                    && (!self.persist_settings || is_installed(current_def, &language))
                {
                    let mut candidate = self.settings.clone();
                    candidate.transcription.language = language;
                    self.commit_settings(candidate)?;
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetRecognitionHints { hints } => {
                let mut candidate = self.settings.clone();
                candidate.transcription.recognition_hints = hints;
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::DeleteTranscriptionModel { model_id } => {
                let model = TranscriptionModelId::from_str(&model_id).map_err(|e| e.to_string())?;
                if self.settings.transcription.model == model {
                    return Err("Cannot delete the currently active transcription model.".into());
                }
                if self.transcription_preparation.status().model == Some(model) {
                    return Err("Cannot delete a model while it is being prepared.".into());
                }
                let definition = crate::transcription_models::definition(model);
                if let Err(error) = crate::transcription_models::remove_installed(definition) {
                    let msg = error.to_string();
                    self.transcription_deletion_error = Some(msg.clone());
                    let snapshot = self.bootstrap_snapshot();
                    self.enqueue_event(hex_event_tag_e::HEX_EVENT_MODEL_PREP_UPDATED, &snapshot);
                    return Err(msg);
                }
                self.transcription_deletion_error = None;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MODEL_PREP_UPDATED, &snapshot);
                Ok(())
            }
            HexEngineCommand::CancelModelPreparation => {
                self.transcription_preparation.cancel();
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MODEL_PREP_UPDATED, &snapshot);
                Ok(())
            }
            HexEngineCommand::PasteLastDictation => {
                if let Some(controls) = &self.recognition_controls {
                    let _ = controls.try_send(RecognitionControl::PasteLast);
                }
                Ok(())
            }
            HexEngineCommand::PasteMeetingTurns => {
                Err("Pasting meeting turns is only available through its keyboard shortcut.".into())
            }
            HexEngineCommand::StartMeeting => {
                if let Some(requests) = &self.meeting_requests {
                    requests
                        .try_send(MeetingRequest::Start)
                        .map_err(|e| format!("could not start meeting: {e}"))?;
                } else {
                    self.apply_controller_event(ControllerEvent::RecordingStarted {
                        title: "Meeting recording".to_string(),
                    });
                }
                Ok(())
            }
            HexEngineCommand::StopMeeting => {
                if let Some(commands) = &self.meeting_commands {
                    commands
                        .try_send(ControllerCommand::Stop)
                        .map_err(|e| format!("could not stop meeting: {e}"))?;
                } else {
                    self.apply_controller_event(ControllerEvent::Transcribing);
                }
                Ok(())
            }
            HexEngineCommand::AcceptMeetingOffer => {
                let candidate = self.pending_offer.take().map(|(c, _)| c);
                self.meeting_panel = HexMeetingPanelSnapshot::Hidden;
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                if let (Some(candidate), Some(commands)) = (candidate, &self.meeting_commands) {
                    let cmd = if candidate.key == "preview" {
                        ControllerCommand::Dismiss
                    } else {
                        ControllerCommand::Record(candidate)
                    };
                    let _ = commands.try_send(cmd);
                }
                Ok(())
            }
            HexEngineCommand::DismissMeetingOffer => {
                self.pending_offer = None;
                self.meeting_panel = HexMeetingPanelSnapshot::Hidden;
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                if let Some(commands) = &self.meeting_commands {
                    let _ = commands.try_send(ControllerCommand::Dismiss);
                }
                Ok(())
            }
            HexEngineCommand::DismissCallEndedPrompt => {
                if matches!(
                    self.meeting_panel,
                    HexMeetingPanelSnapshot::CallEnded { .. }
                ) {
                    self.meeting_panel = HexMeetingPanelSnapshot::Hidden;
                    let panel = self.meeting_panel.clone();
                    self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                }
                Ok(())
            }
            HexEngineCommand::RevealMeetingTranscript { path } => {
                meeting::reveal(Path::new(&path));
                self.ready_panel_deadline = None;
                if matches!(self.meeting_panel, HexMeetingPanelSnapshot::Ready { .. }) {
                    self.meeting_panel = HexMeetingPanelSnapshot::Hidden;
                    let panel = self.meeting_panel.clone();
                    self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                }
                Ok(())
            }
            HexEngineCommand::RenameMeeting { id, title } => {
                let trimmed = title.split_whitespace().collect::<Vec<_>>().join(" ");
                if trimmed.is_empty() {
                    return Err("The meeting title cannot be empty.".into());
                }
                if self.persist_settings {
                    meeting::rename(&id, &trimmed).map_err(|e| format!("{e:#}"))?;
                    self.refresh_meetings_cache();
                } else if let Some(item) = self.meetings_cache.iter_mut().find(|m| m.id == id) {
                    item.title = trimmed;
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::DeleteMeeting { id } => {
                if self.persist_settings {
                    meeting::trash(&id).map_err(|e| format!("{e:#}"))?;
                    self.refresh_meetings_cache();
                } else {
                    self.meetings_cache.retain(|m| m.id != id);
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SummarizeMeeting { id } => {
                if let Some(requests) = &self.meeting_requests {
                    let _ = requests.try_send(MeetingRequest::Summarize(id));
                } else {
                    meeting::spawn_summary(id, move |_result| {});
                }
                self.refresh_meetings_cache();
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::RefreshMeetings => {
                self.refresh_meetings_cache();
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetSoundVolume { volume } => {
                let clamped = volume.clamp(0.0, 1.0);
                let mut candidate = self.settings.clone();
                candidate.sound_effects = clamped > 0.0;
                if clamped > 0.0 {
                    candidate.sound_effect_volume = clamped;
                }
                self.commit_settings(candidate)?;
                if self.persist_settings && clamped > 0.0 {
                    crate::feedback::play(crate::feedback::Tone::DictationStart);
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetShowDockIcon { visible } => {
                let mut candidate = self.settings.clone();
                candidate.show_dock_icon = visible;
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetMeetingsEnabled { enabled } => {
                let mut candidate = self.settings.clone();
                candidate.meetings.enabled = enabled;
                self.commit_settings(candidate)?;
                if enabled {
                    self.refresh_meetings_cache();
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetMeetingNotesEnabled { enabled } => {
                let mut candidate = self.settings.clone();
                candidate.meetings.summarize_with_apple_intelligence = enabled;
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetMeetingOutputFolder { path } => {
                let mut candidate = self.settings.clone();
                candidate.meetings.output_folder = path
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from);
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SaveSettings { settings } => {
                self.commit_settings(settings)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::RequestPermission { kind } => {
                match kind.as_str() {
                    "microphone" => {
                        let status = onboarding::status(&self.settings.transcription);
                        if status.microphone == PermissionState::NeedsRequest {
                            onboarding::request_microphone();
                        } else {
                            onboarding::open_permission_settings("microphone");
                        }
                    }
                    "input_monitoring" | "input" => {
                        crate::permission_guide::show_input_monitoring();
                    }
                    "accessibility" => {
                        crate::permission_guide::show_accessibility();
                    }
                    "screen_recording" => {
                        if !meeting::request_screen_recording() {
                            meeting::open_screen_recording_settings();
                        }
                    }
                    _ => onboarding::open_permission_settings(&kind),
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_READINESS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::OpenPermissionSettings { kind } => {
                match kind.as_str() {
                    "screen_recording" => meeting::open_screen_recording_settings(),
                    "input_monitoring" => onboarding::open_permission_settings("input"),
                    other => onboarding::open_permission_settings(other),
                }
                Ok(())
            }
            HexEngineCommand::RecordOnboardingCompletion => {
                if self.persist_settings {
                    onboarding::record_completion().map_err(|e| e.to_string())?;
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_READINESS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetHotkeyCaptureActive { active } => {
                if self.persist_settings {
                    crate::app_settings::set_hotkey_capture_active(active);
                }
                Ok(())
            }
            HexEngineCommand::SetHotkeyBinding { target, binding } => {
                let mut candidate = self.settings.clone();
                if let Some(ref b) = binding {
                    if b.is_empty() {
                        return Err("Press a shortcut".into());
                    }
                    if hotkey_binding_conflicts_for_target(&candidate, &target, b) {
                        return Err("Shortcut is already in use.".into());
                    }
                }
                match target.as_str() {
                    "dictation" => {
                        let b = binding.ok_or("Dictation shortcut cannot be disabled.")?;
                        candidate.dictation_hotkey = b;
                    }
                    "edit" | "voice_action" => {
                        let b = binding.ok_or("Voice Action shortcut cannot be empty.")?;
                        candidate.edit_hotkey = b;
                    }
                    "paste_last" => {
                        candidate.paste_last_hotkey = binding;
                    }
                    other => return Err(format!("Unknown hotkey target: {other}")),
                }
                self.commit_settings(candidate)?;
                if self.persist_settings {
                    crate::app_settings::set_hotkey_capture_active(false);
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::CaptureHotkey {
                target,
                key,
                key_code,
                modifier_flags,
            } => {
                let raw_modifiers = crate::app_settings::modifiers_from_flags(modifier_flags);
                let binding = if let Some(key_str) = key.filter(|s| !s.trim().is_empty()) {
                    let modifiers = raw_modifiers.without_side_constraints();
                    let hk_key = resolve_hotkey_key(&key_str, key_code)?;
                    if modifiers.is_empty() && !is_function_key_label(&hk_key.label) {
                        return Err("Add a modifier".into());
                    }
                    HotkeyBinding {
                        modifiers,
                        key: Some(hk_key),
                    }
                } else {
                    let mut sides = [
                        raw_modifiers.control,
                        raw_modifiers.option,
                        raw_modifiers.shift,
                        raw_modifiers.command,
                    ]
                    .into_iter()
                    .flatten();
                    let normalized = if let Some(first) = sides.next() {
                        if sides.all(|s| s == first) {
                            raw_modifiers
                        } else {
                            raw_modifiers.without_side_constraints()
                        }
                    } else {
                        raw_modifiers
                    };
                    if normalized.is_empty() {
                        return Err("Press a shortcut".into());
                    }
                    HotkeyBinding {
                        modifiers: normalized,
                        key: None,
                    }
                };
                if hotkey_binding_conflicts_for_target(&self.settings, &target, &binding) {
                    return Err("Already in use".into());
                }
                let mut candidate = self.settings.clone();
                match target.as_str() {
                    "dictation" => candidate.dictation_hotkey = binding,
                    "edit" | "voice_action" => candidate.edit_hotkey = binding,
                    "paste_last" => candidate.paste_last_hotkey = Some(binding),
                    other => return Err(format!("Unknown hotkey target: {other}")),
                }
                self.commit_settings(candidate)?;
                if self.persist_settings {
                    crate::app_settings::set_hotkey_capture_active(false);
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetHotkeySide { target, side } => {
                let parsed_side =
                    parse_modifier_side(&side).ok_or_else(|| format!("Invalid side: {side}"))?;
                let mut candidate = self.settings.clone();
                let mut updated = match target.as_str() {
                    "dictation" => candidate.dictation_hotkey.clone(),
                    "edit" | "voice_action" => candidate.edit_hotkey.clone(),
                    "paste_last" => candidate
                        .paste_last_hotkey
                        .clone()
                        .ok_or("Paste Last shortcut is not set.")?,
                    other => return Err(format!("Unknown hotkey target: {other}")),
                };
                if standalone_modifier_side(&updated).is_none() {
                    return Err("Shortcut is not a standalone modifier.".into());
                }
                set_standalone_modifier_side(&mut updated, parsed_side);
                if hotkey_binding_conflicts_for_target(&candidate, &target, &updated) {
                    return Err("Already in use".into());
                }
                match target.as_str() {
                    "dictation" => candidate.dictation_hotkey = updated,
                    "edit" | "voice_action" => candidate.edit_hotkey = updated,
                    "paste_last" => candidate.paste_last_hotkey = Some(updated),
                    _ => unreachable!(),
                }
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetDoubleTapLock { enabled } => {
                let mut candidate = self.settings.clone();
                candidate.double_tap_lock = enabled;
                if !enabled {
                    candidate.double_tap_only = false;
                }
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetDoubleTapOnly { enabled } => {
                let mut candidate = self.settings.clone();
                if enabled && candidate.dictation_hotkey.key.is_none() {
                    return Err(
                        "Double-tap only requires a key shortcut (for example Control-Space)."
                            .into(),
                    );
                }
                if enabled {
                    candidate.double_tap_lock = true;
                }
                candidate.double_tap_only = enabled;
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetIncrementalTranscription { enabled } => {
                let mut candidate = self.settings.clone();
                candidate.incremental_transcription = enabled;
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetRecordingAudioBehavior { behavior } => {
                let mut candidate = self.settings.clone();
                candidate.recording_audio_behavior = behavior;
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetMicrophone { device } => {
                let mut candidate = self.settings.clone();
                candidate.microphone = device.filter(|d| !d.trim().is_empty());
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::RefreshMicrophones => {
                if let Ok(devices) = crate::audio::input_device_names() {
                    self.available_microphones = devices;
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetReleaseMicrophoneWhileIdle {
                enabled,
                confirm_disable_commands,
            } => {
                let mut candidate = self.settings.clone();
                if enabled && candidate.commands_enabled {
                    if confirm_disable_commands {
                        candidate.disable_commands_and_release_microphone();
                    } else {
                        return Err(
                            "disable voice commands before enabling idle microphone release".into(),
                        );
                    }
                } else {
                    candidate
                        .set_release_microphone_while_idle(enabled)
                        .map_err(|e| e.to_string())?;
                }
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetCommandsEnabled {
                enabled,
                confirm_keep_microphone_ready,
            } => {
                let mut candidate = self.settings.clone();
                if enabled && candidate.release_microphone_while_idle {
                    if confirm_keep_microphone_ready {
                        candidate.keep_microphone_ready_and_enable_commands();
                    } else {
                        return Err(
                            "disable idle microphone release before enabling voice commands".into(),
                        );
                    }
                } else {
                    candidate
                        .set_commands_enabled(enabled)
                        .map_err(|e| e.to_string())?;
                }
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::RetryCommandsModel => {
                if let Some(controls) = &self.recognition_controls {
                    let _ = controls.try_send(RecognitionControl::RetryCommands);
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::ProvisionPersonalCommandsWorkspace => {
                if self.personal_workspace_receiver.is_none() {
                    self.personal_workspace_provisioning = true;
                    self.personal_workspace_error = None;
                    self.personal_workspace_receiver =
                        Some(start_personal_workspace_worker(self.callback_sink.clone()));
                    let snapshot = self.bootstrap_snapshot();
                    self.enqueue_event(
                        hex_event_tag_e::HEX_EVENT_PERSONAL_COMMANDS_UPDATED,
                        &snapshot,
                    );
                }
                Ok(())
            }
            HexEngineCommand::OpenPersonalCommandsConfig => {
                let workspace =
                    crate::app_paths::personal_commands_workspace().map_err(|e| e.to_string())?;
                let config = workspace.join("hex.config.ts");
                let _ = std::process::Command::new("/usr/bin/open")
                    .arg(&config)
                    .spawn();
                Ok(())
            }
            HexEngineCommand::CopyPersonalCommandsAgentPrompt => {
                let workspace =
                    crate::app_paths::personal_commands_workspace().map_err(|e| e.to_string())?;
                let config = workspace.join("hex.config.ts");
                let prompt = format!(
                    "Help me configure my HEX custom voice commands.\n\n\
                     Workspace: {}\n\
                     Config: {}\n\n\
                     Read AGENTS.md and .agents/skills/personal-commands/SKILL.md in the workspace \
                     before editing. Ask me what commands or dictation phrases I want, update \
                     hex.config.ts, and run `bun run check` from the workspace when finished. You \
                     may add npm packages when a command needs them.",
                    workspace.display(),
                    config.display()
                );
                arboard::Clipboard::new()
                    .and_then(|mut cb| cb.set_text(prompt))
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
            HexEngineCommand::SetVoiceActionEnabled { enabled } => {
                let mut candidate = self.settings.clone();
                if enabled
                    && hotkey_binding_conflicts_for_target(
                        &candidate,
                        "edit",
                        &candidate.edit_hotkey,
                    )
                {
                    return Err(
                        "The Voice Action shortcut is already in use. Change it before enabling Voice Action."
                            .into(),
                    );
                }
                candidate.voice_action.enabled = enabled;
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetVoiceActionModel {
                model,
                variant,
                deadline_seconds,
            } => {
                let mut candidate = self.settings.clone();
                candidate.voice_action.model = model.filter(|m| !m.trim().is_empty());
                candidate.voice_action.variant = variant.filter(|v| !v.trim().is_empty());
                if let Some(secs) = deadline_seconds {
                    candidate.voice_action.deadline_seconds = secs.max(1);
                }
                self.commit_settings(candidate)?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::ReloadOpenCodeCatalog => {
                if self.opencode_catalog_receiver.is_none() {
                    self.opencode_catalog = OpenCodeCatalogState::Loading;
                    self.opencode_catalog_receiver =
                        Some(start_opencode_catalog_worker(self.callback_sink.clone()));
                    let snapshot = self.bootstrap_snapshot();
                    self.enqueue_event(
                        hex_event_tag_e::HEX_EVENT_OPENCODE_CATALOG_UPDATED,
                        &snapshot,
                    );
                }
                Ok(())
            }
            HexEngineCommand::OpenOpenCodeSetupDocs => {
                let _ = std::process::Command::new("/usr/bin/open")
                    .arg("https://v2.opencode.ai/")
                    .spawn();
                Ok(())
            }
            HexEngineCommand::DiscoverInstalledApplications => {
                if self.app_discovery_receiver.is_none() {
                    self.app_discovery_receiver =
                        Some(start_app_discovery_worker(self.callback_sink.clone()));
                }
                Ok(())
            }
            HexEngineCommand::SetHistoryRetention { retention } => {
                let mut candidate = self.settings.clone();
                candidate.history_retention = retention;
                self.commit_settings(candidate)?;
                if let Some(history) = &self.history {
                    let _ = history.set_retention(retention);
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_HISTORY_UPDATED, &snapshot);
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::DeleteHistoryEntry { id } => {
                if let Some(history) = &self.history {
                    history.delete(id).map_err(|e| e.to_string())?;
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_HISTORY_UPDATED, &snapshot);
                Ok(())
            }
            HexEngineCommand::ClearHistory => {
                if let Some(history) = &self.history {
                    history.clear().map_err(|e| e.to_string())?;
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_HISTORY_UPDATED, &snapshot);
                Ok(())
            }
            HexEngineCommand::RefreshActivity => {
                let _ = self.refresh_activity_cache();
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_ACTIVITY_APPENDED, &snapshot);
                Ok(())
            }
            HexEngineCommand::ConfigureHudTuning {
                style,
                line_count,
                curvature,
                speed,
                sharpness,
                glow,
                depth,
                light_angle,
                outline,
            } => {
                self.indicator_sender
                    .send(DictationIndicatorEvent::Configure(HudTuning {
                        style,
                        line_count,
                        curvature,
                        speed,
                        sharpness,
                        glow,
                        depth,
                        light_angle,
                        outline,
                    }));
                Ok(())
            }
            HexEngineCommand::SimulateHudState { state, rms, peak } => {
                match state.as_str() {
                    "reset" | "hidden" => {
                        self.hud.apply(DictationIndicatorEvent::Reset);
                    }
                    "listening" | "recording" => {
                        self.hud.apply(DictationIndicatorEvent::Started);
                        if rms.is_some() || peak.is_some() {
                            self.hud.apply(DictationIndicatorEvent::Meter {
                                average: rms.unwrap_or(0.08),
                                peak: peak.unwrap_or(0.42),
                            });
                        }
                    }
                    "locked" => {
                        self.hud.apply(DictationIndicatorEvent::Started);
                        self.hud.telemetry.phase = hex_hud_phase_e::HEX_HUD_PHASE_LOCKED;
                        self.hud.sync_atomic();
                        if rms.is_some() || peak.is_some() {
                            self.hud.apply(DictationIndicatorEvent::Meter {
                                average: rms.unwrap_or(0.08),
                                peak: peak.unwrap_or(0.42),
                            });
                        }
                    }
                    "voice_action" | "editing" => {
                        self.hud.apply(DictationIndicatorEvent::EditingStarted);
                        if rms.is_some() || peak.is_some() {
                            self.hud.apply(DictationIndicatorEvent::Meter {
                                average: rms.unwrap_or(0.08),
                                peak: peak.unwrap_or(0.42),
                            });
                        }
                    }
                    "transcribing" => {
                        self.hud
                            .apply(DictationIndicatorEvent::Submitted { job_id: 0 });
                        self.hud
                            .apply(DictationIndicatorEvent::Transcribing { job_id: 0 });
                    }
                    "processing" => {
                        self.hud
                            .apply(DictationIndicatorEvent::Submitted { job_id: 0 });
                        self.hud
                            .apply(DictationIndicatorEvent::Processing { job_id: 0 });
                    }
                    "completed" => {
                        self.hud
                            .apply(DictationIndicatorEvent::Submitted { job_id: 0 });
                        self.hud
                            .apply(DictationIndicatorEvent::JobCompleted { job_id: 0 });
                    }
                    "cancelled" => {
                        self.hud.apply(DictationIndicatorEvent::Started);
                        self.hud.apply(DictationIndicatorEvent::Cancelled);
                    }
                    "error" | "failed" => {
                        self.hud.apply(DictationIndicatorEvent::Failed);
                    }
                    other => return Err(format!("unknown simulated HUD state: {other}")),
                }
                let telemetry = self.hud.telemetry;
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_HUD_CHANGED, &telemetry);
                Ok(())
            }
            HexEngineCommand::SimulateMeetingPanel { panel } => {
                self.pending_offer = None;
                self.ready_panel_deadline = None;
                self.meeting_panel = panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                Ok(())
            }
            HexEngineCommand::CompleteOnboarding => {
                if self.persist_settings {
                    onboarding::record_completion().map_err(|e| e.to_string())?;
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::RetryInstanceLock => self.try_acquire_instance_lock(),
            HexEngineCommand::QuitOtherHexInstancesAndRetry => {
                self.quit_other_instances_and_retry()
            }
            HexEngineCommand::RequestCheckForUpdates => {
                let empty = serde_json::json!({});
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_REQUEST_CHECK_FOR_UPDATES, &empty);
                Ok(())
            }
            HexEngineCommand::RequestQuit => {
                self.transcription_preparation.cancel();
                crate::SHUTDOWN.store(true, Ordering::Relaxed);
                let empty = serde_json::json!({});
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_QUIT_REQUESTED, &empty);
                Ok(())
            }
        }
    }
}

pub struct hex_engine_s {
    core: Arc<Mutex<EngineCore>>,
    atomic_hud: Arc<AtomicHudTelemetry>,
    callback_sink: Arc<CallbackSink>,
    shutdown: Arc<AtomicBool>,
    coordinator_worker: Option<JoinHandle<()>>,
    preview_worker: Option<JoinHandle<()>>,
}

#[allow(non_camel_case_types)]
pub type hex_engine_t = *mut hex_engine_s;

fn allocate_json_slice<T: Serialize>(value: &T) -> hex_slice_s {
    let Ok(bytes) = serde_json::to_vec(value) else {
        return hex_slice_s::default();
    };
    allocate_bytes_slice(bytes)
}

fn allocate_bytes_slice(bytes: Vec<u8>) -> hex_slice_s {
    if bytes.is_empty() {
        return hex_slice_s::default();
    }
    let boxed = bytes.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed) as *const c_char;
    hex_slice_s { ptr, len }
}

fn slice_to_str<'a>(slice: hex_slice_s) -> Result<&'a str, String> {
    if slice.ptr.is_null() || slice.len == 0 {
        return Ok("");
    }
    let bytes = unsafe { slice::from_raw_parts(slice.ptr.cast::<u8>(), slice.len) };
    std::str::from_utf8(bytes).map_err(|e| format!("invalid UTF-8 slice: {e}"))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_keyboard_initialize_layout_on_main_thread() -> bool {
    if objc2::MainThreadMarker::new().is_none() {
        tracing::error!(
            "hex_keyboard_initialize_layout_on_main_thread requires the OS main thread"
        );
        return false;
    }
    match crate::keyboard::initialize_layout() {
        Ok(()) => true,
        Err(error) => {
            tracing::error!(%error, "failed to initialize TIS keyboard layout on main thread");
            false
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_cli_maybe_run_from_process_args() -> i32 {
    let raw_args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if !crate::should_run_cli_from_args(&raw_args) {
        return -1;
    }
    let filtered: Vec<std::ffi::OsString> = raw_args
        .into_iter()
        .filter(|arg| arg != "--legacy-gpui")
        .collect();
    match crate::run_cli_from_args(filtered) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error:?}");
            1
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_engine_new(runtime_cfg: *const hex_runtime_config_s) -> hex_engine_t {
    let Some(cfg) = (unsafe { runtime_cfg.as_ref() }).copied() else {
        return ptr::null_mut();
    };
    let shutdown = Arc::new(AtomicBool::new(false));
    crate::SHUTDOWN.store(false, Ordering::Relaxed);

    let (settings, instance_guard, instance_conflict_error, event_log, history) = if cfg
        .start_workers
    {
        let log_dir = crate::app_paths::init_process_logging(&crate::SHUTDOWN)
            .ok()
            .or_else(|| crate::app_paths::logs_dir().ok());
        let (instance_guard, instance_conflict_error) = match crate::instance::acquire("listener") {
            Ok(guard) => (Some(guard), None),
            Err(error) => {
                tracing::warn!(%error, "listener instance lock already held; running in conflict state");
                (
                    None,
                    Some(format!(
                        "Another HEX instance is already running ({error}). Quit the other HEX process and retry."
                    )),
                )
            }
        };
        let settings = AppSettings::load().unwrap_or_else(|error| {
            tracing::warn!(%error, "could not load settings.json; using defaults");
            AppSettings::default()
        });
        settings.apply_runtime();
        let history = match History::open_default(settings.history_retention) {
            Ok(history) => Some(history),
            Err(error) => {
                tracing::warn!(%error, "dictation history is unavailable");
                None
            }
        };
        let event_log = log_dir
            .map(|dir| dir.join("live.ndjson"))
            .and_then(|path| crate::events::EventLog::create(&path).ok());
        (
            settings,
            instance_guard,
            instance_conflict_error,
            event_log,
            history,
        )
    } else {
        let settings = AppSettings::default();
        settings.apply_runtime();
        (settings, None, None, None, None)
    };

    let setup_status = onboarding::status(&settings.transcription);
    let setup_ready = setup_status.ready();
    let instance_lock_held = !cfg.start_workers || instance_guard.is_some();
    if cfg.start_workers {
        tracing::info!(
            setup_ready,
            instance_lock_held,
            microphone = ?setup_status.microphone,
            input_monitoring = ?setup_status.input_monitoring,
            accessibility = ?setup_status.accessibility,
            transcription_model = setup_status.transcription_model,
            "native macOS engine initialized"
        );
    }

    let (indicator_sender, indicator_receiver) = dictation_indicator::channel();
    let (controller_event_sender, controller_event_receiver) = mpsc::sync_channel(32);
    let (developer_sender, developer_receiver) = mpsc::sync_channel(8);

    let preview_worker = if cfg.preview_dictation_hud {
        let preview_sender = indicator_sender.clone();
        let preview_shutdown = shutdown.clone();
        Some(thread::spawn(move || {
            while !preview_shutdown.load(Ordering::Relaxed) {
                preview_sender.send(DictationIndicatorEvent::Started);
                thread::sleep(Duration::from_millis(450));
                let started = Instant::now();
                while started.elapsed() < Duration::from_secs(3)
                    && !preview_shutdown.load(Ordering::Relaxed)
                {
                    let wave = (started.elapsed().as_secs_f32() * 5.0).sin() * 0.5 + 0.5;
                    preview_sender.send(DictationIndicatorEvent::Meter {
                        average: 0.025 + wave * 0.09,
                        peak: 0.15 + wave * 0.55,
                    });
                    thread::sleep(Duration::from_millis(20));
                }
                if preview_shutdown.load(Ordering::Relaxed) {
                    break;
                }
                preview_sender.send(DictationIndicatorEvent::Submitted { job_id: 0 });
                preview_sender.send(DictationIndicatorEvent::Transcribing { job_id: 0 });
                thread::sleep(Duration::from_millis(700));
                preview_sender.send(DictationIndicatorEvent::Processing { job_id: 0 });
                thread::sleep(Duration::from_millis(700));
                preview_sender.send(DictationIndicatorEvent::JobCompleted { job_id: 0 });
                thread::sleep(Duration::from_millis(900));
            }
        }))
    } else {
        None
    };

    let callback_sink = Arc::new(CallbackSink {
        userdata: cfg.userdata as usize,
        wakeup_cb: cfg.wakeup_cb,
        event_cb: cfg.event_cb,
        wakeup_pending: AtomicBool::new(false),
    });

    let available_microphones = if cfg.start_workers {
        crate::audio::input_device_names().unwrap_or_default()
    } else {
        vec!["MacBook Pro Microphone".to_string()]
    };
    let mut personal_commands_status = if cfg.start_workers {
        crate::personal_commands::load_status().unwrap_or_default()
    } else {
        StatusSnapshot::default()
    };
    crate::personal_commands::include_builtin_transformations(&mut personal_commands_status);
    let opencode_catalog_receiver = cfg
        .start_workers
        .then(|| start_opencode_catalog_worker(callback_sink.clone()));
    let opencode_catalog = if cfg.start_workers {
        OpenCodeCatalogState::Loading
    } else {
        OpenCodeCatalogState::Idle
    };
    let meetings_cache = if cfg.start_workers {
        meeting::list().unwrap_or_default()
    } else {
        Vec::new()
    };
    let event_reader = if cfg.start_workers {
        crate::app_paths::logs_dir()
            .ok()
            .map(|dir| EventReader::open(dir.join("live.ndjson")))
    } else {
        None
    };

    let atomic_hud = Arc::new(AtomicHudTelemetry::default());
    let mut initial_core = EngineCore {
        developer_features_enabled: cfg.developer_features_enabled,
        persist_settings: cfg.start_workers,
        start_workers: cfg.start_workers,
        preview_dictation_hud: cfg.preview_dictation_hud,
        instance_lock_held,
        instance_conflict_error,
        instance_guard,
        local_api: None,
        recognition_worker: None,
        controller_worker: None,
        event_log,
        developer_sender,
        controller_event_sender,
        settings,
        hud: HudTelemetryTracker::with_atomic(atomic_hud.clone()),
        meeting_state: MeetingMenuState::Idle,
        meeting_panel: HexMeetingPanelSnapshot::Hidden,
        pending_offer: None,
        ready_panel_deadline: None,
        active_meeting_transcript_len: 0,
        transcription_picker_language: None,
        transcription_preparation: TranscriptionPreparation::default(),
        transcription_deletion_error: None,
        available_microphones,
        opencode_catalog,
        opencode_catalog_receiver,
        last_opencode_missing_check: Instant::now(),
        personal_commands_status,
        personal_workspace_provisioning: false,
        personal_workspace_receiver: None,
        personal_workspace_error: None,
        installed_applications: Vec::new(),
        app_discovery_receiver: None,
        meetings_cache,
        event_reader,
        activity: DesktopActivity::default(),
        recent_events: Vec::new(),
        current_context: (None, None),
        last_command_model_status: crate::recognition::command_model_status(),
        history,
        recognition_controls: None,
        meeting_commands: None,
        meeting_requests: None,
        indicator_sender,
        mailbox: VecDeque::new(),
        next_generation: 0,
        callback_sink: callback_sink.clone(),
    };
    if cfg.start_workers {
        let _ = initial_core.refresh_activity_cache();
        initial_core.start_runtime_workers_if_needed();
    }
    let core = Arc::new(Mutex::new(initial_core));

    let coordinator_core = core.clone();
    let coordinator_shutdown = shutdown.clone();
    let coordinator_worker = Some(
        thread::Builder::new()
            .name("hex-ffi-coordinator".into())
            .spawn(move || {
                run_coordinator_loop(
                    coordinator_core,
                    coordinator_shutdown,
                    indicator_receiver,
                    controller_event_receiver,
                    developer_receiver,
                );
            })
            .expect("hex-ffi-coordinator thread must spawn"),
    );

    Box::into_raw(Box::new(hex_engine_s {
        core,
        atomic_hud,
        callback_sink,
        shutdown,
        coordinator_worker,
        preview_worker,
    }))
}

fn run_coordinator_loop(
    core: Arc<Mutex<EngineCore>>,
    shutdown: Arc<AtomicBool>,
    indicator_receiver: Receiver<DictationIndicatorEvent>,
    controller_events: Receiver<ControllerEvent>,
    developer_calls: Receiver<crate::developer_control::DeveloperCall>,
) {
    let mut last_timer_tick = Instant::now();
    while !shutdown.load(Ordering::Relaxed) && !crate::SHUTDOWN.load(Ordering::Relaxed) {
        let mut did_work = false;
        while let Ok(event) = indicator_receiver.try_recv() {
            did_work = true;
            let is_completed = matches!(event, DictationIndicatorEvent::JobCompleted { .. });
            let mut guard = core.lock().unwrap_or_else(|e| e.into_inner());
            let phase_changed = guard.hud.apply(event);
            if phase_changed {
                let telemetry = guard.hud.telemetry;
                guard.enqueue_event(hex_event_tag_e::HEX_EVENT_HUD_CHANGED, &telemetry);
            }
            if is_completed {
                let _ = guard.refresh_activity_cache();
                let snapshot = guard.bootstrap_snapshot();
                guard.enqueue_event(hex_event_tag_e::HEX_EVENT_HISTORY_UPDATED, &snapshot);
            }
        }
        while let Ok(event) = controller_events.try_recv() {
            did_work = true;
            let mut guard = core.lock().unwrap_or_else(|e| e.into_inner());
            guard.apply_controller_event(event);
        }
        while let Ok(call) = developer_calls.try_recv() {
            did_work = true;
            let reply = handle_developer_call(&core, call.command);
            let _ = call.reply.send(reply);
        }
        {
            let mut guard = core.lock().unwrap_or_else(|e| e.into_inner());
            guard.poll_preparation();
            if last_timer_tick.elapsed() >= Duration::from_millis(500) {
                guard.maintain_timers();
                last_timer_tick = Instant::now();
            }
        }
        if !did_work {
            match indicator_receiver.recv_timeout(Duration::from_millis(16)) {
                Ok(event) => {
                    let is_completed =
                        matches!(event, DictationIndicatorEvent::JobCompleted { .. });
                    let mut guard = core.lock().unwrap_or_else(|e| e.into_inner());
                    let phase_changed = guard.hud.apply(event);
                    if phase_changed {
                        let telemetry = guard.hud.telemetry;
                        guard.enqueue_event(hex_event_tag_e::HEX_EVENT_HUD_CHANGED, &telemetry);
                    }
                    if is_completed {
                        let _ = guard.refresh_activity_cache();
                        let snapshot = guard.bootstrap_snapshot();
                        guard.enqueue_event(hex_event_tag_e::HEX_EVENT_HISTORY_UPDATED, &snapshot);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }
}

fn handle_developer_call(
    core: &Arc<Mutex<EngineCore>>,
    command: crate::developer_control::DeveloperCommand,
) -> crate::developer_control::DeveloperReply {
    use crate::developer_control::{DeveloperCommand, DeveloperHudState, DeveloperReply};
    let mut guard = core.lock().unwrap_or_else(|e| e.into_inner());
    match command {
        DeveloperCommand::Status => DeveloperReply::State {
            window_open: false,
            pane: None,
            commands_enabled: guard.settings.commands_enabled,
        },
        DeveloperCommand::Hud { state } => {
            let state_name = match state {
                DeveloperHudState::Reset => "reset",
                DeveloperHudState::Recording => "recording",
                DeveloperHudState::Transcribing => "transcribing",
                DeveloperHudState::Processing => "processing",
            };
            let _ = guard.dispatch(HexEngineCommand::SimulateHudState {
                state: state_name.to_string(),
                rms: Some(0.08),
                peak: Some(0.42),
            });
            DeveloperReply::Ok
        }
        DeveloperCommand::ShowPane { pane } => {
            guard.enqueue_event(hex_event_tag_e::HEX_EVENT_DEV_COMMAND_REQUESTED, &pane);
            DeveloperReply::Ok
        }
        DeveloperCommand::SetCommandsEnabled { enabled } => {
            if let Err(error) = guard.settings.set_commands_enabled(enabled) {
                return DeveloperReply::error("settings-conflict", error.to_string());
            }
            match guard.settings.save() {
                Ok(()) => {
                    let snapshot = guard.bootstrap_snapshot();
                    guard.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                    DeveloperReply::Ok
                }
                Err(error) => DeveloperReply::error("settings-save-failed", error.to_string()),
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_engine_free(engine: hex_engine_t) {
    if engine.is_null() {
        return;
    }
    let mut boxed = unsafe { Box::from_raw(engine) };
    boxed.shutdown.store(true, Ordering::Release);
    crate::SHUTDOWN.store(true, Ordering::Release);
    let (local_api, recognition_worker, controller_worker) = {
        let mut guard = boxed.core.lock().unwrap_or_else(|e| e.into_inner());
        guard.transcription_preparation.cancel();
        (
            guard.local_api.take(),
            guard.recognition_worker.take(),
            guard.controller_worker.take(),
        )
    };
    if let Some(mut local_api) = local_api {
        local_api.shutdown();
    }
    if let Some(worker) = boxed.preview_worker.take() {
        let _ = worker.join();
    }
    if let Some(worker) = boxed.coordinator_worker.take() {
        let _ = worker.join();
    }
    if let Some(worker) = recognition_worker {
        let _ = worker.join();
    }
    if let Some(worker) = controller_worker {
        let _ = worker.join();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_engine_tick(engine: hex_engine_t) {
    let Some(engine_ref) = (unsafe { engine.as_ref() }) else {
        return;
    };
    engine_ref
        .callback_sink
        .wakeup_pending
        .store(false, Ordering::Release);

    let events: Vec<QueuedEvent> = {
        let mut guard = engine_ref.core.lock().unwrap_or_else(|e| e.into_inner());
        guard.poll_preparation();
        guard.mailbox.drain(..).collect()
    };

    let Some(event_cb) = engine_ref.callback_sink.event_cb else {
        return;
    };
    let userdata = engine_ref.callback_sink.userdata as *mut c_void;
    for item in events {
        let slice = hex_slice_s {
            ptr: item.payload.as_ptr().cast::<c_char>(),
            len: item.payload.len(),
        };
        let raw_event = hex_event_s {
            tag: item.tag,
            generation: item.generation,
            json_payload: slice,
        };
        unsafe { event_cb(userdata, &raw const raw_event) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_engine_read_hud_telemetry(
    engine: hex_engine_t,
) -> hex_hud_telemetry_s {
    let Some(engine_ref) = (unsafe { engine.as_ref() }) else {
        return hex_hud_telemetry_s::default();
    };
    engine_ref.atomic_hud.load()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_engine_copy_bootstrap_snapshot_json(
    engine: hex_engine_t,
) -> hex_slice_s {
    let Some(engine_ref) = (unsafe { engine.as_ref() }) else {
        return hex_slice_s::default();
    };
    let guard = engine_ref.core.lock().unwrap_or_else(|e| e.into_inner());
    allocate_json_slice(&guard.bootstrap_snapshot())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_engine_copy_history_page_json(
    engine: hex_engine_t,
    query_utf8: hex_slice_s,
    limit: u32,
) -> hex_slice_s {
    let Some(engine_ref) = (unsafe { engine.as_ref() }) else {
        return hex_slice_s::default();
    };
    let query = slice_to_str(query_utf8).unwrap_or("");
    let guard = engine_ref.core.lock().unwrap_or_else(|e| e.into_inner());
    let entries: Vec<HistoryEntry> = guard
        .history
        .as_ref()
        .map(|history| {
            history
                .search(query)
                .into_iter()
                .take(limit as usize)
                .collect()
        })
        .unwrap_or_default();
    allocate_json_slice(&entries)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_engine_copy_meeting_detail_json(
    engine: hex_engine_t,
    meeting_id_utf8: hex_slice_s,
) -> hex_slice_s {
    let Some(engine_ref) = (unsafe { engine.as_ref() }) else {
        return hex_slice_s::default();
    };
    let Ok(meeting_id) = slice_to_str(meeting_id_utf8) else {
        return hex_slice_s::default();
    };
    let manifest_opt = meeting::list()
        .ok()
        .and_then(|items| items.into_iter().find(|item| item.id == meeting_id))
        .or_else(|| {
            let guard = engine_ref.core.lock().unwrap_or_else(|e| e.into_inner());
            guard
                .meetings_cache
                .iter()
                .find(|item| item.id == meeting_id)
                .cloned()
        });
    let detail = manifest_opt.map(|manifest| {
        let mut reader = meeting::TranscriptReader::default();
        match reader.read(&meeting_id) {
            Ok(entries) => {
                let coalesced = meeting::coalesce_transcript(entries);
                let transcript_markdown = meeting::transcript_markdown(&coalesced);
                let turns = coalesced
                    .iter()
                    .scan(None, |previous_end: &mut Option<u64>, turn| {
                        let pause_before_ms = previous_end
                            .map(|end| turn.start_ms.saturating_sub(end))
                            .filter(|pause| *pause >= 30_000);
                        *previous_end =
                            Some(previous_end.map_or(turn.end_ms, |end| end.max(turn.end_ms)));
                        Some(HexMeetingTurnSnapshot {
                            source: match turn.source {
                                MeetingSource::Microphone => "you".to_string(),
                                MeetingSource::System => "computer".to_string(),
                            },
                            clock: meeting::format_duration(turn.start_ms),
                            start_ms: turn.start_ms,
                            end_ms: turn.end_ms,
                            pause_before_ms,
                            text: turn.text.clone(),
                        })
                    })
                    .collect();
                HexMeetingDetailSnapshot {
                    manifest: manifest_to_row_snapshot(&manifest),
                    turns,
                    transcript_markdown,
                    transcript_error: None,
                }
            }
            Err(error) => HexMeetingDetailSnapshot {
                manifest: manifest_to_row_snapshot(&manifest),
                turns: Vec::new(),
                transcript_markdown: String::new(),
                transcript_error: Some(error.to_string()),
            },
        }
    });
    allocate_json_slice(&detail)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_string_free(str: hex_slice_s) {
    if str.ptr.is_null() || str.len == 0 {
        return;
    }
    unsafe {
        let slice_ptr = ptr::slice_from_raw_parts_mut(str.ptr.cast_mut().cast::<u8>(), str.len);
        drop(Box::from_raw(slice_ptr));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn hex_engine_dispatch_json(
    engine: hex_engine_t,
    action_json: hex_slice_s,
    out_error_json: *mut hex_slice_s,
) -> bool {
    let write_err = |msg: String| {
        if !out_error_json.is_null() {
            let payload = serde_json::json!({ "error": msg });
            unsafe { *out_error_json = allocate_json_slice(&payload) };
        }
    };
    let Some(engine_ref) = (unsafe { engine.as_ref() }) else {
        write_err("engine handle is null".into());
        return false;
    };
    let raw = match slice_to_str(action_json) {
        Ok(raw) => raw,
        Err(error) => {
            write_err(error);
            return false;
        }
    };
    let command: HexEngineCommand = match serde_json::from_str(raw) {
        Ok(cmd) => cmd,
        Err(error) => {
            write_err(format!("invalid command JSON: {error}"));
            return false;
        }
    };
    let result = {
        let mut guard = engine_ref.core.lock().unwrap_or_else(|e| e.into_inner());
        guard.dispatch(command)
    };
    match result {
        Ok(()) => true,
        Err(error) => {
            write_err(error);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{offset_of, size_of};
    use std::process::Command;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn ffi_header_matches_exported_c_abi_and_clang_layout() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let header_path = manifest_dir.join("include/hex.h");
        let header = std::fs::read_to_string(&header_path).expect("include/hex.h must exist");
        let swift_header_path =
            manifest_dir.join("macos/HexMacPackage/Sources/HexEngineFFI/include/hex.h");
        if swift_header_path.exists() {
            let swift_header = std::fs::read_to_string(&swift_header_path).unwrap();
            assert_eq!(
                header, swift_header,
                "macos/HexMacPackage/Sources/HexEngineFFI/include/hex.h drifted from include/hex.h"
            );
        }
        let rust_source = std::fs::read_to_string(manifest_dir.join("src/ffi.rs"))
            .expect("src/ffi.rs must exist");

        // 1. Verify every #[unsafe(no_mangle)] extern "C" fn in src/ffi.rs appears in include/hex.h
        let mut exported_fns = Vec::new();
        for line in rust_source.lines() {
            if let Some(rest) = line.trim().strip_prefix("pub unsafe extern \"C\" fn ")
                && let Some(name) = rest.split('(').next()
            {
                exported_fns.push(name.trim().to_string());
            }
        }
        assert!(
            !exported_fns.is_empty(),
            "must discover exported C ABI functions"
        );
        for func in &exported_fns {
            assert!(
                header.contains(&format!("{func}(")),
                "include/hex.h is missing declaration for exported function {func}"
            );
        }

        // 2. Compile a C probe against include/hex.h with clang to verify C99/C11 validity and exact layout parity.
        let probe_dir = std::env::temp_dir().join(format!("hex-ffi-probe-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&probe_dir);
        let probe_c = probe_dir.join("probe.c");
        let probe_bin = probe_dir.join("probe");
        let c_code = format!(
            r#"
#include <assert.h>
#include <stddef.h>
#include <stdio.h>
#include "{header}"

int main(void) {{
    assert(sizeof(hex_slice_s) == {slice_size});
    assert(offsetof(hex_slice_s, ptr) == {slice_ptr});
    assert(offsetof(hex_slice_s, len) == {slice_len});

    assert(sizeof(hex_hud_telemetry_s) == {hud_size});
    assert(offsetof(hex_hud_telemetry_s, phase) == {hud_phase});
    assert(offsetof(hex_hud_telemetry_s, rms_level) == {hud_rms});
    assert(offsetof(hex_hud_telemetry_s, peak_level) == {hud_peak});
    assert(offsetof(hex_hud_telemetry_s, active_queue_count) == {hud_queue});
    assert(offsetof(hex_hud_telemetry_s, mode_color_rgba) == {hud_color});
    assert(offsetof(hex_hud_telemetry_s, phase_started_ns) == {hud_started});
    assert(offsetof(hex_hud_telemetry_s, is_command_mode_awake) == {hud_awake});
    assert(offsetof(hex_hud_telemetry_s, is_voice_action) == {hud_voice_action});
    assert(offsetof(hex_hud_telemetry_s, is_post_processing) == {hud_post});
    assert(offsetof(hex_hud_telemetry_s, is_capturing) == {hud_capturing});

    assert(sizeof(hex_event_s) == {event_size});
    assert(offsetof(hex_event_s, tag) == {event_tag});
    assert(offsetof(hex_event_s, generation) == {event_gen});
    assert(offsetof(hex_event_s, json_payload) == {event_payload});

    assert(sizeof(hex_runtime_config_s) == {cfg_size});
    assert(offsetof(hex_runtime_config_s, userdata) == {cfg_userdata});
    assert(offsetof(hex_runtime_config_s, wakeup_cb) == {cfg_wakeup});
    assert(offsetof(hex_runtime_config_s, event_cb) == {cfg_event});
    assert(offsetof(hex_runtime_config_s, developer_features_enabled) == {cfg_dev});
    assert(offsetof(hex_runtime_config_s, start_workers) == {cfg_workers});
    assert(offsetof(hex_runtime_config_s, preview_dictation_hud) == {cfg_preview});

    assert(HEX_HUD_PHASE_HIDDEN == 0);
    assert(HEX_HUD_PHASE_ERROR == 7);
    assert(HEX_EVENT_READINESS_CHANGED == 1);
    assert(HEX_EVENT_QUIT_REQUESTED == 15);
    return 0;
}}
"#,
            header = header_path.display(),
            slice_size = size_of::<hex_slice_s>(),
            slice_ptr = offset_of!(hex_slice_s, ptr),
            slice_len = offset_of!(hex_slice_s, len),
            hud_size = size_of::<hex_hud_telemetry_s>(),
            hud_phase = offset_of!(hex_hud_telemetry_s, phase),
            hud_rms = offset_of!(hex_hud_telemetry_s, rms_level),
            hud_peak = offset_of!(hex_hud_telemetry_s, peak_level),
            hud_queue = offset_of!(hex_hud_telemetry_s, active_queue_count),
            hud_color = offset_of!(hex_hud_telemetry_s, mode_color_rgba),
            hud_started = offset_of!(hex_hud_telemetry_s, phase_started_ns),
            hud_awake = offset_of!(hex_hud_telemetry_s, is_command_mode_awake),
            hud_voice_action = offset_of!(hex_hud_telemetry_s, is_voice_action),
            hud_post = offset_of!(hex_hud_telemetry_s, is_post_processing),
            hud_capturing = offset_of!(hex_hud_telemetry_s, is_capturing),
            event_size = size_of::<hex_event_s>(),
            event_tag = offset_of!(hex_event_s, tag),
            event_gen = offset_of!(hex_event_s, generation),
            event_payload = offset_of!(hex_event_s, json_payload),
            cfg_size = size_of::<hex_runtime_config_s>(),
            cfg_userdata = offset_of!(hex_runtime_config_s, userdata),
            cfg_wakeup = offset_of!(hex_runtime_config_s, wakeup_cb),
            cfg_event = offset_of!(hex_runtime_config_s, event_cb),
            cfg_dev = offset_of!(hex_runtime_config_s, developer_features_enabled),
            cfg_workers = offset_of!(hex_runtime_config_s, start_workers),
            cfg_preview = offset_of!(hex_runtime_config_s, preview_dictation_hud),
        );
        std::fs::write(&probe_c, c_code).unwrap();
        let compile = Command::new("clang")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror"])
            .arg(&probe_c)
            .arg("-o")
            .arg(&probe_bin)
            .status()
            .expect("clang must be available");
        assert!(
            compile.success(),
            "include/hex.h failed clang -Wall -Werror check"
        );
        let run = Command::new(&probe_bin).status().unwrap();
        assert!(run.success(), "include/hex.h C layout assertions failed");
        let _ = std::fs::remove_dir_all(&probe_dir);
    }

    #[test]
    fn engine_lifecycle_tick_mailbox_and_wakeup_coalescing() {
        struct TestHarness {
            wakeups: AtomicUsize,
            events: Mutex<Vec<(hex_event_tag_e, u64, String)>>,
        }

        unsafe extern "C" fn on_wakeup(userdata: *mut c_void) {
            let harness = unsafe { &*(userdata as *const TestHarness) };
            harness.wakeups.fetch_add(1, Ordering::SeqCst);
        }

        unsafe extern "C" fn on_event(userdata: *mut c_void, event: *const hex_event_s) {
            let harness = unsafe { &*(userdata as *const TestHarness) };
            let ev = unsafe { *event };
            let text = slice_to_str(ev.json_payload).unwrap().to_string();
            harness
                .events
                .lock()
                .unwrap()
                .push((ev.tag, ev.generation, text));
        }

        let harness = TestHarness {
            wakeups: AtomicUsize::new(0),
            events: Mutex::new(Vec::new()),
        };

        let cfg = hex_runtime_config_s {
            userdata: (&raw const harness).cast_mut().cast::<c_void>(),
            wakeup_cb: Some(on_wakeup),
            event_cb: Some(on_event),
            developer_features_enabled: true,
            start_workers: false,
            preview_dictation_hud: false,
        };

        let engine = unsafe { hex_engine_new(&raw const cfg) };
        assert!(!engine.is_null());

        let snapshot_slice = unsafe { hex_engine_copy_bootstrap_snapshot_json(engine) };
        let snapshot_str = slice_to_str(snapshot_slice).unwrap();
        let snapshot: HexBootstrapSnapshot = serde_json::from_str(snapshot_str).unwrap();
        assert_eq!(snapshot.version, env!("CARGO_PKG_VERSION"));
        assert!(snapshot.developer_features_enabled);
        assert_eq!(
            snapshot.hud_telemetry.phase,
            hex_hud_phase_e::HEX_HUD_PHASE_HIDDEN
        );
        unsafe { hex_string_free(snapshot_slice) };

        // Dispatch two commands before ticking: wakeup_cb must coalesce to 1 call.
        let cmd1 = br#"{"type":"simulate_hud_state","state":"listening","rms":0.05,"peak":0.3}"#;
        let cmd2 = br#"{"type":"simulate_meeting_panel","panel":{"kind":"offer","key":"zoom-1","title":"Zoom call detected","subtitle":"Record locally?","source_label":"Zoom","remaining_seconds":20}}"#;
        let mut err_slice = hex_slice_s::default();
        assert!(unsafe {
            hex_engine_dispatch_json(
                engine,
                hex_slice_s {
                    ptr: cmd1.as_ptr().cast::<c_char>(),
                    len: cmd1.len(),
                },
                &raw mut err_slice,
            )
        });
        assert!(unsafe {
            hex_engine_dispatch_json(
                engine,
                hex_slice_s {
                    ptr: cmd2.as_ptr().cast::<c_char>(),
                    len: cmd2.len(),
                },
                &raw mut err_slice,
            )
        });
        assert_eq!(harness.wakeups.load(Ordering::SeqCst), 1);

        let telemetry = {
            // Hold the engine core mutex on this thread while reading HUD telemetry
            // to prove hex_engine_read_hud_telemetry is lock-free (no deadlock).
            let _held_lock = unsafe { (*engine).core.lock().unwrap() };
            unsafe { hex_engine_read_hud_telemetry(engine) }
        };
        assert_eq!(telemetry.phase, hex_hud_phase_e::HEX_HUD_PHASE_LISTENING);
        assert!(telemetry.is_capturing);
        assert!(telemetry.rms_level > 0.4);

        {
            let mut guard = unsafe { (*engine).core.lock().unwrap() };
            guard.instance_lock_held = false;
            guard.instance_conflict_error = Some("Another HEX instance is running".into());
            let snap = guard.bootstrap_snapshot();
            assert!(!snap.instance_lock_held);
            assert!(!snap.setup_ready);
            assert_eq!(
                snap.instance_conflict_error.as_deref(),
                Some("Another HEX instance is running")
            );
            guard.instance_lock_held = true;
            guard.instance_conflict_error = None;
        }

        unsafe { hex_engine_tick(engine) };
        let delivered = harness.events.lock().unwrap();
        assert_eq!(delivered.len(), 2);
        assert_eq!(delivered[0].0, hex_event_tag_e::HEX_EVENT_HUD_CHANGED);
        assert_eq!(delivered[0].1, 1);
        assert_eq!(
            delivered[1].0,
            hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED
        );
        assert_eq!(delivered[1].1, 2);
        drop(delivered);

        unsafe { hex_engine_free(engine) };
    }

    #[test]
    fn hud_telemetry_state_machine_tracks_capture_voice_action_and_queue() {
        let mut tracker = HudTelemetryTracker::default();
        assert_eq!(
            tracker.telemetry.phase,
            hex_hud_phase_e::HEX_HUD_PHASE_HIDDEN
        );

        assert!(tracker.apply(DictationIndicatorEvent::Started));
        assert_eq!(
            tracker.telemetry.phase,
            hex_hud_phase_e::HEX_HUD_PHASE_LISTENING
        );
        assert!(tracker.telemetry.is_capturing);
        assert!(!tracker.telemetry.is_voice_action);

        assert!(!tracker.apply(DictationIndicatorEvent::Meter {
            average: 0.05,
            peak: 0.25,
        }));
        assert!((tracker.telemetry.rms_level - 0.45).abs() < 0.001);
        assert!((tracker.telemetry.peak_level - 0.75).abs() < 0.001);

        assert!(tracker.apply(DictationIndicatorEvent::PromotedToVoiceAction));
        assert_eq!(
            tracker.telemetry.phase,
            hex_hud_phase_e::HEX_HUD_PHASE_VOICE_ACTION
        );
        assert!(tracker.telemetry.is_voice_action);

        assert!(tracker.apply(DictationIndicatorEvent::Submitted { job_id: 1 }));
        assert_eq!(
            tracker.telemetry.phase,
            hex_hud_phase_e::HEX_HUD_PHASE_TRANSCRIBING
        );
        assert!(!tracker.telemetry.is_capturing);
        assert_eq!(tracker.telemetry.active_queue_count, 0);

        assert!(tracker.apply(DictationIndicatorEvent::Submitted { job_id: 2 }));
        assert_eq!(tracker.telemetry.active_queue_count, 1);

        assert!(tracker.apply(DictationIndicatorEvent::Processing { job_id: 1 }));
        assert!(tracker.telemetry.is_post_processing);

        assert!(tracker.apply(DictationIndicatorEvent::JobCompleted { job_id: 1 }));
        assert_eq!(
            tracker.telemetry.phase,
            hex_hud_phase_e::HEX_HUD_PHASE_TRANSCRIBING
        );
        assert_eq!(tracker.telemetry.active_queue_count, 0);

        assert!(tracker.apply(DictationIndicatorEvent::JobCompleted { job_id: 2 }));
        assert_eq!(
            tracker.telemetry.phase,
            hex_hud_phase_e::HEX_HUD_PHASE_COMPLETED
        );
    }

    #[test]
    fn settings_and_pane_commands_roundtrip_and_enforce_invariants() {
        let cfg = hex_runtime_config_s {
            userdata: ptr::null_mut(),
            wakeup_cb: None,
            event_cb: None,
            developer_features_enabled: true,
            start_workers: false,
            preview_dictation_hud: false,
        };
        let engine = unsafe { hex_engine_new(&raw const cfg) };
        assert!(!engine.is_null());

        let dispatch = |json: &str| -> Result<(), String> {
            let mut err_slice = hex_slice_s::default();
            let ok = unsafe {
                hex_engine_dispatch_json(
                    engine,
                    hex_slice_s {
                        ptr: json.as_ptr().cast::<c_char>(),
                        len: json.len(),
                    },
                    &raw mut err_slice,
                )
            };
            if ok {
                Ok(())
            } else {
                let err_str = slice_to_str(err_slice).unwrap_or("").to_string();
                unsafe { hex_string_free(err_slice) };
                Err(err_str)
            }
        };

        let read_snapshot = || -> HexBootstrapSnapshot {
            let slice = unsafe { hex_engine_copy_bootstrap_snapshot_json(engine) };
            let text = slice_to_str(slice).unwrap();
            let parsed: HexBootstrapSnapshot = serde_json::from_str(text).unwrap();
            unsafe { hex_string_free(slice) };
            parsed
        };

        let initial = read_snapshot();
        assert!(!initial.transcription_catalog.languages.is_empty());
        assert!(!initial.transcription_catalog.models.is_empty());
        assert!(!initial.commands_snapshot.catalog.is_empty());
        assert!(!initial.commands_snapshot.transformations.is_empty());
        assert_eq!(initial.dictation_hotkey_side.as_deref(), Some("either"));

        // Switch dictation standalone modifier side to right.
        dispatch(r#"{"type":"set_hotkey_side","target":"dictation","side":"right"}"#).unwrap();
        let after_side = read_snapshot();
        assert_eq!(after_side.dictation_hotkey_side.as_deref(), Some("right"));
        assert_eq!(after_side.dictation_hotkey_keycaps, vec!["R⌥".to_string()]);

        // Double-tap only requires a key shortcut on dictation_hotkey; standalone Option must reject.
        assert!(dispatch(r#"{"type":"set_double_tap_only","enabled":true}"#).is_err());

        // Mutual exclusion between commands_enabled and release_microphone_while_idle.
        dispatch(
            r#"{"type":"set_commands_enabled","enabled":true,"confirm_keep_microphone_ready":false}"#,
        )
        .unwrap();
        assert!(read_snapshot().commands_enabled);
        assert!(
            dispatch(
                r#"{"type":"set_release_microphone_while_idle","enabled":true,"confirm_disable_commands":false}"#
            )
            .is_err()
        );
        dispatch(
            r#"{"type":"set_release_microphone_while_idle","enabled":true,"confirm_disable_commands":true}"#,
        )
        .unwrap();
        let after_confirm = read_snapshot();
        assert!(!after_confirm.commands_enabled);
        assert!(after_confirm.release_microphone_while_idle);

        // Voice action toggle + model update.
        dispatch(r#"{"type":"set_voice_action_enabled","enabled":true}"#).unwrap();
        dispatch(
            r#"{"type":"set_voice_action_model","model":"anthropic/claude-sonnet-4-5","variant":"low","deadline_seconds":15}"#,
        )
        .unwrap();
        let after_va = read_snapshot();
        assert!(after_va.voice_action_enabled);
        assert_eq!(
            after_va.settings.voice_action.model.as_deref(),
            Some("anthropic/claude-sonnet-4-5")
        );
        assert_eq!(
            after_va.settings.voice_action.variant.as_deref(),
            Some("low")
        );
        assert_eq!(after_va.settings.voice_action.deadline_seconds, 15);

        unsafe { hex_engine_free(engine) };
    }

    #[test]
    fn headless_ffi_dictation_capture_transcribe_and_paste_cycle() {
        use crate::app_settings::{LEFT_OPTION_MASK, OPTION_KEY_MASK, TextReplacement};
        use crate::audio::CaptureInstant;
        use crate::context::ContextSnapshot;
        use crate::dictation::{
            DictationCapture, Finish, MINIMUM_HOLD_DURATION, POST_RELEASE_TAIL,
        };
        use crate::history::{HistoryRetention, HistoryStore};
        use crate::parakeet::{
            DictationJobStage, DictationWorker, TranscriptionTarget, WorkerEvent,
        };
        use crate::suppression::{HotkeyAction, HotkeySuppression, InputEvent};

        let temp_dir = std::env::temp_dir().join(format!(
            "hex-ffi-dictation-test-{}-{}",
            std::process::id(),
            crate::events::now_ms()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let history_path = temp_dir.join("history.json");
        let history = History::new(HistoryStore::open(
            history_path,
            HistoryRetention::Week,
            crate::events::now_ms(),
        ));

        let cfg = hex_runtime_config_s {
            userdata: ptr::null_mut(),
            wakeup_cb: None,
            event_cb: None,
            developer_features_enabled: true,
            start_workers: false,
            preview_dictation_hud: false,
        };
        let engine = unsafe { hex_engine_new(&raw const cfg) };
        assert!(!engine.is_null());

        {
            let mut guard = unsafe { (*engine).core.lock().unwrap() };
            guard.history = Some(history.clone());
        }

        let dispatch = |json: &str| {
            let mut err_slice = hex_slice_s::default();
            let ok = unsafe {
                hex_engine_dispatch_json(
                    engine,
                    hex_slice_s {
                        ptr: json.as_ptr().cast::<c_char>(),
                        len: json.len(),
                    },
                    &raw mut err_slice,
                )
            };
            if !ok {
                let err_str = slice_to_str(err_slice).unwrap_or("").to_string();
                unsafe { hex_string_free(err_slice) };
                panic!("dispatch failed for {json}: {err_str}");
            }
        };

        // 1. Capture Left-Option dictation shortcut with raw CGEventFlags (OPTION | LEFT_OPTION)
        //    and verify runtime_hotkeys() reflects the left-side binding immediately.
        let left_option_flags = OPTION_KEY_MASK | LEFT_OPTION_MASK;
        dispatch(&format!(
            r#"{{"type":"capture_hotkey","target":"dictation","key":null,"key_code":null,"modifier_flags":{left_option_flags}}}"#
        ));
        let active_hotkey = crate::app_settings::runtime_hotkeys().dictation;
        assert!(active_hotkey.matches_flags(left_option_flags));

        // 2. Configure a Global mode replacement via FFI and verify runtime profiles update.
        let mut mode = {
            let guard = unsafe { (*engine).core.lock().unwrap() };
            guard.settings.dictation_processing.default_mode.clone()
        };
        mode.replacements = vec![TextReplacement {
            matched_phrase: "initiative".into(),
            output: "milestone".into(),
        }];
        let update_cmd =
            serde_json::to_string(&HexEngineCommand::UpdateDefaultMode { mode }).unwrap();
        dispatch(&update_cmd);

        // 3. Load synthetic speech audio from ~/Library/Caches/hex-echo-scratch/synth/part1.wav
        //    when present, or generate a 2-second speech-level waveform.
        let synth_wav = dirs::home_dir()
            .map(|h| h.join("Library/Caches/hex-echo-scratch/synth/part1.wav"))
            .filter(|p| p.is_file());
        let (sample_rate, speech_samples): (u32, Vec<f32>) = if let Some(path) = synth_wav {
            let mut reader = hound::WavReader::open(&path).expect("open synth part1.wav");
            let spec = reader.spec();
            let max_frames = (spec.sample_rate as usize) * 5;
            let samples: Vec<f32> = match spec.sample_format {
                hound::SampleFormat::Int => reader
                    .samples::<i16>()
                    .take(max_frames)
                    .map(|s| f32::from(s.unwrap()) / f32::from(i16::MAX))
                    .collect(),
                hound::SampleFormat::Float => reader
                    .samples::<f32>()
                    .take(max_frames)
                    .map(|s| s.unwrap())
                    .collect(),
            };
            (spec.sample_rate, samples)
        } else {
            let sr = 16_000_u32;
            let samples = (0..(sr as usize * 2))
                .map(|i| ((i as f32 * 220.0 * std::f32::consts::TAU) / sr as f32).sin() * 0.25)
                .collect();
            (sr, samples)
        };

        // 4. Drive HotkeySuppression + DictationCapture across press -> intentional hold -> release + tail.
        let mut suppression = HotkeySuppression::default();
        let mut capture = DictationCapture::new(sample_rate).expect("create DictationCapture");
        let chunk_frames = (sample_rate as usize / 50).max(1); // 20 ms chunks
        let mut now = CaptureInstant::from_nanos(1_000_000_000);

        // Warm 450 ms pre-roll before hotkey press.
        for chunk in speech_samples.chunks(chunk_frames).take(23) {
            now = now + Duration::from_millis(20);
            capture.push_samples(chunk, now, None);
        }

        // Press Left Option -> HotkeyAction::Start.
        let press_at = now;
        assert_eq!(
            suppression.process(
                InputEvent::Flags {
                    flags: left_option_flags
                },
                active_hotkey,
                press_at
            ),
            Some(HotkeyAction::Start)
        );
        let indicator_sender = {
            let guard = unsafe { (*engine).core.lock().unwrap() };
            guard.indicator_sender.clone()
        };
        capture
            .start(press_at, None, None)
            .expect("start dictation capture");
        indicator_sender.send(DictationIndicatorEvent::Started);

        // Feed audio through intentional hold (>= MINIMUM_HOLD_DURATION).
        let mut became_intentional = false;
        for chunk in speech_samples.chunks(chunk_frames) {
            now = now + Duration::from_millis(20);
            capture.push_samples(chunk, now, None);
            if now.duration_since(press_at) >= MINIMUM_HOLD_DURATION {
                became_intentional |= capture.become_intentional(now);
            }
        }
        assert!(
            became_intentional,
            "capture must cross MINIMUM_HOLD_DURATION"
        );

        // Release Left Option -> HotkeyAction::Finish.
        let release_at = now;
        assert_eq!(
            suppression.process(InputEvent::Flags { flags: 0 }, active_hotkey, release_at),
            Some(HotkeyAction::Finish)
        );

        // Feed POST_RELEASE_TAIL (200 ms) of audio and finish capture.
        let tail_chunks = (POST_RELEASE_TAIL.as_millis() as usize / 20).max(1);
        let silence_chunk = vec![0.0_f32; chunk_frames];
        for _ in 0..tail_chunks {
            now = now + Duration::from_millis(20);
            capture.push_samples(&silence_chunk, now, None);
        }
        let finish = capture.finish_at(release_at + POST_RELEASE_TAIL);
        let Finish::Transcribe(clip) = finish else {
            panic!("expected Finish::Transcribe, got Finish::Discard");
        };
        assert!(
            !clip.is_near_silent(),
            "synthetic speech clip must not be near-silent"
        );

        // 5. Submit clip to DictationWorker with a fake paste sink and assert text is pasted and recorded in History.
        let pasted_outputs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let pasted_sink = pasted_outputs.clone();
        let transformations = Arc::new(crate::personal_commands::TransformationClient::new(
            temp_dir.clone(),
            temp_dir.clone(),
            temp_dir.join("personal-commands.json"),
        ));
        let worker = DictationWorker::start_for_test(
            transformations,
            Some(history),
            move |text, _mode| {
                pasted_sink.lock().unwrap().push(text.to_string());
                Ok(())
            },
            |_samples| {
                Ok("Okay, let's get started. I want to go through each initiative.".to_string())
            },
        );

        let job_id = worker
            .transcribe(
                clip,
                TranscriptionTarget::Paste,
                None,
                ContextSnapshot::default(),
            )
            .expect("submit dictation job");
        indicator_sender.send(DictationIndicatorEvent::Submitted {
            job_id: job_id.value(),
        });

        let deadline = Instant::now() + Duration::from_secs(30);
        let mut completed_text = None;
        while Instant::now() < deadline {
            if let Some(event) = worker.try_recv() {
                match event {
                    WorkerEvent::Stage { job_id, stage } => {
                        indicator_sender.send(match stage {
                            DictationJobStage::Transcribing => {
                                DictationIndicatorEvent::Transcribing {
                                    job_id: job_id.value(),
                                }
                            }
                            DictationJobStage::Processing => DictationIndicatorEvent::Processing {
                                job_id: job_id.value(),
                            },
                        });
                    }
                    WorkerEvent::Completed {
                        job_id,
                        result: Ok(text),
                        ..
                    } => {
                        indicator_sender.send(DictationIndicatorEvent::JobCompleted {
                            job_id: job_id.value(),
                        });
                        completed_text = Some(text);
                        break;
                    }
                    WorkerEvent::Completed {
                        result: Err(err), ..
                    } => panic!("dictation worker failed: {err}"),
                    other => panic!("unexpected worker event: {other:?}"),
                }
            }
            thread::sleep(Duration::from_millis(10));
        }

        let final_text = completed_text.expect("dictation job must complete with transcript");
        assert!(
            !final_text.trim().is_empty(),
            "transcribed text must be non-empty"
        );
        assert!(
            final_text.to_lowercase().contains("started")
                && final_text.to_lowercase().contains("milestone"),
            "expected transcript with mode replacement ('initiative' -> 'milestone'), got: {final_text:?}"
        );

        let pasted = pasted_outputs.lock().unwrap().clone();
        assert_eq!(pasted, vec![final_text.clone()]);

        // Wait briefly for coordinator loop to process JobCompleted HUD telemetry.
        let hud_deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < hud_deadline {
            let hud = unsafe { hex_engine_read_hud_telemetry(engine) };
            if hud.phase == hex_hud_phase_e::HEX_HUD_PHASE_COMPLETED {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            unsafe { hex_engine_read_hud_telemetry(engine) }.phase,
            hex_hud_phase_e::HEX_HUD_PHASE_COMPLETED
        );

        // Verify FFI history query returns the pasted dictation entry.
        let history_slice =
            unsafe { hex_engine_copy_history_page_json(engine, hex_slice_s::default(), 10) };
        let history_json = slice_to_str(history_slice).unwrap().to_string();
        unsafe { hex_string_free(history_slice) };
        let entries: Vec<crate::history::HistoryEntry> =
            serde_json::from_str(&history_json).expect("decode history entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].final_text, final_text);

        // Reset global runtime settings back to defaults before freeing.
        AppSettings::default().apply_runtime();
        unsafe { hex_engine_free(engine) };
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

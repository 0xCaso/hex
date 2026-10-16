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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::app_settings::AppSettings;
use crate::dictation_indicator::{self, DictationIndicatorEvent, DictationIndicatorSender};
use crate::history::{History, HistoryEntry};
use crate::meeting::{self, MeetingRequest};
use crate::meeting_detection::MeetingCandidate;
use crate::meeting_watcher::{
    ControllerCommand, ControllerEvent, MeetingMenuState, OFFER_TIMEOUT, READY_TIMEOUT,
};
use crate::onboarding::{self, PermissionState};
use crate::recognition::RecognitionControl;
use crate::status_item::{meeting_menu, model_menu_entries};
use crate::transcription_models::{TranscriptionModelId, is_installed};
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HexPermissionSnapshot {
    pub microphone_granted: bool,
    pub accessibility_granted: bool,
    pub input_monitoring_granted: bool,
    pub model_installed: bool,
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HexBootstrapSnapshot {
    pub version: String,
    pub developer_features_enabled: bool,
    pub pid: u32,
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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HexEngineCommand {
    RefreshSnapshot,
    SelectInstalledModel {
        model_id: String,
    },
    CancelModelPreparation,
    PasteLastDictation,
    StartMeeting,
    StopMeeting,
    AcceptMeetingOffer,
    DismissMeetingOffer,
    DismissCallEndedPrompt,
    RevealMeetingTranscript {
        path: String,
    },
    SetSoundVolume {
        volume: f32,
    },
    SetShowDockIcon {
        visible: bool,
    },
    SetMeetingsEnabled {
        enabled: bool,
    },
    SimulateHudState {
        state: String,
        rms: Option<f32>,
        peak: Option<f32>,
    },
    SimulateMeetingPanel {
        panel: HexMeetingPanelSnapshot,
    },
    RequestCheckForUpdates,
    RequestQuit,
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
}

impl Default for HudTelemetryTracker {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            telemetry: hex_hud_telemetry_s {
                mode_color_rgba: 0xFF06_09FF,
                ..Default::default()
            },
            jobs: BTreeMap::new(),
        }
    }
}

impl HudTelemetryTracker {
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

        previous_phase != self.telemetry.phase
            || previous_post != self.telemetry.is_post_processing
            || previous_voice_action != self.telemetry.is_voice_action
            || previous_queue != self.telemetry.active_queue_count
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

struct EngineCore {
    developer_features_enabled: bool,
    settings: AppSettings,
    hud: HudTelemetryTracker,
    meeting_state: MeetingMenuState,
    meeting_panel: HexMeetingPanelSnapshot,
    pending_offer: Option<(MeetingCandidate, Instant)>,
    ready_panel_deadline: Option<Instant>,
    transcription_preparation: TranscriptionPreparation,
    history: Option<History>,
    recognition_controls: Option<SyncSender<RecognitionControl>>,
    meeting_commands: Option<SyncSender<ControllerCommand>>,
    meeting_requests: Option<SyncSender<MeetingRequest>>,
    _indicator_sender: DictationIndicatorSender,
    mailbox: VecDeque<QueuedEvent>,
    next_generation: u64,
    callback_sink: Arc<CallbackSink>,
}

const MAX_MAILBOX_EVENTS: usize = 256;

impl EngineCore {
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

    fn permission_snapshot(&self) -> (bool, HexPermissionSnapshot) {
        let status = onboarding::status(&self.settings.transcription);
        (
            status.ready(),
            HexPermissionSnapshot {
                microphone_granted: status.microphone == PermissionState::Ready,
                accessibility_granted: status.accessibility == PermissionState::Ready,
                input_monitoring_granted: status.input_monitoring == PermissionState::Ready,
                model_installed: status.transcription_model,
            },
        )
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
        HexBootstrapSnapshot {
            version: env!("CARGO_PKG_VERSION").to_string(),
            developer_features_enabled: self.developer_features_enabled,
            pid: std::process::id(),
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
        }
    }

    fn poll_preparation(&mut self) {
        let Some(result) = self.transcription_preparation.poll() else {
            return;
        };
        match result {
            Ok(selection) => {
                if let Err(error) = self.settings.save_transcription(selection) {
                    tracing::warn!(%error, "could not persist prepared transcription model");
                    self.transcription_preparation
                        .set_error(Some(error.to_string()));
                } else {
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
            }
            ControllerEvent::NotesFinished(manifest) => {
                if let HexMeetingPanelSnapshot::Ready { meeting_id, .. } = &self.meeting_panel
                    && *meeting_id == manifest.id
                {
                    self.meeting_panel = Self::ready_panel_from_manifest(&manifest);
                    let panel = self.meeting_panel.clone();
                    self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
                }
            }
            ControllerEvent::Failed { title, error } => {
                tracing::error!(%title, %error, "meeting recording failed");
                self.pending_offer = None;
                self.ready_panel_deadline = None;
                self.meeting_state = MeetingMenuState::Idle;
                self.meeting_panel = HexMeetingPanelSnapshot::Hidden;
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
            if let HexMeetingPanelSnapshot::Recording {
                elapsed_text,
                app_label,
                ..
            } = &self.meeting_panel
                && *elapsed_text != elapsed
            {
                self.meeting_panel = HexMeetingPanelSnapshot::Recording {
                    title: title.clone(),
                    app_label: app_label.clone(),
                    elapsed_text: elapsed,
                };
                let panel = self.meeting_panel.clone();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_PANEL_CHANGED, &panel);
            }
            let snapshot = self.bootstrap_snapshot();
            self.enqueue_event(hex_event_tag_e::HEX_EVENT_MEETING_STATE_CHANGED, &snapshot);
        }
    }

    fn dispatch(&mut self, command: HexEngineCommand) -> Result<(), String> {
        match command {
            HexEngineCommand::RefreshSnapshot => {
                if let Ok(loaded) = AppSettings::load() {
                    self.settings = loaded;
                }
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
            HexEngineCommand::SetSoundVolume { volume } => {
                let clamped = volume.clamp(0.0, 1.0);
                self.settings.sound_effects = clamped > 0.0;
                if clamped > 0.0 {
                    self.settings.sound_effect_volume = clamped;
                }
                self.settings.save().map_err(|e| e.to_string())?;
                if clamped > 0.0 {
                    crate::feedback::play(crate::feedback::Tone::DictationStart);
                }
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetShowDockIcon { visible } => {
                self.settings.show_dock_icon = visible;
                self.settings.save().map_err(|e| e.to_string())?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
                Ok(())
            }
            HexEngineCommand::SetMeetingsEnabled { enabled } => {
                self.settings.meetings.enabled = enabled;
                self.settings.save().map_err(|e| e.to_string())?;
                let snapshot = self.bootstrap_snapshot();
                self.enqueue_event(hex_event_tag_e::HEX_EVENT_SETTINGS_CHANGED, &snapshot);
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
    callback_sink: Arc<CallbackSink>,
    shutdown: Arc<AtomicBool>,
    _instance: Option<crate::instance::InstanceLock>,
    local_api: Option<crate::local_api::LocalApi>,
    recognition_worker: Option<JoinHandle<()>>,
    controller_worker: Option<JoinHandle<()>>,
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

    let (settings, instance_guard, event_log, history) = if cfg.start_workers {
        let log_dir = crate::app_paths::init_process_logging(&crate::SHUTDOWN).ok();
        let instance_guard = match crate::instance::acquire("listener") {
            Ok(guard) => Some(guard),
            Err(error) => {
                tracing::warn!(%error, "listener instance lock already held; running in UI-observer mode");
                None
            }
        };
        let settings = AppSettings::load().unwrap_or_else(|error| {
            tracing::warn!(%error, "could not load settings.json; using defaults");
            AppSettings::default()
        });
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
        (settings, instance_guard, event_log, history)
    } else {
        (AppSettings::default(), None, None, None)
    };

    let setup_status = onboarding::status(&settings.transcription);
    let setup_ready = setup_status.ready();
    if cfg.start_workers {
        tracing::info!(
            setup_ready,
            microphone = ?setup_status.microphone,
            input_monitoring = ?setup_status.input_monitoring,
            accessibility = ?setup_status.accessibility,
            transcription_model = setup_status.transcription_model,
            "native macOS engine initialized"
        );
    }

    let (indicator_sender, indicator_receiver) = dictation_indicator::channel();
    let (controller_event_sender, controller_event_receiver) = mpsc::sync_channel(32);
    let (controller_cmd_sender, controller_cmd_receiver) = mpsc::sync_channel(8);
    let (meeting_request_sender, meeting_request_receiver) = mpsc::sync_channel(8);
    let (recognition_control_sender, recognition_control_receiver) = mpsc::sync_channel(8);
    let (developer_sender, developer_receiver) = mpsc::sync_channel(8);

    let local_api = if cfg.start_workers && instance_guard.is_some() {
        event_log.as_ref().and_then(|events| {
            crate::local_api::LocalApi::start(
                events.clone(),
                crate::local_api::LocalApiOptions {
                    developer_control: cfg
                        .developer_features_enabled
                        .then(|| developer_sender.clone()),
                    dictation_control: Some(recognition_control_sender.clone()),
                },
            )
            .map_err(|error| {
                tracing::warn!(%error, "could not start local API");
                error
            })
            .ok()
        })
    } else {
        None
    };

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let controller_worker = if cfg.start_workers && !cfg.preview_dictation_hud {
        let controller_events = controller_event_sender.clone();
        let worker_root = project_root.clone();
        Some(thread::spawn(move || {
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
        }))
    } else {
        None
    };

    let recognition_worker =
        if cfg.start_workers && instance_guard.is_some() && !cfg.preview_dictation_hud {
            event_log.map(|recognition_events| {
            let failure_indicator = indicator_sender.clone();
            let recognition_indicator = indicator_sender.clone();
            let meeting_requests = Some(meeting_request_sender.clone());
            let recognition_history = history.clone();
            let worker_root = project_root.clone();
            thread::spawn(move || {
                if !setup_ready {
                    tracing::info!(
                        "waiting for permissions and local model before starting hotkey listener"
                    );
                    while !crate::SHUTDOWN.load(Ordering::Relaxed) {
                        let current = AppSettings::load().unwrap_or_default();
                        if onboarding::status(&current.transcription).ready() {
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
        })
        } else {
            None
        };

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

    let core = Arc::new(Mutex::new(EngineCore {
        developer_features_enabled: cfg.developer_features_enabled,
        settings,
        hud: HudTelemetryTracker::default(),
        meeting_state: MeetingMenuState::Idle,
        meeting_panel: HexMeetingPanelSnapshot::Hidden,
        pending_offer: None,
        ready_panel_deadline: None,
        transcription_preparation: TranscriptionPreparation::default(),
        history,
        recognition_controls: cfg.start_workers.then_some(recognition_control_sender),
        meeting_commands: cfg.start_workers.then_some(controller_cmd_sender),
        meeting_requests: cfg.start_workers.then_some(meeting_request_sender),
        _indicator_sender: indicator_sender,
        mailbox: VecDeque::new(),
        next_generation: 0,
        callback_sink: callback_sink.clone(),
    }));

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
        callback_sink,
        shutdown,
        _instance: instance_guard,
        local_api,
        recognition_worker,
        controller_worker,
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
            let mut guard = core.lock().unwrap_or_else(|e| e.into_inner());
            let phase_changed = guard.hud.apply(event);
            if phase_changed {
                let telemetry = guard.hud.telemetry;
                guard.enqueue_event(hex_event_tag_e::HEX_EVENT_HUD_CHANGED, &telemetry);
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
                    let mut guard = core.lock().unwrap_or_else(|e| e.into_inner());
                    let phase_changed = guard.hud.apply(event);
                    if phase_changed {
                        let telemetry = guard.hud.telemetry;
                        guard.enqueue_event(hex_event_tag_e::HEX_EVENT_HUD_CHANGED, &telemetry);
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
    {
        let guard = boxed.core.lock().unwrap_or_else(|e| e.into_inner());
        guard.transcription_preparation.cancel();
    }
    if let Some(mut local_api) = boxed.local_api.take() {
        local_api.shutdown();
    }
    if let Some(worker) = boxed.preview_worker.take() {
        let _ = worker.join();
    }
    if let Some(worker) = boxed.coordinator_worker.take() {
        let _ = worker.join();
    }
    if let Some(worker) = boxed.recognition_worker.take() {
        let _ = worker.join();
    }
    if let Some(worker) = boxed.controller_worker.take() {
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
    let guard = engine_ref.core.lock().unwrap_or_else(|e| e.into_inner());
    guard.hud.telemetry
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
    let Some(_engine_ref) = (unsafe { engine.as_ref() }) else {
        return hex_slice_s::default();
    };
    let Ok(meeting_id) = slice_to_str(meeting_id_utf8) else {
        return hex_slice_s::default();
    };
    let detail = meeting::list()
        .ok()
        .and_then(|items| items.into_iter().find(|item| item.id == meeting_id));
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

        let telemetry = unsafe { hex_engine_read_hud_telemetry(engine) };
        assert_eq!(telemetry.phase, hex_hud_phase_e::HEX_HUD_PHASE_LISTENING);
        assert!(telemetry.is_capturing);
        assert!(telemetry.rms_level > 0.4);

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
}

//! The Windows Settings window's snapshot-and-action contract with the
//! in-process runtime it drives. Linux has its own service contract in
//! `desktop_host`; macOS commits its settings directly.

use crate::desktop_activity::DesktopActivity;
use crate::transcription_models::{
    ModelPreparationStage, TranscriptionModelId, TranscriptionSelection,
};

/// The panes the Windows shell implements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DesktopCapabilities {
    pub(crate) activity: bool,
    pub(crate) commands: bool,
    pub(crate) history: bool,
    pub(crate) hud_lab: bool,
    pub(crate) meetings: bool,
    pub(crate) modes: bool,
    pub(crate) replacements: bool,
    pub(crate) listener_control: bool,
    pub(crate) update_restart: bool,
    pub(crate) voice_action: bool,
}

impl DesktopCapabilities {
    pub(crate) const fn windows() -> Self {
        Self {
            activity: true,
            commands: false,
            history: true,
            hud_lab: crate::DEVELOPER_FEATURES_ENABLED,
            meetings: false,
            modes: true,
            replacements: true,
            listener_control: true,
            update_restart: true,
            voice_action: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DesktopSnapshot {
    pub(crate) activity: DesktopActivity,
    /// The current session's completed transcripts, newest first.
    pub(crate) transcripts: Vec<String>,
    /// The newest dictation failure, until a later transcript completes.
    pub(crate) last_failure: Option<String>,
    pub(crate) dictation_shortcut: Vec<String>,
    pub(crate) dictation_shortcut_label: String,
    pub(crate) double_tap_lock: bool,
    pub(crate) double_tap_only: bool,
    pub(crate) shortcut_mode: crate::dictation::ShortcutMode,
    pub(crate) microphone: DesktopMicrophoneSnapshot,
    pub(crate) paste_last_shortcut: Option<Vec<String>>,
    pub(crate) listener: Option<DesktopListenerSnapshot>,
    pub(crate) operation_error: Option<String>,
    pub(crate) observations_path: String,
    pub(crate) transcription: DesktopTranscriptionSnapshot,
    pub(crate) update_status: DesktopUpdateStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DesktopMicrophoneSnapshot {
    pub(crate) devices: Vec<String>,
    pub(crate) error: Option<String>,
    pub(crate) selected: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DesktopTranscriptionSnapshot {
    pub(crate) downloaded_bytes: u64,
    pub(crate) error: Option<String>,
    pub(crate) preparation_stage: Option<ModelPreparationStage>,
    pub(crate) selection: TranscriptionSelection,
    pub(crate) preparing: Option<TranscriptionModelId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DesktopListenerSnapshot {
    pub(crate) running: bool,
    pub(crate) status: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum DesktopUpdateStatus {
    Unavailable,
    Checking,
    Current,
    Failed,
    Available,
    ReadyToRestart,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DesktopShortcut {
    pub(crate) alt: bool,
    pub(crate) control: bool,
    pub(crate) function: bool,
    pub(crate) key: String,
    pub(crate) platform: bool,
    pub(crate) shift: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum DesktopAction {
    CheckForUpdates,
    ClearError,
    RefreshMicrophones,
    RestartIntoUpdate,
    SetDictationShortcut(DesktopShortcut),
    SetDoubleTapLock(bool),
    SetDoubleTapOnly(bool),
    SetShortcutMode(crate::dictation::ShortcutMode),
    SetMicrophone(Option<String>),
    StartListening,
    StopListening,
}

const TRANSCRIPT_LIMIT: usize = 8;

/// The current session's completed transcripts, newest first, with their
/// timestamps. `events` is oldest first, as `EventReader` keeps them.
pub(crate) fn recent_transcripts<'a>(
    events: impl DoubleEndedIterator<Item = &'a crate::events::VoiceEvent>,
) -> Vec<(u64, String)> {
    use crate::events::{TranscriptPhase, VoiceEvent};
    let mut transcripts = Vec::new();
    for event in events.rev() {
        match event {
            VoiceEvent::SessionStarted { .. } => break,
            VoiceEvent::Transcript {
                timestamp_ms,
                phase: TranscriptPhase::Completed,
                text,
                ..
            } if !text.trim().is_empty() => {
                transcripts.push((*timestamp_ms, text.clone()));
                if transcripts.len() == TRANSCRIPT_LIMIT {
                    break;
                }
            }
            _ => {}
        }
    }
    transcripts
}

/// The newest failure, hidden once a later transcript completed, so a silent
/// failure still reaches the Settings window without sticking around.
pub(crate) fn visible_failure(
    activity: &DesktopActivity,
    transcripts: &[(u64, String)],
) -> Option<String> {
    activity
        .last_failure
        .as_ref()
        .filter(|(failed_at, _)| {
            transcripts
                .first()
                .is_none_or(|(completed_at, _)| completed_at < failed_at)
        })
        .map(|(_, message)| message.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{TranscriptPhase, VoiceEvent};

    fn transcript(timestamp_ms: u64, text: &str) -> VoiceEvent {
        VoiceEvent::Transcript {
            timestamp_ms,
            phase: TranscriptPhase::Completed,
            latency_ms: 10,
            text: text.into(),
        }
    }

    #[test]
    fn recent_transcripts_are_bounded_newest_first_and_session_scoped() {
        let mut events = vec![
            transcript(1, "Old session"),
            VoiceEvent::SessionStarted { timestamp_ms: 2 },
        ];
        events.extend((0..10).map(|index| transcript(index + 3, &format!("Transcript {index}"))));
        events.push(VoiceEvent::Transcript {
            timestamp_ms: 14,
            phase: TranscriptPhase::Updated,
            latency_ms: 10,
            text: "Partial".into(),
        });

        let transcripts = recent_transcripts(events.iter());

        assert_eq!(transcripts.len(), TRANSCRIPT_LIMIT);
        assert_eq!(transcripts[0].1, "Transcript 9");
        assert_eq!(transcripts[7].1, "Transcript 2");
    }

    #[test]
    fn newest_failure_shows_until_a_later_transcript_completes() {
        let activity = DesktopActivity {
            last_failure: Some((5, "OpenCode generation failed".into())),
            ..DesktopActivity::default()
        };

        assert_eq!(
            visible_failure(&activity, &[(4, "Earlier".into())]).as_deref(),
            Some("OpenCode generation failed")
        );
        assert_eq!(visible_failure(&activity, &[(6, "Recovered".into())]), None);
    }

    #[test]
    fn windows_exposes_the_managed_update_restart_action() {
        assert!(DesktopCapabilities::windows().update_restart);
    }
}

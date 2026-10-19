//! The Windows runtime's view of the shared settings. `settings.json` is the
//! one persisted file; `WindowsSettings` is projected from `AppSettings` for
//! the Windows dictation loop and shell, and merged back when saved. It also
//! reads the legacy `windows-settings.json` once to migrate it.

use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{Result, WrapErr, eyre};
use serde::{Deserialize, Serialize};
use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

pub use crate::app_settings::IndicatorPosition;
/// What happens to other applications' audio while a dictation records.
pub use crate::app_settings::RecordingAudioBehavior as WhileDictating;
use crate::app_settings::{
    AppSettings, DictationMode, HotkeyBinding, HotkeyKey, HotkeyModifiers, ModifierSide,
};
use crate::transcription_models::TranscriptionSelection;

const LOCALE_NAME_CAPACITY: usize = 85;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct WindowsSettings {
    pub schema_version: u32,
    pub platform: String,
    pub microphone: Option<String>,
    pub listen_on_launch: bool,
    pub dictation_hotkey: WindowsHotkey,
    pub shortcut_mode: crate::dictation::ShortcutMode,
    pub paste_last_hotkey: Option<WindowsHotkey>,
    pub voice_action_hotkey: Option<WindowsHotkey>,
    pub voice_action_model: Option<crate::dictation_processor::Model>,
    pub double_tap_lock: bool,
    pub double_tap_only: bool,
    pub while_dictating: WhileDictating,
    pub release_microphone_while_idle: bool,
    pub feedback_volume: u8,
    pub ui_language: Option<String>,
    pub indicator_position: IndicatorPosition,
    pub history_retention: crate::history::HistoryRetention,
    pub text_replacements: Vec<crate::text_replacements::TextReplacement>,
    /// Global rewrite profile used when no contextual mode matches.
    pub dictation_post_processing: crate::dictation_processor::PostProcessingSettings,
    /// Ordered final transformation IDs for the global fallback profile.
    pub dictation_transformations: Vec<String>,
    pub modes: Vec<WindowsMode>,
    pub transcription: TranscriptionSelection,
}

/// A contextual dictation mode: when its native Windows matching rule wins,
/// corrections and the shared OpenCode rewrite run after global replacements.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct WindowsMode {
    pub name: String,
    /// Case-insensitive substrings matched against the focused process's
    /// executable name (e.g. "chrome", "code", "slack").
    pub applications: Vec<String>,
    /// Exact hosts matched against the focused browser page (e.g.
    /// "x.com", "github.com"); case-insensitive, trailing dots ignored.
    pub websites: Vec<String>,
    pub corrections: Vec<crate::text_replacements::TextReplacement>,
    pub post_processing: crate::dictation_processor::PostProcessingSettings,
    pub transformations: Vec<String>,
}

impl WindowsMode {
    pub fn matches_application(&self, application: &str) -> bool {
        let application = application.to_lowercase();
        self.applications.iter().any(|candidate| {
            let candidate = candidate.trim().to_lowercase();
            !candidate.is_empty() && application.contains(&candidate)
        })
    }

    pub fn matches_website(&self, host: &str) -> bool {
        let host = crate::windows_context::normalize_host(host);
        self.websites.iter().any(|candidate| {
            // Entries are stored as typed; pasted URLs, ports, and IDN
            // text reduce to their host here.
            crate::windows_context::canonical_host(candidate)
                .is_some_and(|candidate| candidate == host)
        })
    }
}

impl WindowsSettings {
    pub fn transformations_enabled(&self) -> bool {
        !self.dictation_transformations.is_empty()
            || self
                .modes
                .iter()
                .any(|mode| !mode.transformations.is_empty())
    }
}

/// The first mode matching the focused context. A website rule is more
/// specific than an application rule, so browser-host matches win, like
/// the macOS selector ordering.
pub fn mode_for_context<'a>(
    modes: &'a [WindowsMode],
    application: Option<&str>,
    browser_host: Option<&str>,
) -> Option<&'a WindowsMode> {
    if let Some(host) = browser_host
        && let Some(mode) = modes.iter().find(|mode| mode.matches_website(host))
    {
        return Some(mode);
    }
    let application = application?;
    modes
        .iter()
        .find(|mode| mode.matches_application(application))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct WindowsHotkey {
    pub control: bool,
    pub windows: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: Option<String>,
}

impl Default for WindowsSettings {
    fn default() -> Self {
        Self::from_app(&AppSettings::default())
    }
}

impl Default for WindowsHotkey {
    fn default() -> Self {
        Self {
            control: true,
            windows: false,
            alt: false,
            shift: false,
            key: Some("space".into()),
        }
    }
}

impl WindowsSettings {
    /// The Windows view of `settings.json`.
    pub fn load() -> Result<Self> {
        let settings = Self::from_app(&AppSettings::load()?);
        settings.validate()?;
        Ok(settings)
    }

    /// Merges this view into `settings.json`, keeping every setting Windows
    /// does not own.
    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let mut settings = AppSettings::load()?;
        self.apply_to(&mut settings);
        settings.save()
    }

    fn validate(&self) -> Result<()> {
        self.dictation_hotkey.validate()?;
        if self.feedback_volume > 100 {
            return Err(eyre!("feedback volume must be between 0 and 100"));
        }
        if let Some(paste_last) = &self.paste_last_hotkey {
            paste_last.validate()?;
            if paste_last.key.is_none() {
                return Err(eyre!("Paste Last requires a non-modifier key"));
            }
            if paste_last == &self.dictation_hotkey {
                return Err(eyre!(
                    "the dictation and Paste Last shortcuts must be different"
                ));
            }
        }
        if let Some(voice_action) = &self.voice_action_hotkey {
            voice_action.validate()?;
            if voice_action.key.is_none() {
                return Err(eyre!(
                    "the voice action shortcut requires a non-modifier key"
                ));
            }
            if voice_action == &self.dictation_hotkey
                || Some(voice_action) == self.paste_last_hotkey.as_ref()
            {
                return Err(eyre!("the voice action shortcut must be unique"));
            }
        }
        crate::transcription_models::validate(&self.transcription)?;
        Ok(())
    }

    pub fn from_app(settings: &AppSettings) -> Self {
        let modes = &settings.dictation_processing;
        Self {
            schema_version: 1,
            platform: "windows".into(),
            microphone: settings.microphone.clone(),
            listen_on_launch: settings.enable_on_launch,
            dictation_hotkey: WindowsHotkey::from(&settings.dictation_hotkey),
            shortcut_mode: settings.shortcut_mode,
            paste_last_hotkey: settings.paste_last_hotkey.as_ref().map(WindowsHotkey::from),
            voice_action_hotkey: settings
                .voice_action
                .enabled
                .then(|| WindowsHotkey::from(&settings.edit_hotkey)),
            voice_action_model: settings.voice_action.model.as_deref().and_then(|key| {
                let (provider, id) = key.split_once('/')?;
                Some(crate::dictation_processor::Model {
                    provider: provider.into(),
                    id: id.into(),
                    variant: settings.voice_action.variant.clone(),
                })
            }),
            double_tap_lock: settings.double_tap_lock,
            double_tap_only: settings.double_tap_only,
            while_dictating: settings.recording_audio_behavior,
            release_microphone_while_idle: settings.release_microphone_while_idle,
            feedback_volume: if settings.sound_effects {
                (settings.sound_effect_volume.clamp(0.0, 1.0) * 100.0).round() as u8
            } else {
                0
            },
            ui_language: settings.ui_language.clone(),
            indicator_position: settings.indicator_position,
            history_retention: settings.history_retention,
            text_replacements: modes.default_mode.replacements.clone(),
            dictation_post_processing: modes.default_mode.post_processing.clone(),
            dictation_transformations: modes.default_mode.transformations.clone(),
            modes: modes.modes.iter().map(WindowsMode::from).collect(),
            transcription: settings.transcription.clone(),
        }
    }

    /// Writes the settings Windows owns into `settings`.
    pub fn apply_to(&self, settings: &mut AppSettings) {
        settings.microphone = self.microphone.clone();
        settings.enable_on_launch = self.listen_on_launch;
        settings.dictation_hotkey = HotkeyBinding::from(&self.dictation_hotkey);
        settings.shortcut_mode = self.shortcut_mode;
        settings.paste_last_hotkey = self.paste_last_hotkey.as_ref().map(HotkeyBinding::from);
        settings.voice_action.enabled = self.voice_action_hotkey.is_some();
        if let Some(hotkey) = &self.voice_action_hotkey {
            settings.edit_hotkey = HotkeyBinding::from(hotkey);
        }
        settings.voice_action.model = self
            .voice_action_model
            .as_ref()
            .map(|model| format!("{}/{}", model.provider, model.id));
        settings.voice_action.variant = self
            .voice_action_model
            .as_ref()
            .and_then(|model| model.variant.clone());
        settings.double_tap_lock = self.double_tap_lock;
        settings.double_tap_only = self.double_tap_only;
        settings.recording_audio_behavior = self.while_dictating;
        settings.release_microphone_while_idle = self.release_microphone_while_idle;
        settings.sound_effects = self.feedback_volume > 0;
        if self.feedback_volume > 0 {
            settings.sound_effect_volume = f32::from(self.feedback_volume.min(100)) / 100.0;
        }
        settings.ui_language = self.ui_language.clone();
        settings.indicator_position = self.indicator_position;
        settings.history_retention = self.history_retention;
        let processing = &mut settings.dictation_processing;
        processing.default_mode.replacements = self.text_replacements.clone();
        processing.default_mode.post_processing = self.dictation_post_processing.clone();
        processing.default_mode.transformations = self.dictation_transformations.clone();
        processing.modes = self.modes.iter().map(DictationMode::from).collect();
        settings.transcription = self.transcription.clone();
    }
}

impl From<&DictationMode> for WindowsMode {
    fn from(mode: &DictationMode) -> Self {
        Self {
            name: mode.name.clone(),
            applications: mode.applications.clone(),
            websites: mode.browser_hosts.clone(),
            corrections: mode.replacements.clone(),
            post_processing: mode.post_processing.clone(),
            transformations: mode.transformations.clone(),
        }
    }
}

impl From<&WindowsMode> for DictationMode {
    fn from(mode: &WindowsMode) -> Self {
        Self {
            name: mode.name.clone(),
            applications: mode.applications.clone(),
            browser_hosts: mode.websites.clone(),
            replacements: mode.corrections.clone(),
            transformations: mode.transformations.clone(),
            post_processing: mode.post_processing.clone(),
        }
    }
}

/// Windows names Command the Windows key and Option Alt; keys go by name.
impl From<&HotkeyBinding> for WindowsHotkey {
    fn from(binding: &HotkeyBinding) -> Self {
        Self {
            control: binding.modifiers.control.is_some(),
            windows: binding.modifiers.command.is_some(),
            alt: binding.modifiers.option.is_some(),
            shift: binding.modifiers.shift.is_some(),
            key: binding.key.as_ref().map(HotkeyKey::name),
        }
    }
}

impl From<&WindowsHotkey> for HotkeyBinding {
    fn from(hotkey: &WindowsHotkey) -> Self {
        let side = |held: bool| held.then_some(ModifierSide::Either);
        Self {
            modifiers: HotkeyModifiers {
                control: side(hotkey.control),
                option: side(hotkey.alt),
                shift: side(hotkey.shift),
                command: side(hotkey.windows),
                function: false,
            },
            key: hotkey.key.as_deref().map(HotkeyKey::named),
        }
    }
}

/// The legacy `windows-settings.json` in `directory`, converted, for a
/// one-time migration into `settings.json`.
pub(crate) fn legacy_settings(directory: &Path) -> Result<Option<(PathBuf, AppSettings)>> {
    let path = directory.join("windows-settings.json");
    if !path.exists() {
        return Ok(None);
    }
    let legacy: WindowsSettings = serde_json::from_slice(&fs::read(&path)?)
        .wrap_err_with(|| format!("could not parse {}", path.display()))?;
    let mut settings = AppSettings::default();
    legacy.apply_to(&mut settings);
    Ok(Some((path, settings)))
}

impl WindowsHotkey {
    pub fn paste_last_default() -> Self {
        Self {
            control: true,
            windows: false,
            alt: true,
            shift: false,
            key: Some("v".into()),
        }
    }

    pub fn voice_action_default() -> Self {
        Self {
            control: true,
            windows: false,
            alt: true,
            shift: false,
            key: Some("i".into()),
        }
    }

    pub fn validate(&self) -> Result<()> {
        let modifier_count = [self.control, self.windows, self.alt, self.shift]
            .into_iter()
            .filter(|enabled| *enabled)
            .count();
        let Some(key) = self.key.as_deref() else {
            if modifier_count < 2 {
                return Err(eyre!(
                    "modifier-only Windows dictation shortcuts require at least two modifiers"
                ));
            }
            return Ok(());
        };
        if key.trim().is_empty() {
            return Err(eyre!("Windows dictation shortcut key cannot be empty"));
        }
        crate::windows_input::virtual_key(key)?;
        let function_key = key
            .strip_prefix('f')
            .or_else(|| key.strip_prefix('F'))
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=24).contains(&number));
        if !function_key && modifier_count == 0 {
            return Err(eyre!(
                "Windows dictation shortcuts require a modifier or a function key"
            ));
        }
        Ok(())
    }

    pub fn label(&self) -> String {
        self.keycaps().join("+")
    }

    pub fn keycaps(&self) -> Vec<String> {
        let mut parts = Vec::new();
        if self.control {
            parts.push("Ctrl".into());
        }
        if self.windows {
            parts.push("Win".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        if let Some(key) = &self.key {
            parts.push(match key.to_ascii_lowercase().as_str() {
                "space" => "Space".into(),
                "enter" | "return" => "Enter".into(),
                key => key.to_ascii_uppercase(),
            });
        }
        parts
    }
}

/// Whether a persisted settings file exists; a first run has none, and
/// the app then offers onboarding.
pub fn exists() -> bool {
    crate::app_settings::exists()
}

pub(crate) fn user_language() -> String {
    let mut buffer = [0_u16; LOCALE_NAME_CAPACITY];
    let length = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if length <= 1 {
        return "en".into();
    }
    let locale = String::from_utf16_lossy(&buffer[..length as usize - 1]);
    locale
        .split(['-', '_'])
        .next()
        .unwrap_or("en")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcription_models::TranscriptionModelId;

    #[test]
    fn polish_uses_the_multilingual_model() {
        let selection = crate::transcription_models::recommended_selection("pl");
        assert_eq!(selection.model, TranscriptionModelId::ParakeetV3);
        assert_eq!(selection.language, "pl");
    }

    #[test]
    fn supported_languages_keep_their_recommended_model() {
        let selection = crate::transcription_models::recommended_selection("de");
        assert_eq!(selection.model, TranscriptionModelId::ParakeetV3);
        assert_eq!(selection.language, "de");
    }

    #[test]
    fn unknown_languages_fall_back_to_english() {
        let selection = crate::transcription_models::recommended_selection("tlh");
        assert_eq!(
            selection.model,
            TranscriptionModelId::ParakeetUnifiedEnglish
        );
        assert_eq!(selection.language, "en");
    }

    #[test]
    fn default_binding_is_ctrl_space() {
        let binding = WindowsHotkey::default();
        assert_eq!(binding.label(), "Ctrl+Space");
        assert!(binding.validate().is_ok());
    }

    #[test]
    fn shortcut_mode_defaults_to_hold_and_round_trips() {
        let legacy: WindowsSettings = serde_json::from_str(r#"{"platform":"windows"}"#).unwrap();
        assert_eq!(legacy.shortcut_mode, crate::dictation::ShortcutMode::Hold);
        let settings = WindowsSettings {
            shortcut_mode: crate::dictation::ShortcutMode::Toggle,
            ..WindowsSettings::default()
        };
        let saved = serde_json::to_string(&settings).unwrap();
        assert!(saved.contains(r#""shortcut_mode":"toggle""#));
        assert_eq!(
            serde_json::from_str::<WindowsSettings>(&saved)
                .unwrap()
                .shortcut_mode,
            crate::dictation::ShortcutMode::Toggle
        );
    }

    #[test]
    fn retained_history_defaults_to_one_week() {
        assert_eq!(
            WindowsSettings::default().history_retention,
            crate::history::HistoryRetention::Week
        );
    }

    #[test]
    fn older_settings_default_mode_processing_without_migration() {
        let settings: WindowsSettings = serde_json::from_str(
            r#"{
                "platform": "windows",
                "modes": [{
                    "name": "Code",
                    "applications": ["code"],
                    "corrections": []
                }]
            }"#,
        )
        .expect("older Windows settings remain readable");

        assert!(!settings.dictation_post_processing.enabled);
        assert_eq!(
            settings.dictation_post_processing,
            crate::dictation_processor::PostProcessingSettings::default()
        );
        assert_eq!(
            settings.modes[0].post_processing,
            crate::dictation_processor::PostProcessingSettings::default()
        );
        assert!(settings.dictation_transformations.is_empty());
        assert!(settings.modes[0].transformations.is_empty());
    }

    #[test]
    fn mode_processing_round_trips_with_windows_settings() {
        let processing = crate::dictation_processor::PostProcessingSettings {
            enabled: true,
            prompt: "Keep code identifiers exact.".into(),
            model: Some("openai/gpt-5.6-sol".into()),
            variant: Some("fast".into()),
            deadline_seconds: 15,
        };
        let settings = WindowsSettings {
            dictation_post_processing: processing.clone(),
            dictation_transformations: vec!["lowercase".into()],
            modes: vec![WindowsMode {
                name: "Code".into(),
                post_processing: processing,
                transformations: vec!["trim-code-block".into(), "lowercase".into()],
                ..WindowsMode::default()
            }],
            ..WindowsSettings::default()
        };

        let encoded = serde_json::to_string(&settings).expect("settings serialize");
        let decoded: WindowsSettings =
            serde_json::from_str(&encoded).expect("settings deserialize");

        assert_eq!(decoded, settings);
    }

    #[test]
    fn modes_match_case_insensitive_application_substrings() {
        let modes = vec![
            WindowsMode {
                name: "Code".into(),
                applications: vec!["Code".into(), "rustrover".into()],
                ..WindowsMode::default()
            },
            WindowsMode {
                name: "Chat".into(),
                applications: vec!["slack".into()],
                ..WindowsMode::default()
            },
        ];

        assert_eq!(
            mode_for_context(&modes, Some("code"), None).map(|mode| mode.name.as_str()),
            Some("Code")
        );
        assert_eq!(
            mode_for_context(&modes, Some("slack"), None).map(|mode| mode.name.as_str()),
            Some("Chat")
        );
        // Substring semantics: "vscode" contains "code".
        assert_eq!(
            mode_for_context(&modes, Some("vscode"), None).map(|mode| mode.name.as_str()),
            Some("Code")
        );
        assert_eq!(mode_for_context(&modes, Some("notepad"), None), None);
        assert_eq!(mode_for_context(&modes, None, None), None);
    }

    #[test]
    fn website_rules_match_exact_hosts_and_beat_application_rules() {
        let modes = vec![
            WindowsMode {
                name: "Browser".into(),
                applications: vec!["chrome".into()],
                ..WindowsMode::default()
            },
            WindowsMode {
                name: "X".into(),
                websites: vec!["X.com".into()],
                ..WindowsMode::default()
            },
        ];

        // The exact-host website rule wins over the earlier application
        // rule when the page host matches.
        assert_eq!(
            mode_for_context(&modes, Some("chrome"), Some("x.com.")).map(|mode| mode.name.as_str()),
            Some("X")
        );
        // Subdomains are different hosts.
        assert_eq!(
            mode_for_context(&modes, Some("chrome"), Some("news.x.com"))
                .map(|mode| mode.name.as_str()),
            Some("Browser")
        );
        assert_eq!(
            mode_for_context(&modes, Some("chrome"), None).map(|mode| mode.name.as_str()),
            Some("Browser")
        );
    }

    #[test]
    fn blank_application_rules_never_match() {
        let modes = vec![WindowsMode {
            name: "Broken".into(),
            applications: vec!["  ".into(), String::new()],
            websites: vec![" ".into()],
            ..WindowsMode::default()
        }];
        assert_eq!(
            mode_for_context(&modes, Some("anything"), Some("x.com")),
            None
        );
    }

    #[test]
    fn double_tap_lock_is_enabled_by_default() {
        assert!(WindowsSettings::default().double_tap_lock);
    }

    #[test]
    fn paste_last_defaults_to_ctrl_alt_v() {
        assert_eq!(
            WindowsSettings::default()
                .paste_last_hotkey
                .expect("Paste Last is enabled")
                .label(),
            "Ctrl+Alt+V"
        );
    }

    #[test]
    fn feedback_volume_defaults_to_fifty_percent() {
        assert_eq!(WindowsSettings::default().feedback_volume, 50);
    }

    #[test]
    fn indicator_defaults_to_the_top_of_the_screen() {
        assert_eq!(
            WindowsSettings::default().indicator_position,
            IndicatorPosition::Top
        );
    }

    #[test]
    fn unmodified_letters_are_rejected() {
        let binding = WindowsHotkey {
            control: false,
            windows: false,
            alt: false,
            shift: false,
            key: Some("d".into()),
        };
        assert!(binding.validate().is_err());
    }

    #[test]
    fn voice_action_default_is_valid_and_unique() {
        let defaults = WindowsSettings::default();
        let preset = WindowsHotkey::voice_action_default();
        assert!(preset.validate().is_ok());
        assert!(preset.key.is_some(), "voice action requires a key");
        assert_ne!(preset, defaults.dictation_hotkey);
        assert_ne!(Some(&preset), defaults.paste_last_hotkey.as_ref());
    }

    fn scratch_directory(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "hex-windows-migration-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn legacy_windows_settings_migrate_once_into_settings_json() {
        let directory = scratch_directory("once");
        fs::write(
            directory.join("windows-settings.json"),
            br#"{
                "schema_version": 1,
                "platform": "windows",
                "microphone": "Headset",
                "listen_on_launch": false,
                "dictation_hotkey": {"control": true, "windows": true, "alt": false, "shift": false, "key": null},
                "shortcut_mode": "toggle",
                "paste_last_hotkey": {"control": true, "windows": false, "alt": true, "shift": false, "key": "v"},
                "voice_action_hotkey": {"control": true, "windows": false, "alt": true, "shift": false, "key": "i"},
                "voice_action_model": {"providerID": "openai", "id": "gpt-5.6-sol", "variant": "fast"},
                "double_tap_lock": false,
                "while_dictating": "pause_media",
                "feedback_volume": 30,
                "ui_language": "pl",
                "indicator_position": "bottom",
                "history_retention": "day",
                "text_replacements": [{"matched_phrase": "open code", "output": "OpenCode"}],
                "dictation_transformations": ["lowercase"],
                "modes": [{"name": "Code", "applications": ["code"], "websites": ["github.com"],
                           "corrections": [{"matched_phrase": "rust", "output": "Rust"}]}],
                "transcription": {"model": "parakeet_v3", "language": "pl", "recognition_hints": ""}
            }"#,
        )
        .unwrap();
        let path = directory.join("settings.json");

        crate::app_settings::migrate_legacy_settings(&path).unwrap();

        assert!(!directory.join("windows-settings.json").exists());
        assert!(directory.join("windows-settings.json.migrated").is_file());
        let migrated: AppSettings = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(migrated.microphone.as_deref(), Some("Headset"));
        assert!(!migrated.enable_on_launch);
        assert!(migrated.voice_action.enabled);
        assert_eq!(
            migrated.voice_action.model.as_deref(),
            Some("openai/gpt-5.6-sol")
        );
        assert_eq!(migrated.voice_action.variant.as_deref(), Some("fast"));
        assert_eq!(
            migrated.recording_audio_behavior,
            WhileDictating::PauseMedia
        );
        assert!(migrated.sound_effects);
        assert!((migrated.sound_effect_volume - 0.3).abs() < 1e-6);
        assert_eq!(migrated.ui_language.as_deref(), Some("pl"));
        assert_eq!(migrated.indicator_position, IndicatorPosition::Bottom);
        assert_eq!(
            migrated.dictation_processing.modes[0].browser_hosts,
            ["github.com"]
        );
        // The Windows view of the migrated file is the legacy file.
        let view = WindowsSettings::from_app(&migrated);
        assert_eq!(view.dictation_hotkey.label(), "Ctrl+Win");
        assert_eq!(view.paste_last_hotkey.unwrap().label(), "Ctrl+Alt+V");
        assert_eq!(view.voice_action_hotkey.unwrap().label(), "Ctrl+Alt+I");
        assert_eq!(view.shortcut_mode, crate::dictation::ShortcutMode::Toggle);
        assert!(!view.double_tap_lock);
        assert_eq!(view.feedback_volume, 30);
        assert_eq!(view.text_replacements[0].output, "OpenCode");
        assert_eq!(view.dictation_transformations, ["lowercase"]);
        assert_eq!(view.modes[0].corrections[0].output, "Rust");
        assert_eq!(view.transcription.language, "pl");

        // A second launch reads settings.json and leaves the backup alone.
        fs::write(directory.join("windows-settings.json"), b"{}").unwrap();
        crate::app_settings::migrate_legacy_settings(&path).unwrap();
        assert!(directory.join("windows-settings.json").is_file());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn the_windows_view_round_trips_through_app_settings() {
        let view = WindowsSettings {
            dictation_hotkey: WindowsHotkey {
                key: Some("pageup".into()),
                shift: true,
                ..WindowsHotkey::default()
            },
            voice_action_hotkey: Some(WindowsHotkey::voice_action_default()),
            feedback_volume: 0,
            ..WindowsSettings::default()
        };
        let mut settings = AppSettings::default();
        view.apply_to(&mut settings);
        assert_eq!(WindowsSettings::from_app(&settings), view);
        // Muted sounds keep the remembered volume for when they return.
        assert!(!settings.sound_effects);
        assert_eq!(settings.sound_effect_volume, 0.5);
    }

    #[test]
    fn one_modifier_is_not_safe_as_a_modifier_only_binding() {
        let binding = WindowsHotkey {
            control: true,
            windows: false,
            alt: false,
            shift: false,
            key: None,
        };
        assert!(binding.validate().is_err());
    }
}

//! The Linux runtime's view of the shared settings. `settings.json` is the
//! one persisted file; `LinuxSettings` is projected from `AppSettings` for the
//! service and its dictation loop, and merged back when saved. It also reads
//! the legacy `linux-settings.json` once to migrate it.

use std::fs;
use std::path::{Path, PathBuf};

use color_eyre::eyre::{Result, WrapErr, eyre};
use serde::{Deserialize, Serialize};
use x11rb::protocol::xproto::ModMask;
use x11rb::rust_connection::RustConnection;

use crate::app_settings::{AppSettings, HotkeyBinding, HotkeyKey, HotkeyModifiers, ModifierSide};

use crate::linux_input::{
    Keymap, XK_ALT_L, XK_ALT_R, XK_CONTROL_L, XK_CONTROL_R, XK_SHIFT_L, XK_SHIFT_R, XK_SUPER_L,
    XK_SUPER_R,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct LinuxSettings {
    pub schema_version: u32,
    pub dictation_hotkey: LinuxHotkey,
    pub double_tap_lock: bool,
    pub shortcut_mode: crate::dictation::ShortcutMode,
    #[serde(default = "legacy_paste_with_shift")]
    pub paste_with_shift: bool,
    pub sound_effect_volume: f32,
    pub transcription: crate::transcription_models::TranscriptionSelection,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct LinuxHotkey {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub key: String,
}

impl Default for LinuxSettings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            dictation_hotkey: LinuxHotkey::default(),
            double_tap_lock: true,
            shortcut_mode: crate::dictation::ShortcutMode::Hold,
            paste_with_shift: false,
            sound_effect_volume: 0.5,
            transcription: crate::transcription_models::TranscriptionSelection::default(),
        }
    }
}

fn legacy_paste_with_shift() -> bool {
    // Existing X11 settings predate this preference and always used Ctrl-Shift-V.
    true
}

impl Default for LinuxHotkey {
    fn default() -> Self {
        Self {
            control: false,
            alt: true,
            shift: false,
            super_key: false,
            key: "space".into(),
        }
    }
}

impl LinuxSettings {
    /// The Linux view of `settings.json`.
    pub fn load() -> Result<Self> {
        let settings = Self::from_app(&AppSettings::load()?);
        settings.validate()?;
        Ok(settings)
    }

    fn validate(&self) -> Result<()> {
        self.dictation_hotkey.validate()?;
        crate::transcription_models::validate(&self.transcription)?;
        if !(0.0..=1.0).contains(&self.sound_effect_volume) {
            return Err(eyre!("Sound volume must be between zero and one"));
        }
        Ok(())
    }

    /// Merges this view into `settings.json`, keeping every setting Linux
    /// does not own.
    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let mut settings = AppSettings::load()?;
        self.apply_to(&mut settings);
        settings.save()
    }

    pub fn from_app(settings: &AppSettings) -> Self {
        Self {
            schema_version: 1,
            dictation_hotkey: LinuxHotkey::from(&settings.dictation_hotkey),
            double_tap_lock: settings.double_tap_lock,
            shortcut_mode: settings.shortcut_mode,
            paste_with_shift: settings.paste_with_shift,
            sound_effect_volume: if settings.sound_effects {
                settings.sound_effect_volume
            } else {
                0.0
            },
            transcription: settings.transcription.clone(),
        }
    }

    /// Writes the settings Linux owns into `settings`.
    pub fn apply_to(&self, settings: &mut AppSettings) {
        settings.dictation_hotkey = HotkeyBinding::from(&self.dictation_hotkey);
        settings.double_tap_lock = self.double_tap_lock;
        settings.shortcut_mode = self.shortcut_mode;
        settings.paste_with_shift = self.paste_with_shift;
        settings.sound_effects = self.sound_effect_volume > 0.0;
        if self.sound_effect_volume > 0.0 {
            settings.sound_effect_volume = self.sound_effect_volume;
        }
        settings.transcription = self.transcription.clone();
    }
}

/// Linux names Command the Super key and Option Alt; keys go by name.
impl From<&HotkeyBinding> for LinuxHotkey {
    fn from(binding: &HotkeyBinding) -> Self {
        Self {
            control: binding.modifiers.control.is_some(),
            alt: binding.modifiers.option.is_some(),
            shift: binding.modifiers.shift.is_some(),
            super_key: binding.modifiers.command.is_some(),
            key: binding
                .key
                .as_ref()
                .map(HotkeyKey::name)
                .unwrap_or_default(),
        }
    }
}

impl From<&LinuxHotkey> for HotkeyBinding {
    fn from(hotkey: &LinuxHotkey) -> Self {
        let side = |held: bool| held.then_some(ModifierSide::Either);
        Self {
            modifiers: HotkeyModifiers {
                control: side(hotkey.control),
                option: side(hotkey.alt),
                shift: side(hotkey.shift),
                command: side(hotkey.super_key),
                function: false,
            },
            key: (!hotkey.key.trim().is_empty()).then(|| HotkeyKey::named(&hotkey.key)),
        }
    }
}

/// The legacy `linux-settings.json` in `directory`, converted, for a
/// one-time migration into `settings.json`.
pub(crate) fn legacy_settings(directory: &Path) -> Result<Option<(PathBuf, AppSettings)>> {
    let path = directory.join("linux-settings.json");
    if !path.exists() {
        return Ok(None);
    }
    let legacy: LinuxSettings = serde_json::from_slice(&fs::read(&path)?)
        .wrap_err_with(|| format!("could not parse {}", path.display()))?;
    let mut settings = AppSettings::default();
    legacy.apply_to(&mut settings);
    Ok(Some((path, settings)))
}

impl LinuxHotkey {
    pub fn validate(&self) -> Result<()> {
        if self.key.trim().is_empty() {
            return Err(eyre!("Linux hotkeys require a non-modifier key"));
        }
        Ok(())
    }

    pub fn label(&self) -> String {
        self.keycaps().join("+")
    }

    pub fn keycaps(&self) -> Vec<String> {
        let mut parts = Vec::new();
        if self.control {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.super_key {
            parts.push("Super".to_string());
        }
        let key = if self.key == "space" {
            "Space".into()
        } else {
            self.key.to_ascii_uppercase()
        };
        parts.push(key);
        parts
    }

    pub(crate) fn modifier_mask(
        &self,
        connection: &RustConnection,
        keymap: &Keymap,
    ) -> Result<ModMask> {
        let mut result = ModMask::default();
        for (enabled, symbols) in [
            (self.control, &[XK_CONTROL_L, XK_CONTROL_R][..]),
            (self.alt, &[XK_ALT_L, XK_ALT_R][..]),
            (self.shift, &[XK_SHIFT_L, XK_SHIFT_R][..]),
            (self.super_key, &[XK_SUPER_L, XK_SUPER_R][..]),
        ] {
            if enabled {
                result |= keymap.modifier_for(connection, symbols)?;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_binding_is_alt_space() {
        assert_eq!(
            LinuxSettings::default().dictation_hotkey.label(),
            "Alt+Space"
        );
    }

    #[test]
    fn modifier_only_bindings_are_rejected() {
        let binding = LinuxHotkey {
            key: String::new(),
            ..LinuxHotkey::default()
        };
        assert!(binding.validate().is_err());
    }

    #[test]
    fn standalone_function_keys_are_accepted() {
        let binding = LinuxHotkey {
            alt: false,
            key: "f12".into(),
            ..LinuxHotkey::default()
        };
        assert_eq!(binding.label(), "F12");
        assert!(binding.validate().is_ok());
    }

    #[test]
    fn settings_without_transcription_keep_the_default_selection() {
        let settings: LinuxSettings = serde_json::from_str(
            r#"{
                "schema_version": 1,
                "platform": "x11",
                "dictation_hotkey": {
                    "control": false,
                    "alt": true,
                    "shift": false,
                    "super_key": false,
                    "key": "space"
                },
                "double_tap_lock": true
            }"#,
        )
        .unwrap();

        assert_eq!(
            settings.transcription,
            crate::transcription_models::TranscriptionSelection::default()
        );
    }

    #[test]
    fn shortcut_mode_defaults_to_hold_and_round_trips() {
        let legacy: LinuxSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(legacy.shortcut_mode, crate::dictation::ShortcutMode::Hold);
        let toggled: LinuxSettings = serde_json::from_str(r#"{"shortcut_mode":"toggle"}"#).unwrap();
        assert_eq!(
            toggled.shortcut_mode,
            crate::dictation::ShortcutMode::Toggle
        );
    }

    #[test]
    fn legacy_settings_are_shared_between_display_sessions() {
        let legacy = r#"{"platform":"x11","double_tap_lock":false}"#;
        let settings: LinuxSettings = serde_json::from_str(legacy).unwrap();
        assert!(!settings.double_tap_lock);
        assert!(settings.paste_with_shift);
        // Display selection is runtime state, not a preference that survives logout.
        let saved = serde_json::to_value(&settings).unwrap();
        assert!(saved.get("platform").is_none());
        assert_eq!(
            serde_json::from_value::<LinuxSettings>(saved).unwrap(),
            settings
        );
    }

    #[test]
    fn fresh_settings_use_standard_paste_and_preserve_an_explicit_choice() {
        let settings = LinuxSettings::default();
        assert!(!settings.paste_with_shift);
        let saved = serde_json::to_value(&settings).unwrap();
        assert_eq!(
            serde_json::from_value::<LinuxSettings>(saved).unwrap(),
            settings
        );
        let legacy: LinuxSettings = serde_json::from_str(r#"{"paste_with_shift":false}"#).unwrap();
        assert!(!legacy.paste_with_shift);
    }

    #[test]
    fn recording_volume_defaults_for_new_and_legacy_settings_and_round_trips() {
        assert_eq!(LinuxSettings::default().sound_effect_volume, 0.5);
        let legacy: LinuxSettings = serde_json::from_str(r#"{"platform":"x11"}"#).unwrap();
        assert_eq!(legacy.sound_effect_volume, 0.5);
        for volume in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let settings = LinuxSettings {
                sound_effect_volume: volume,
                ..Default::default()
            };
            settings.validate().unwrap();
            let saved = serde_json::to_vec(&settings).unwrap();
            assert_eq!(
                serde_json::from_slice::<LinuxSettings>(&saved).unwrap(),
                settings
            );
        }
    }

    #[test]
    fn invalid_sound_volume_is_rejected_before_saving() {
        for volume in [-0.1, 1.1, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let settings = LinuxSettings {
                sound_effect_volume: volume,
                ..Default::default()
            };
            assert!(settings.validate().is_err());
        }
    }

    fn scratch_directory(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "hex-linux-migration-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn legacy_linux_settings_migrate_once_into_settings_json() {
        let directory = scratch_directory("once");
        fs::write(
            directory.join("linux-settings.json"),
            br#"{"schema_version":1,"dictation_hotkey":{"control":true,"alt":false,"shift":false,"super_key":true,"key":"f12"},"double_tap_lock":false,"shortcut_mode":"toggle","sound_effect_volume":0.25}"#,
        )
        .unwrap();
        let path = directory.join("settings.json");

        crate::app_settings::migrate_legacy_settings(&path).unwrap();

        assert!(!directory.join("linux-settings.json").exists());
        assert!(directory.join("linux-settings.json.migrated").is_file());
        let migrated: AppSettings = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let view = LinuxSettings::from_app(&migrated);
        assert_eq!(view.dictation_hotkey.label(), "Ctrl+Super+F12");
        assert!(!view.double_tap_lock);
        assert_eq!(view.shortcut_mode, crate::dictation::ShortcutMode::Toggle);
        // Settings that predate the preference keep the terminal paste.
        assert!(view.paste_with_shift);
        assert_eq!(view.sound_effect_volume, 0.25);

        // A second launch reads settings.json and leaves the backup alone.
        fs::write(directory.join("linux-settings.json"), b"{}").unwrap();
        crate::app_settings::migrate_legacy_settings(&path).unwrap();
        assert!(directory.join("linux-settings.json").is_file());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn the_linux_view_round_trips_through_app_settings() {
        let view = LinuxSettings {
            dictation_hotkey: LinuxHotkey {
                control: true,
                alt: true,
                shift: false,
                super_key: false,
                key: "space".into(),
            },
            double_tap_lock: false,
            shortcut_mode: crate::dictation::ShortcutMode::Toggle,
            paste_with_shift: true,
            sound_effect_volume: 0.75,
            ..LinuxSettings::default()
        };
        let mut settings = AppSettings::default();
        view.apply_to(&mut settings);
        assert_eq!(LinuxSettings::from_app(&settings), view);
        // Muting keeps the remembered volume for when sounds return.
        let muted = LinuxSettings {
            sound_effect_volume: 0.0,
            ..view
        };
        muted.apply_to(&mut settings);
        assert!(!settings.sound_effects);
        assert_eq!(settings.sound_effect_volume, 0.75);
        assert_eq!(LinuxSettings::from_app(&settings).sound_effect_volume, 0.0);
    }
}

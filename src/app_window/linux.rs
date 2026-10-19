//! Linux parts of the app window. Settings is its own process: it edits
//! `settings.json` and asks `hex.service`, which owns the listener, shortcut
//! registration, the HUD, and updates, to reload it over the service socket.

use super::*;
use crate::desktop_host::{DesktopAction, DesktopUpdateStatus};
use crate::linux_service::{Client, Request};
use crate::linux_settings::LinuxHotkey;

pub(super) const CAPABILITIES: Capabilities = Capabilities {
    meetings: false,
    commands: false,
    hud_lab: false,
    modes: false,
    voice_action: false,
    history: false,
    permissions: false,
    dock_icon: false,
    launch_at_login: false,
    listener_control: true,
    paste_last: false,
    modifier_sides: false,
    indicator_position: false,
    interface_language: false,
    software_updates: true,
    terminal_paste: true,
};

/// Linux records shortcuts from the focused window's key events.
pub(super) const KEY_EVENT_HOTKEY_CAPTURE: bool = true;

/// The service checks for updates on its own schedule.
pub(super) const MANUAL_UPDATE_CHECK: bool = false;

pub(super) struct Args;

pub(super) struct Platform {
    client: Client,
    /// What the window last showed of the service, to redraw only on change.
    shown: Option<(crate::desktop_host::DesktopSnapshot, Option<String>)>,
}

impl Platform {
    pub(super) fn new(
        _: Args,
        _: Option<&AppWindowPreview>,
        _: &AppSettings,
        _: &StatusSnapshot,
        _: &mut Context<AppWindow>,
    ) -> Self {
        Self {
            client: Client::new(),
            shown: None,
        }
    }
}

/// Opens the shared window in the Settings process.
pub fn open(event_path: PathBuf, cx: &mut App) -> gpui::Result<WindowHandle<AppWindow>> {
    open_new(event_path, None, None, Args, cx)
}

pub(super) fn window_options(options: WindowOptions) -> WindowOptions {
    WindowOptions {
        app_id: Some("hex".into()),
        ..options
    }
}

pub(super) fn frame(frame: Div) -> Div {
    frame
}

pub(super) fn sidebar(sidebar: Div) -> Div {
    sidebar
}

/// Empty sidebar space moves the window.
pub(super) fn window_drag_region(region: Div) -> Div {
    region.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
}

pub(super) fn caption(_: &mut Window) -> Option<AnyElement> {
    None
}

/// The service keeps its shortcut registered while Settings records one;
/// the chord it already owns reaches the service, not this window.
pub(super) fn set_hotkey_capture_active(_: bool) {}

pub(super) fn hotkey_modifiers(modifiers: GpuiModifiers) -> HotkeyModifiers {
    let held = |down: bool| down.then_some(ModifierSide::Either);
    HotkeyModifiers {
        control: held(modifiers.control),
        option: held(modifiers.alt),
        shift: held(modifiers.shift),
        command: held(modifiers.platform),
        function: false,
    }
}

pub(super) fn hotkey_key(key: &str) -> std::result::Result<HotkeyKey, &'static str> {
    if key.trim().is_empty() {
        Err("Unsupported key")
    } else {
        Ok(HotkeyKey::named(key))
    }
}

/// X11 and Wayland register a modifier chord with a key.
pub(super) fn validate_hotkey(
    _: HotkeyKind,
    binding: &HotkeyBinding,
) -> std::result::Result<(), &'static str> {
    LinuxHotkey::from(binding)
        .validate()
        .map_err(|_| "Add a key Linux can register")
}

impl AppWindow {
    pub(super) fn poll_platform(&mut self, cx: &mut Context<Self>) -> bool {
        let mut changed = false;
        // Settings prepares the model, saves it, and the service loads it
        // when it reloads settings.
        if !self.preview
            && let Some(result) = self.transcription_preparation.poll()
        {
            let result =
                result.and_then(|selection| self.apply_transcription_selection(selection, cx));
            if let Err(error) = result {
                self.transcription_preparation.set_error(Some(error));
            }
            self.poll_transcription_download();
            changed = true;
        }
        self.platform.client.refresh();
        let shown = self
            .platform
            .client
            .state
            .as_ref()
            .map(|state| (state.desktop.clone(), self.platform.client.error.clone()));
        if shown != self.platform.shown {
            self.platform.shown = shown;
            changed = true;
        }
        changed
    }

    pub(super) fn platform_opened(&mut self, _: Option<&AppWindowPreview>) {}

    pub(super) fn platform_refresh(&mut self) {}

    pub(super) fn platform_select_pane(&mut self, _: Pane) {}

    pub(super) fn render_platform_pane(
        &mut self,
        _: Pane,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<AnyElement> {
        None
    }

    pub(super) fn platform_settings_saved(&mut self) {
        if let Err(error) = self.platform.client.send(Request::ReloadSettings) {
            self.platform.client.error = Some(format!("{error:#}"));
        }
    }

    pub(super) fn platform_closed(&mut self) {}

    pub(super) fn platform_settings_notice(&mut self, _: &mut Context<Self>) -> Option<AnyElement> {
        None
    }

    pub(super) fn platform_settings_section(
        &mut self,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        Vec::new()
    }

    pub(super) fn platform_model_notice(
        &mut self,
        _: &mut Context<Self>,
    ) -> Option<(&'static str, &'static str, AnyElement)> {
        None
    }

    pub(super) fn platform_setup_permissions(
        &mut self,
        _: &mut Context<Self>,
    ) -> Vec<(&'static str, &'static str, AnyElement)> {
        Vec::new()
    }

    pub(super) fn platform_awaits_setup(&self) -> bool {
        false
    }

    pub(super) fn platform_setup_ready(&mut self) {}

    pub(super) fn platform_poll_setup(&mut self) -> bool {
        false
    }

    pub(super) fn personal_commands_changed(&mut self) {}

    pub(super) fn platform_update_button(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<Div>> {
        matches!(
            self.platform_software_update(),
            Some(SoftwareUpdate::ReadyToRestart { .. })
        )
        .then(|| {
            sidebar_update_button().on_click(cx.listener(|this, _, _, cx| {
                this.install_update(cx);
            }))
        })
    }

    pub(super) fn platform_capture_started(&mut self, _: &mut Context<Self>) {}

    pub(super) fn platform_listener(&self) -> Option<ListenerView> {
        let error = self.platform.client.error.clone();
        let Some(state) = &self.platform.client.state else {
            return Some(ListenerView {
                running: false,
                status: "Service disconnected".into(),
                last_failure: None,
                error,
            });
        };
        let desktop = &state.desktop;
        Some(ListenerView {
            running: desktop.listener.running,
            status: desktop.listener.status.clone(),
            last_failure: desktop.listener.warning.clone(),
            error: error.or_else(|| desktop.operation_error.clone()),
        })
    }

    pub(super) fn set_listening(&mut self, enabled: bool) {
        let action = if enabled {
            DesktopAction::StartListening
        } else {
            DesktopAction::StopListening
        };
        if let Err(error) = self.platform.client.send(Request::Desktop(action)) {
            self.platform.client.error = Some(format!("{error:#}"));
        }
    }

    /// Only service-managed installs update themselves.
    pub(super) fn platform_software_update(&self) -> Option<SoftwareUpdate> {
        let state = self.platform.client.state.as_ref()?;
        Some(match state.desktop.update_status {
            DesktopUpdateStatus::Unavailable => return None,
            DesktopUpdateStatus::Checking => SoftwareUpdate::Checking,
            DesktopUpdateStatus::Current => SoftwareUpdate::Current,
            DesktopUpdateStatus::Failed => SoftwareUpdate::Failed,
            DesktopUpdateStatus::ReadyToRestart => SoftwareUpdate::ReadyToRestart {
                version: "the new version".into(),
            },
        })
    }

    /// The service checks on its own schedule.
    pub(super) fn check_for_updates(&mut self) {}

    pub(super) fn install_update(&mut self, _: &mut Context<Self>) {
        if let Err(error) = self
            .platform
            .client
            .send(Request::Desktop(DesktopAction::RestartIntoUpdate))
        {
            self.platform.client.error = Some(format!("{error:#}"));
        }
    }
}

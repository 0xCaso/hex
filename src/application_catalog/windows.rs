//! Windows applications, named by executable stem ("chrome", "code") as the
//! Windows mode matcher compares them: apps registered under App Paths plus
//! every application that currently shows a window.

use std::path::{Path, PathBuf};

use color_eyre::eyre::{Result, eyre};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongPtrW, GetWindowTextLengthW,
    IsWindowVisible, WS_EX_TOOLWINDOW,
};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

use super::InstalledApplication;

const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";

pub(super) fn applications() -> Vec<InstalledApplication> {
    let own = std::env::current_exe().ok();
    let mut paths = registered_executables();
    paths.extend(windowed_executables());
    paths
        .into_iter()
        .filter(|path| own.as_deref() != Some(path.as_path()))
        .filter_map(|path| metadata(&path).ok())
        .collect()
}

pub(super) fn metadata(path: &Path) -> Result<InstalledApplication> {
    let is_executable = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"));
    if !is_executable {
        return Err(eyre!("select a Windows .exe application"));
    }
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .ok_or_else(|| eyre!("{} has no executable name", path.display()))?
        .to_lowercase();
    Ok(InstalledApplication {
        name,
        bundle_id: None,
        path: path.to_owned(),
        icon: None,
    })
}

/// Executables installers register for `Win+R` and `start`.
fn registered_executables() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        let Ok(app_paths) = RegKey::predef(hive).open_subkey_with_flags(APP_PATHS, KEY_READ) else {
            continue;
        };
        for name in app_paths.enum_keys().flatten() {
            let path = app_paths
                .open_subkey_with_flags(&name, KEY_READ)
                .and_then(|key| key.get_value::<String, _>(""))
                .map(|value| PathBuf::from(value.trim().trim_matches('"')))
                .unwrap_or_else(|_| PathBuf::from(&name));
            paths.push(path);
        }
    }
    paths
}

/// Executables behind visible, titled, unowned top-level windows: the
/// applications in the taskbar right now.
fn windowed_executables() -> Vec<PathBuf> {
    unsafe extern "system" fn collect(window: HWND, windows: LPARAM) -> i32 {
        let windows = unsafe { &mut *(windows as *mut Vec<HWND>) };
        windows.push(window);
        1
    }
    let mut windows: Vec<HWND> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), (&raw mut windows) as LPARAM);
    }
    windows
        .into_iter()
        .filter(|&window| unsafe {
            IsWindowVisible(window) != 0
                && GetWindowTextLengthW(window) > 0
                && GetWindow(window, GW_OWNER).is_null()
                && GetWindowLongPtrW(window, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW == 0
        })
        .filter_map(crate::windows_input::window_process_path)
        .collect()
}

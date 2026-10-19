//! Windows applications, named by executable stem ("chrome", "code") as the
//! Windows mode matcher compares them: Start menu shortcuts, apps registered
//! under App Paths, and every application that currently shows a window,
//! each with its executable's icon.

use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use color_eyre::eyre::{Result, eyre};
use gpui::{Image, ImageFormat};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetIconInfo, GetWindow, GetWindowLongPtrW,
    GetWindowTextLengthW, HICON, ICONINFO, IsWindowVisible, PrivateExtractIconsW, WS_EX_TOOLWINDOW,
};
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};

use super::InstalledApplication;

const APP_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths";
const ICON_SIZE: i32 = 48;

pub(super) fn applications() -> Vec<InstalledApplication> {
    let own = std::env::current_exe().ok();
    let mut paths = start_menu_executables();
    paths.extend(registered_executables());
    paths.extend(windowed_executables());
    // One icon read per executable name; the catalog keeps one entry each.
    let mut names = HashSet::new();
    paths
        .into_iter()
        .filter(|path| own.as_deref() != Some(path.as_path()))
        .filter(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| names.insert(stem.to_lowercase()))
        })
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
        icon: executable_icon(path),
    })
}

/// Executables behind the Start menu's shortcuts, for every user and the
/// current one. Uninstallers and helpers that ship shortcuts are skipped.
fn start_menu_executables() -> Vec<PathBuf> {
    let roots = [
        std::env::var_os("ProgramData").map(PathBuf::from),
        std::env::var_os("APPDATA").map(PathBuf::from),
    ];
    let mut shortcuts = Vec::new();
    for root in roots.into_iter().flatten() {
        collect_shortcuts(
            &root.join(r"Microsoft\Windows\Start Menu\Programs"),
            &mut shortcuts,
            0,
        );
    }
    shortcuts
        .iter()
        .filter_map(|shortcut| std::fs::read(shortcut).ok())
        .filter_map(|bytes| shortcut_target(&bytes))
        .map(PathBuf::from)
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        })
        .filter(|path| {
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            !stem.contains("uninst") && !stem.starts_with("unins")
        })
        .filter(|path| path.is_file())
        .collect()
}

fn collect_shortcuts(directory: &Path, shortcuts: &mut Vec<PathBuf>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && depth < 4 {
            collect_shortcuts(&path, shortcuts, depth + 1);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
        {
            shortcuts.push(path);
        }
    }
}

/// The local target path of a Shell Link (`.lnk`) file, read from its
/// LinkInfo structure ([MS-SHLLINK] 2.3). Shortcuts without a local path,
/// such as advertised installer shortcuts, have none.
fn shortcut_target(bytes: &[u8]) -> Option<String> {
    const HAS_LINK_TARGET_ID_LIST: u32 = 0x1;
    const HAS_LINK_INFO: u32 = 0x2;
    const VOLUME_ID_AND_LOCAL_BASE_PATH: u32 = 0x1;
    let u16_at = |offset: usize| -> Option<usize> {
        Some(u16::from_le_bytes(bytes.get(offset..offset + 2)?.try_into().ok()?) as usize)
    };
    let u32_at = |offset: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    if u32_at(0)? != 0x4C {
        return None;
    }
    let flags = u32_at(0x14)?;
    if flags & HAS_LINK_INFO == 0 {
        return None;
    }
    let mut offset = 0x4C;
    if flags & HAS_LINK_TARGET_ID_LIST != 0 {
        offset += 2 + u16_at(offset)?;
    }
    let info = offset;
    let header_size = u32_at(info + 4)? as usize;
    if u32_at(info + 8)? & VOLUME_ID_AND_LOCAL_BASE_PATH == 0 {
        return None;
    }
    if header_size >= 0x24 {
        let start = info + u32_at(info + 0x1C)? as usize;
        let units = bytes
            .get(start..)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .take_while(|unit| *unit != 0)
            .collect::<Vec<_>>();
        return String::from_utf16(&units).ok();
    }
    let start = info + u32_at(info + 0x10)? as usize;
    let path = bytes.get(start..)?.split(|byte| *byte == 0).next()?;
    Some(String::from_utf8_lossy(path).into_owned())
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

/// The executable's first icon at 48 pixels, as PNG.
fn executable_icon(path: &Path) -> Option<Arc<Image>> {
    let wide = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // PrivateExtractIconsW reads a fixed MAX_PATH buffer.
    if wide.len() > 260 {
        return None;
    }
    let mut buffer = [0_u16; 260];
    buffer[..wide.len()].copy_from_slice(&wide);
    let mut icon: HICON = std::ptr::null_mut();
    let extracted = unsafe {
        PrivateExtractIconsW(
            buffer.as_ptr(),
            0,
            ICON_SIZE,
            ICON_SIZE,
            &mut icon,
            std::ptr::null_mut(),
            1,
            0,
        )
    };
    if extracted == 0 || extracted == u32::MAX || icon.is_null() {
        return None;
    }
    let rgba = unsafe { icon_rgba(icon) };
    unsafe { DestroyIcon(icon) };
    let (width, height, pixels) = rgba?;
    let image = image::RgbaImage::from_raw(width, height, pixels)?;
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut png, image::ImageFormat::Png)
        .ok()?;
    Some(Arc::new(Image::from_bytes(
        ImageFormat::Png,
        png.into_inner(),
    )))
}

/// The icon's color bitmap as straight RGBA. Icons without an alpha channel
/// are drawn opaque.
unsafe fn icon_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut info: ICONINFO = unsafe { std::mem::zeroed() };
    if unsafe { GetIconInfo(icon, &mut info) } == 0 {
        return None;
    }
    let result = (|| {
        if info.hbmColor.is_null() {
            return None;
        }
        let mut bitmap: BITMAP = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<BITMAP>() as i32;
        if unsafe { GetObjectW(info.hbmColor, size, (&raw mut bitmap).cast()) } == 0 {
            return None;
        }
        let (width, height) = (bitmap.bmWidth, bitmap.bmHeight);
        if width <= 0 || height <= 0 {
            return None;
        }
        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negative height reads the rows top-down.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..unsafe { std::mem::zeroed() }
            },
            ..unsafe { std::mem::zeroed() }
        };
        let mut pixels = vec![0_u8; width as usize * height as usize * 4];
        let context = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
        let rows = unsafe {
            GetDIBits(
                context,
                info.hbmColor,
                0,
                height as u32,
                pixels.as_mut_ptr().cast(),
                &mut header,
                DIB_RGB_COLORS,
            )
        };
        unsafe { DeleteDC(context) };
        if rows == 0 {
            return None;
        }
        let opaque = pixels.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 0);
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
            if opaque {
                pixel[3] = 255;
            }
        }
        Some((width as u32, height as u32, pixels))
    })();
    unsafe {
        if !info.hbmColor.is_null() {
            DeleteObject(info.hbmColor);
        }
        if !info.hbmMask.is_null() {
            DeleteObject(info.hbmMask);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_targets_come_from_the_link_info_local_path() {
        // A minimal Shell Link: header, no ID list, LinkInfo with an ANSI
        // local base path.
        let mut link = vec![0_u8; 0x4C];
        link[0..4].copy_from_slice(&0x4C_u32.to_le_bytes());
        link[0x14..0x18].copy_from_slice(&0x2_u32.to_le_bytes());
        let path = b"C:\\Program Files\\App\\app.exe\0";
        let header_size = 0x1C_u32;
        let mut info = Vec::new();
        info.extend_from_slice(&(header_size + path.len() as u32).to_le_bytes());
        info.extend_from_slice(&header_size.to_le_bytes());
        info.extend_from_slice(&1_u32.to_le_bytes());
        info.extend_from_slice(&0_u32.to_le_bytes());
        info.extend_from_slice(&header_size.to_le_bytes());
        info.extend_from_slice(&0_u32.to_le_bytes());
        info.extend_from_slice(&0_u32.to_le_bytes());
        info.extend_from_slice(path);
        link.extend_from_slice(&info);
        assert_eq!(
            shortcut_target(&link).as_deref(),
            Some(r"C:\Program Files\App\app.exe")
        );
        link[0x14..0x18].copy_from_slice(&0_u32.to_le_bytes());
        assert_eq!(shortcut_target(&link), None, "no LinkInfo, no target");
    }

    #[test]
    fn executables_carry_their_icon() {
        let notepad = PathBuf::from(r"C:\Windows\System32\notepad.exe");
        if notepad.is_file() {
            let application = metadata(&notepad).unwrap();
            assert_eq!(application.name, "notepad");
            assert!(application.icon.is_some());
        }
    }
}

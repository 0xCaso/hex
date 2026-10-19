//! Linux applications from their `.desktop` entries, named by executable
//! ("code", "firefox") as the Linux mode matcher sees the frontmost
//! process, with icons from the icon theme.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use color_eyre::eyre::{Result, eyre};
use gpui::{Image, ImageFormat};

use super::InstalledApplication;

const ICON_SIZES: &[&str] = &[
    "256x256", "128x128", "96x96", "64x64", "48x48", "scalable", "32x32",
];

pub(super) fn applications() -> Vec<InstalledApplication> {
    let own = std::env::current_exe()
        .ok()
        .and_then(|path| executable_name(&path.to_string_lossy()));
    data_directories()
        .iter()
        .flat_map(|directory| desktop_files(&directory.join("applications")))
        .filter_map(|path| desktop_application(&path))
        .filter(|application| Some(&application.name) != own.as_ref())
        .collect()
}

pub(super) fn metadata(path: &Path) -> Result<InstalledApplication> {
    if path
        .extension()
        .is_some_and(|extension| extension == "desktop")
    {
        return desktop_application(path)
            .ok_or_else(|| eyre!("{} does not launch an application", path.display()));
    }
    let name = executable_name(&path.to_string_lossy())
        .ok_or_else(|| eyre!("{} has no executable name", path.display()))?;
    Ok(InstalledApplication {
        name,
        bundle_id: None,
        path: path.to_owned(),
        icon: None,
    })
}

/// `$XDG_DATA_HOME` then `$XDG_DATA_DIRS`, with the specification's
/// defaults.
fn data_directories() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    home.into_iter()
        .chain(
            system
                .split(':')
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
        )
        .collect()
}

fn desktop_files(directory: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(desktop_files(&path));
        } else if path
            .extension()
            .is_some_and(|extension| extension == "desktop")
        {
            files.push(path);
        }
    }
    files
}

#[derive(Default)]
struct DesktopEntry {
    exec: Option<String>,
    icon: Option<String>,
    hidden: bool,
    application: bool,
}

/// The `[Desktop Entry]` group's keys this catalog reads.
fn parse_desktop_entry(text: &str) -> DesktopEntry {
    let mut entry = DesktopEntry::default();
    let mut in_entry = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_entry) else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Type" => entry.application = value == "Application",
            "Exec" => entry.exec = Some(value.into()),
            "Icon" => entry.icon = Some(value.into()),
            "NoDisplay" | "Hidden" => entry.hidden |= value == "true",
            _ => {}
        }
    }
    entry
}

fn desktop_application(path: &Path) -> Option<InstalledApplication> {
    let entry = parse_desktop_entry(&fs::read_to_string(path).ok()?);
    if !entry.application || entry.hidden {
        return None;
    }
    let name = executable_name(entry.exec.as_deref()?)?;
    Some(InstalledApplication {
        name,
        bundle_id: None,
        path: path.to_owned(),
        icon: entry.icon.as_deref().and_then(icon),
    })
}

/// The process name an `Exec` line starts: the program's file name, past
/// `env` and its assignments. Flatpak apps run as their app ID's last
/// segment (`org.mozilla.firefox` runs `firefox`).
fn executable_name(exec: &str) -> Option<String> {
    let mut words = exec
        .split_whitespace()
        .map(|word| word.trim_matches(|c| c == '"' || c == '\''))
        .skip_while(|word| *word == "env" || (word.contains('=') && !word.starts_with('/')));
    let program = words.next()?;
    let name = Path::new(program).file_name()?.to_str()?;
    if name == "flatpak" {
        let id = words.find(|word| !word.starts_with('-') && *word != "run")?;
        return id.rsplit('.').next().map(str::to_lowercase);
    }
    Some(name.to_lowercase()).filter(|name| !name.is_empty() && !name.starts_with('%'))
}

/// An icon by absolute path or theme name, from hicolor or pixmaps.
fn icon(name: &str) -> Option<Arc<Image>> {
    let path = Path::new(name);
    if path.is_absolute() {
        return image(path);
    }
    let directories = data_directories();
    let themed = directories.iter().flat_map(|directory| {
        ICON_SIZES.iter().flat_map(move |size| {
            ["png", "svg"].map(|extension| {
                directory.join(format!("icons/hicolor/{size}/apps/{name}.{extension}"))
            })
        })
    });
    let pixmaps = directories.iter().flat_map(|directory| {
        ["png", "svg"].map(|extension| directory.join(format!("pixmaps/{name}.{extension}")))
    });
    themed.chain(pixmaps).find_map(|path| image(&path))
}

fn image(path: &Path) -> Option<Arc<Image>> {
    let format = match path.extension()?.to_str()? {
        "png" => ImageFormat::Png,
        "svg" => ImageFormat::Svg,
        _ => return None,
    };
    let bytes = fs::read(path).ok()?;
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_lines_reduce_to_the_process_name() {
        assert_eq!(
            executable_name("/usr/bin/code --new-window %F").as_deref(),
            Some("code")
        );
        assert_eq!(
            executable_name("env GDK_BACKEND=x11 firefox %u").as_deref(),
            Some("firefox")
        );
        assert_eq!(
            executable_name("\"/opt/Slack/slack\" -s %U").as_deref(),
            Some("slack")
        );
        assert_eq!(
            executable_name(
                "/usr/bin/flatpak run --branch=stable --arch=x86_64 org.mozilla.firefox @@u %u @@"
            )
            .as_deref(),
            Some("firefox")
        );
    }

    #[test]
    fn hidden_and_non_application_entries_are_skipped() {
        let entry = parse_desktop_entry(
            "[Desktop Entry]\nType=Application\nExec=code\nIcon=vscode\n\n[Desktop Action new]\nExec=other\n",
        );
        assert!(entry.application && !entry.hidden);
        assert_eq!(entry.exec.as_deref(), Some("code"));
        assert_eq!(entry.icon.as_deref(), Some("vscode"));
        assert!(parse_desktop_entry("[Desktop Entry]\nType=Application\nNoDisplay=true").hidden);
        assert!(!parse_desktop_entry("[Desktop Entry]\nType=Link").application);
    }
}

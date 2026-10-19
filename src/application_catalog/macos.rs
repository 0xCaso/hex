//! macOS applications: `.app` bundles named as Finder shows them, with
//! their bundle identifiers and icons.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::Arc;

use color_eyre::eyre::{Result, eyre};
use gpui::{Image, ImageFormat};
use objc2::AnyThread;
use objc2::rc::autoreleasepool;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSCompositingOperation,
    NSDeviceRGBColorSpace, NSGraphicsContext, NSImage, NSImageInterpolation, NSWorkspace,
};
use objc2_foundation::{NSBundle, NSDictionary, NSFileManager, NSPoint, NSRect, NSSize, NSString};

use super::InstalledApplication;

pub(super) fn applications() -> Vec<InstalledApplication> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for root in application_roots() {
        collect_applications(&root, &mut paths, &mut seen);
    }
    paths
        .into_iter()
        .filter_map(|path| metadata(&path).ok())
        .collect()
}

pub(super) fn metadata(path: &Path) -> Result<InstalledApplication> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("app") {
        return Err(eyre!("select a macOS .app bundle"));
    }
    autoreleasepool(|_| {
        let path_string = NSString::from_str(&path.to_string_lossy());
        let bundle = NSBundle::bundleWithPath(&path_string)
            .ok_or_else(|| eyre!("{} is not a readable application bundle", path.display()))?;
        let display_name = NSFileManager::defaultManager()
            .displayNameAtPath(&path_string)
            .to_string();
        let name = crate::context::strip_bundle_extension(&display_name).to_owned();
        let bundle_id = bundle
            .bundleIdentifier()
            .map(|identifier| identifier.to_string());
        let native_icon = NSWorkspace::sharedWorkspace().iconForFile(&path_string);
        let icon = rasterize_icon(&native_icon).map(Arc::new);
        Ok(InstalledApplication {
            name,
            bundle_id,
            path: path.to_owned(),
            icon,
        })
    })
}

fn application_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Library/CoreServices/Applications"),
    ];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    roots
}

fn collect_applications(directory: &Path, output: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir()
            || file_type.is_symlink()
            || path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'))
        {
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) == Some("app") {
            let identity = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            if seen.insert(identity) {
                output.push(path);
            }
        } else {
            collect_applications(&path, output, seen);
        }
    }
}

fn rasterize_icon(icon: &NSImage) -> Option<Image> {
    const ICON_SIZE: isize = 128;
    let bitmap = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            ptr::null_mut(),
            ICON_SIZE,
            ICON_SIZE,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            0,
            0,
        )?
    };
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)?;
    let previous = NSGraphicsContext::currentContext();
    NSGraphicsContext::setCurrentContext(Some(&context));
    context.setImageInterpolation(NSImageInterpolation::High);
    icon.drawInRect_fromRect_operation_fraction(
        NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(ICON_SIZE as f64, ICON_SIZE as f64),
        ),
        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
        NSCompositingOperation::Copy,
        1.0,
    );
    NSGraphicsContext::setCurrentContext(previous.as_deref());
    let properties = NSDictionary::<NSBitmapImageRepPropertyKey, AnyObject>::new();
    let png = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)?
    };
    Some(Image::from_bytes(ImageFormat::Png, png.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finder_has_native_picker_metadata_and_an_icon() {
        let finder = metadata(Path::new("/System/Library/CoreServices/Finder.app")).unwrap();

        assert_eq!(finder.name, "Finder");
        assert_eq!(finder.bundle_id.as_deref(), Some("com.apple.finder"));
        assert!(finder.icon.is_some());
    }
}

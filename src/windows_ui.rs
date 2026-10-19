//! Windows window chrome on the shared design system: Segoe Fluent Icons
//! glyphs, the custom caption bar drawn into gpui's frameless titlebar area,
//! the window and tray icons, and the HUD popup's DWM attributes.

use std::sync::{Arc, OnceLock};

use gpui::{
    AnyElement, Bounds, Div, ElementId, FontWeight, Image, ImageFormat, Pixels, Stateful, Window,
    WindowControlArea, div, img, prelude::*, px, rgb,
};
use windows_sys::Win32::Foundation::{HINSTANCE, HWND};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWLP_HINSTANCE, GetSystemMetrics, GetWindowLongPtrW, ICON_BIG, ICON_SMALL, IMAGE_ICON,
    LR_DEFAULTCOLOR, LoadImageW, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, SendMessageW,
    WM_SETICON,
};
use winreg::RegKey;
use winreg::enums::HKEY_LOCAL_MACHINE;

use crate::design_tokens::{
    CAPTION, FOOTNOTE, MENU_ITEM_HEIGHT, MENU_OFFSET, SURFACE_HOVER, SURFACE_SELECTED,
    TEXT_PRIMARY, TEXT_SECONDARY, Typography,
};
use crate::desktop_ui::{menu, menu_item, truncated_line};

pub(crate) const FONT_UI: &str = "Segoe UI Variable Text";

/// Segoe Fluent Icons ships with Windows 11; Windows 10 carries the same
/// glyphs we use in Segoe MDL2 Assets.
pub(crate) fn icon_font() -> &'static str {
    static FONT: OnceLock<&'static str> = OnceLock::new();
    FONT.get_or_init(|| {
        let build: u32 = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
            .and_then(|key| key.get_value::<String, _>("CurrentBuild"))
            .ok()
            .and_then(|build| build.parse().ok())
            .unwrap_or(22000);
        if build >= 22000 {
            "Segoe Fluent Icons"
        } else {
            "Segoe MDL2 Assets"
        }
    })
}

pub(crate) const CAPTION_HEIGHT: f32 = 40.0;
const CAPTION_BUTTON_WIDTH: f32 = 46.0;

// The system caption close button's hover and pressed reds.
const CLOSE_HOVER: u32 = 0xc42b1c;
const CLOSE_PRESSED: u32 = 0xb2271a;

/// Gives the main window the executable's embedded icon (resource 1, see
/// build.rs) for the taskbar, Alt-Tab, and the caption, so they match the
/// exe in Explorer and Task Manager. GPUI registers its window class without
/// one. Windows picks the frame that fits each size and DPI.
pub(crate) fn apply_window_icon(hwnd: Option<HWND>) {
    let Some(hwnd) = hwnd else {
        return;
    };
    const APP_ICON: u16 = 1;
    unsafe {
        let module = GetWindowLongPtrW(hwnd, GWLP_HINSTANCE) as HINSTANCE;
        for (kind, width, height) in [
            (ICON_SMALL, SM_CXSMICON, SM_CYSMICON),
            (ICON_BIG, SM_CXICON, SM_CYICON),
        ] {
            let icon = LoadImageW(
                module,
                APP_ICON as usize as *const u16,
                IMAGE_ICON,
                GetSystemMetrics(width),
                GetSystemMetrics(height),
                LR_DEFAULTCOLOR,
            );
            if !icon.is_null() {
                SendMessageW(hwnd, WM_SETICON, kind as usize, icon as isize);
            }
        }
    }
}

/// GPUI's Transparent window background enables the legacy
/// TRANSPARENTGRADIENT accent policy, which paints a faint veil over the
/// whole window rectangle. DirectComposition already provides per-pixel
/// alpha, so switch the policy back to disabled for the HUD.
/// Windows 11 decorates top-level windows with rounded corners, a hairline
/// border, and a drop shadow; on the mostly transparent HUD popup they all
/// read as a ghost box floating around the capsule, so turn them off.
pub(crate) fn strip_popup_chrome(hwnd: HWND) {
    use windows_sys::Win32::Graphics::Dwm::{
        DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_NCRENDERING_POLICY,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DwmSetWindowAttribute,
    };
    // DWMWA_COLOR_NONE: no border at all (not merely transparent).
    const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;
    unsafe {
        let corner = DWMWCP_DONOTROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&raw const corner).cast(),
            size_of_val(&corner) as u32,
        );
        let color = DWMWA_COLOR_NONE;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR as u32,
            (&raw const color).cast(),
            size_of_val(&color) as u32,
        );
        // The shadow is non-client rendering; verified live that disabling
        // it removes the ghost rectangle behind the transparent HUD.
        let policy = DWMNCRP_DISABLED;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY as u32,
            (&raw const policy).cast(),
            size_of_val(&policy) as u32,
        );
    }
}

pub(crate) fn disable_window_tint(hwnd: HWND) {
    #[repr(C)]
    struct AccentPolicy {
        state: u32,
        flags: u32,
        gradient: u32,
        animation_id: u32,
    }
    #[repr(C)]
    struct CompositionData {
        attribute: u32,
        data: *mut core::ffi::c_void,
        size: usize,
    }
    type SetWindowCompositionAttribute =
        unsafe extern "system" fn(HWND, *mut CompositionData) -> i32;
    unsafe {
        let Ok(user32) = libloading::Library::new("user32.dll") else {
            return;
        };
        let Ok(set_attribute) =
            user32.get::<SetWindowCompositionAttribute>(b"SetWindowCompositionAttribute\0")
        else {
            return;
        };
        let mut policy = AccentPolicy {
            state: 0,
            flags: 0,
            gradient: 0,
            animation_id: 0,
        };
        let mut data = CompositionData {
            attribute: 19,
            data: (&raw mut policy).cast(),
            size: size_of::<AccentPolicy>(),
        };
        set_attribute(hwnd, &mut data);
    }
}

/// The 32×32 system-tray icon: the app icon pre-rendered to raw RGBA at
/// build time from app/AppIcon.icon/Assets/Image.png (full color, so both
/// taskbar themes use the same pixels).
pub(crate) const TRAY_ICON_SIZE: u32 = 32;

pub(crate) fn tray_icon_rgba() -> Vec<u8> {
    include_bytes!("../resources/windows/tray-32.rgba").to_vec()
}

/// One Segoe Fluent Icons glyph, tinted and sized like text.
pub(crate) fn fluent_icon(glyph: &'static str, size: f32, color: u32) -> AnyElement {
    div()
        .flex_none()
        .font_family(icon_font())
        .text_size(px(size))
        .text_color(rgb(color))
        .child(glyph)
        .into_any_element()
}

/// The caption row is split to match the columns below it, so the sidebar
/// runs the full window height as it does on macOS. Both halves are OS drag
/// regions (double-click maximizes), and gpui routes the window buttons
/// through WM_NCHITTEST, so hover states, Snap Layouts on the maximize
/// button, and click behavior all come from Windows itself.
///
/// This is the sidebar half: the brand, aligned with the navigation icons.
pub(crate) fn caption_brand() -> impl IntoElement {
    caption_row().child(
        div()
            .id("caption-brand")
            .window_control_area(WindowControlArea::Drag)
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap_2()
            .pl_3()
            .child(brand_logo())
            .child(
                div()
                    .text_step(CAPTION)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(TEXT_SECONDARY))
                    .child("Hex"),
            ),
    )
}

/// The content half of the caption row: a drag region and the window
/// buttons at the far right.
pub(crate) fn caption_controls(window: &Window) -> impl IntoElement {
    let maximize_glyph = if window.is_maximized() {
        "\u{E923}"
    } else {
        "\u{E922}"
    };
    caption_row()
        .child(
            div()
                .id("caption-drag")
                .window_control_area(WindowControlArea::Drag)
                .h_full()
                .flex_1()
                .min_w(px(0.0)),
        )
        .child(caption_button(
            "caption-minimize",
            WindowControlArea::Min,
            "\u{E921}",
        ))
        .child(caption_button(
            "caption-maximize",
            WindowControlArea::Max,
            maximize_glyph,
        ))
        .child(
            div()
                .id("caption-close")
                .window_control_area(WindowControlArea::Close)
                .w(px(CAPTION_BUTTON_WIDTH))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .font_family(icon_font())
                .text_size(px(10.0))
                .text_color(rgb(TEXT_SECONDARY))
                .hover(|button| button.bg(rgb(CLOSE_HOVER)).text_color(rgb(TEXT_PRIMARY)))
                .active(|button| button.bg(rgb(CLOSE_PRESSED)).text_color(rgb(TEXT_PRIMARY)))
                .child("\u{E8BB}"),
        )
}

/// The caption row blocks the window's own mouse handling beneath it (such as
/// focus-on-click), which would otherwise claim the press before Windows can
/// start a drag, snap, or caption-button click.
fn caption_row() -> Div {
    div()
        .occlude()
        .w_full()
        .h(px(CAPTION_HEIGHT))
        // Leave a thin non-interactive strip at the very top so the OS
        // top-edge resize hit test stays reachable above the drag and
        // caption-button hitboxes.
        .pt(px(3.0))
        .flex_none()
        .flex()
        .items_center()
}

/// The branded logo at 16 points, rendered from the 64px raster for crisp
/// results on high-DPI displays.
fn brand_logo() -> AnyElement {
    static LOGO: OnceLock<Arc<Image>> = OnceLock::new();
    let image = LOGO.get_or_init(|| {
        Arc::new(Image::from_bytes(
            ImageFormat::Png,
            include_bytes!("../resources/windows/icon-64.png").to_vec(),
        ))
    });
    img(image.clone()).size(px(16.0)).into_any_element()
}

fn caption_button(
    id: &'static str,
    area: WindowControlArea,
    glyph: &'static str,
) -> impl IntoElement {
    div()
        .id(id)
        .window_control_area(area)
        .w(px(CAPTION_BUTTON_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .font_family(icon_font())
        .text_size(px(10.0))
        .text_color(rgb(TEXT_SECONDARY))
        .hover(|button| button.bg(rgb(SURFACE_HOVER)).text_color(rgb(TEXT_PRIMARY)))
        .active(|button| button.bg(rgb(SURFACE_SELECTED)))
        .child(glyph)
}

// Anchored dropdowns. Windows dropdowns open as window-level overlays so a
// long list can flip above its trigger and scroll inside the window. Callers
// capture the trigger's bounds with a zero-size canvas; the panel and rows
// are the shared `menu` and `menu_item` components.

const DROPDOWN_MARGIN: f32 = 8.0;
const DROPDOWN_MAX_HEIGHT: f32 = 360.0;
/// `menu`'s `p_1` inset plus its border, on each side.
const DROPDOWN_PADDING: f32 = 5.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DropdownLayout {
    height: Pixels,
    top: Pixels,
}

fn dropdown_rows_height(rows: usize) -> Pixels {
    px(DROPDOWN_PADDING * 2.0) + px(MENU_ITEM_HEIGHT) * rows
}

/// The tallest whole-row height that fits `available`, so a clamped panel
/// never slices a row in half at its scroll edge.
fn dropdown_fit_height(row_count: usize, available: Pixels) -> Pixels {
    let content = dropdown_rows_height(row_count);
    if content <= available {
        return content;
    }
    let usable = f32::from(available) - DROPDOWN_PADDING * 2.0;
    let rows = (usable / MENU_ITEM_HEIGHT).floor().max(1.0) as usize;
    dropdown_rows_height(rows.min(row_count))
}

fn dropdown_layout(
    anchor_top: Pixels,
    anchor_bottom: Pixels,
    viewport_height: Pixels,
    row_count: usize,
) -> DropdownLayout {
    let row_count = row_count.max(1);
    let max_height = px(DROPDOWN_MAX_HEIGHT);
    let gap = px(MENU_OFFSET);
    let margin = px(DROPDOWN_MARGIN);
    let downward_top = anchor_bottom + gap;
    let below = (viewport_height - margin - downward_top).max(Pixels::ZERO);
    // Keep the panel below the caption bar so the window stays draggable.
    let above = (anchor_top - gap - px(CAPTION_HEIGHT) - margin).max(Pixels::ZERO);
    if below >= dropdown_fit_height(row_count, max_height) || below >= above {
        DropdownLayout {
            height: dropdown_fit_height(row_count, below.min(max_height)),
            top: downward_top,
        }
    } else {
        let height = dropdown_fit_height(row_count, above.min(max_height));
        DropdownLayout {
            height,
            top: anchor_top - gap - height,
        }
    }
}

/// The full-window layer behind an open dropdown; clicking it dismisses.
pub(crate) fn dropdown_backdrop(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .occlude()
}

pub(crate) fn dropdown_panel(
    bounds: Bounds<Pixels>,
    viewport_height: Pixels,
    row_count: usize,
) -> Div {
    dropdown_panel_with_width(bounds, viewport_height, row_count, bounds.size.width)
}

/// A dropdown wider than its trigger; the extra width grows leftward so
/// right-edge triggers keep the panel inside the window.
pub(crate) fn dropdown_panel_with_width(
    bounds: Bounds<Pixels>,
    viewport_height: Pixels,
    row_count: usize,
    width: Pixels,
) -> Div {
    let width = width.max(bounds.size.width);
    let layout = dropdown_layout(bounds.top(), bounds.bottom(), viewport_height, row_count);
    menu()
        .absolute()
        .left(bounds.right() - width)
        .top(layout.top)
        .w(width)
        .h(layout.height)
}

pub(crate) fn dropdown_item(
    id: impl Into<ElementId>,
    label: String,
    selected: bool,
) -> Stateful<Div> {
    menu_item(selected)
        .id(id)
        .child(truncated_line(FOOTNOTE, label).flex_1())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_dropdown_flips_up_and_stays_inside_the_viewport() {
        let viewport_height = px(700.0);
        let layout = dropdown_layout(px(500.0), px(530.0), viewport_height, 20);

        assert!(layout.top < px(500.0));
        assert_eq!(
            layout.height,
            dropdown_fit_height(20, px(DROPDOWN_MAX_HEIGHT))
        );
        assert!(layout.height <= px(DROPDOWN_MAX_HEIGHT));
        let rows = f32::from(layout.height) - DROPDOWN_PADDING * 2.0;
        assert_eq!(rows % MENU_ITEM_HEIGHT, 0.0);
        assert!(layout.top >= px(CAPTION_HEIGHT) + px(DROPDOWN_MARGIN));
        assert!(layout.top + layout.height <= viewport_height - px(DROPDOWN_MARGIN));
    }

    #[test]
    fn short_dropdown_opens_below_its_trigger_at_content_height() {
        let layout = dropdown_layout(px(100.0), px(130.0), px(700.0), 3);

        assert_eq!(layout.top, px(130.0) + px(MENU_OFFSET));
        assert_eq!(layout.height, dropdown_rows_height(3));
    }
}

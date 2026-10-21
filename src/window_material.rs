//! Real AppKit materials behind GPUI windows.
//!
//! GPUI paints into a non-opaque `CAMetalLayer` inside the window's content
//! view. A material view added to that content view *below* GPUI's view
//! shows through wherever GPUI paints nothing, and WindowServer composites
//! it off-process, without a GPUI fork change.
//!
//! On macOS 26 and later the sidebar column and the capsules are Liquid
//! Glass (`NSGlassEffectView`). Glass samples whatever lies beneath it, so
//! nothing opaque may sit under the sidebar inside the window. Glass stays translucent
//! while the window is inactive and turns opaque under Reduce Transparency.
//! Earlier releases fall back to `NSVisualEffectView` with the `.sidebar` and
//! HUD materials; the `.sidebar` material looks flat grey on macOS 26+
//! because the system now reserves its translucency for glass. GPUI's own `WindowBackgroundAppearance::Blurred` is not used:
//! it strips the material's tint and saturation, and on macOS 27 its
//! `Selection` material no longer blurs at all.

use gpui::Window;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSGlassEffectView, NSToolbar, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowOrderingMode,
    NSWindowTitleVisibility, NSWindowToolbarStyle,
};
use objc2_foundation::{NSNumber, NSPoint, NSRect, NSSize, ns_string};
use objc2_quartz_core::{
    CABasicAnimation, CALayer, CAMediaTiming, CASpringAnimation, CATransaction,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// GPUI's view and the window content view it sits in.
fn content_views(window: &Window) -> Option<(Retained<NSView>, Retained<NSView>)> {
    MainThreadMarker::new()?;
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: GPUI hands out its live content NSView; we only retain it.
    let gpui_view: Retained<NSView> =
        unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>())? };
    let content = unsafe { gpui_view.superview()? };
    Some((gpui_view, content))
}

fn force_dark(ns_window: &objc2_app_kit::NSWindow) {
    let appearance = objc2_app_kit::NSAppearance::appearanceNamed(unsafe {
        objc2_app_kit::NSAppearanceNameDarkAqua
    });
    objc2_app_kit::NSAppearanceCustomization::setAppearance(ns_window, appearance.as_deref());
}

/// Whether this macOS has Liquid Glass. Checked once at runtime, since the
/// class is missing before macOS 26 and allocating it there would crash.
pub(crate) fn has_glass() -> bool {
    static GLASS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *GLASS.get_or_init(|| objc2::runtime::AnyClass::get(c"NSGlassEffectView").is_some())
}

fn glass_view(mtm: MainThreadMarker, frame: NSRect, radius: f64) -> Retained<NSGlassEffectView> {
    let glass = NSGlassEffectView::initWithFrame(mtm.alloc(), frame);
    glass.setCornerRadius(radius);
    glass
}

fn effect_view(
    mtm: MainThreadMarker,
    frame: NSRect,
    material: NSVisualEffectMaterial,
    state: NSVisualEffectState,
) -> Retained<NSVisualEffectView> {
    let view = NSVisualEffectView::initWithFrame(mtm.alloc(), frame);
    view.setMaterial(material);
    view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    view.setState(state);
    view
}

/// Gives the main window its native chrome: the `.sidebar` material under
/// the first `sidebar_width` points, and an empty unified toolbar so AppKit
/// itself centres the traffic lights in the 52 pt toolbar row (and keeps
/// their hit area, fullscreen behaviour, and per-OS metrics correct).
pub(crate) fn install_sidebar_material(window: &Window, sidebar_width: f32) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some((_, content)) = content_views(window) else {
        return;
    };
    // The palette is dark-only for now, so the materials match it.
    if let Some(ns_window) = content.window() {
        force_dark(&ns_window);
    }
    let height = content.bounds().size.height;
    let column = NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(f64::from(sidebar_width), height),
    );
    if has_glass() {
        // Edge to edge; the window's own corners round it.
        let glass = glass_view(mtm, column, 0.0);
        glass.setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable);
        content.addSubview_positioned_relativeTo(&glass, NSWindowOrderingMode::Below, None);
    } else {
        let sidebar = effect_view(
            mtm,
            column,
            NSVisualEffectMaterial::Sidebar,
            NSVisualEffectState::FollowsWindowActiveState,
        );
        sidebar.setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable);
        content.addSubview_positioned_relativeTo(&sidebar, NSWindowOrderingMode::Below, None);
    }

    if let Some(ns_window) = content.window() {
        let toolbar = NSToolbar::new(mtm);
        ns_window.setToolbar(Some(&toolbar));
        ns_window.setToolbarStyle(NSWindowToolbarStyle::Unified);
        ns_window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        ns_window.invalidateShadow();
    }
}

/// Glass behind a floating capsule: a pill of `size` at `origin` (GPUI
/// coordinates, top-left origin) inside a transparent panel window. Returns
/// the view so animated capsules can resize it.
pub(crate) fn install_capsule_glass(
    window: &Window,
    origin: (f32, f32),
    size: (f32, f32),
) -> Option<Retained<NSView>> {
    let mtm = MainThreadMarker::new()?;
    let (_, content) = content_views(window)?;
    // The window server would shadow the whole transparent panel as a
    // rounded rectangle; the pill draws its own shadow instead.
    if let Some(ns_window) = content.window() {
        ns_window.setHasShadow(false);
    }
    let glass: Retained<NSView> = if has_glass() {
        Retained::into_super(glass_view(mtm, NSRect::default(), 0.0))
    } else {
        let hud = effect_view(
            mtm,
            NSRect::default(),
            NSVisualEffectMaterial::HUDWindow,
            NSVisualEffectState::Active,
        );
        hud.setWantsLayer(true);
        Retained::into_super(hud)
    };
    if let Some(ns_window) = content.window() {
        force_dark(&ns_window);
    }
    content.addSubview_positioned_relativeTo(&glass, NSWindowOrderingMode::Below, None);
    set_capsule_glass_frame(&glass, origin, size);
    Some(glass)
}

/// Moves and resizes capsule glass without implicit Core Animation, so it
/// tracks a GPUI-sprung width frame by frame.
pub(crate) fn set_capsule_glass_frame(glass: &NSView, origin: (f32, f32), size: (f32, f32)) {
    let Some(superview) = (unsafe { glass.superview() }) else {
        return;
    };
    let height = superview.bounds().size.height;
    let frame = NSRect::new(
        NSPoint::new(
            f64::from(origin.0),
            height - f64::from(origin.1) - f64::from(size.1),
        ),
        NSSize::new(f64::from(size.0), f64::from(size.1)),
    );
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    glass.setFrame(frame);
    if let Some(liquid) = glass.downcast_ref::<NSGlassEffectView>() {
        liquid.setCornerRadius(f64::from(size.1) / 2.0);
    } else if let Some(layer) = glass.layer() {
        layer.setCornerRadius(f64::from(size.1) / 2.0);
        layer.setMasksToBounds(true);
    }
    CATransaction::commit();
}

fn ns_window(window: &Window) -> Option<Retained<objc2_app_kit::NSWindow>> {
    let (gpui_view, _) = content_views(window)?;
    gpui_view.window()
}

fn timing(points: [f32; 4]) -> Retained<objc2_quartz_core::CAMediaTimingFunction> {
    objc2_quartz_core::CAMediaTimingFunction::functionWithControlPoints(
        points[0], points[1], points[2], points[3],
    )
}

/// The capsule entrance: a fade plus a zero-bounce spring that settles the
/// pill `travel` points down into place (negative rises from below).
/// Both run as Core Animation on the panel's content layer, so the render
/// server drives every frame even while the app's main thread is busy
/// launching; the window itself never moves. Reduce Motion keeps only a
/// short fade.
pub(crate) fn animate_panel_in(window: &Window, travel: f64) {
    let Some(layer) = panel_layer(window) else {
        return;
    };
    let reduced = crate::motion::reduced_motion();
    let fade = CABasicAnimation::animationWithKeyPath(Some(ns_string!("opacity")));
    unsafe {
        fade.setFromValue(Some(&NSNumber::new_f32(0.0)));
        fade.setToValue(Some(&NSNumber::new_f32(1.0)));
    }
    fade.setDuration(if reduced { 0.16 } else { 0.2 });
    fade.setTimingFunction(Some(&timing([0.2, 0.8, 0.2, 1.0])));
    layer.addAnimation_forKey(&fade, Some(ns_string!("hex-enter-fade")));
    if reduced {
        return;
    }
    // SwiftUI's `.spring(duration: 0.32, bounce: 0)`: critically damped,
    // stiffness (2π/d)² and damping 4π/d at unit mass.
    let response = 0.32_f64;
    let slide =
        CASpringAnimation::animationWithKeyPath(Some(ns_string!("transform.translation.y")));
    slide.setMass(1.0);
    slide.setStiffness((std::f64::consts::TAU / response).powi(2));
    slide.setDamping(2.0 * std::f64::consts::TAU / response);
    unsafe {
        // Layer space is y-up: a positive offset starts above the target.
        slide.setFromValue(Some(&NSNumber::new_f64(travel)));
        slide.setToValue(Some(&NSNumber::new_f64(0.0)));
    }
    slide.setDuration(slide.settlingDuration());
    layer.addAnimation_forKey(&slide, Some(ns_string!("hex-enter-slide")));
}

/// The capsule exit: a fade, shorter than the entrance. Returns false when
/// there is no native window to fade, so the caller removes it at once.
pub(crate) fn animate_panel_out(window: &Window) -> bool {
    let (Some(ns_window), Some(layer)) = (ns_window(window), panel_layer(window)) else {
        return false;
    };
    ns_window.setIgnoresMouseEvents(true);
    let fade = CABasicAnimation::animationWithKeyPath(Some(ns_string!("opacity")));
    unsafe {
        fade.setFromValue(Some(&NSNumber::new_f32(1.0)));
        fade.setToValue(Some(&NSNumber::new_f32(0.0)));
    }
    fade.setDuration(PANEL_EXIT_SECONDS);
    fade.setTimingFunction(Some(&timing([0.4, 0.0, 1.0, 1.0])));
    // Hold the final frame instead of changing the model opacity: AppKit owns
    // a layer-backed view's opacity and would show one bare frame first.
    fade.setFillMode(unsafe { objc2_quartz_core::kCAFillModeForwards });
    fade.setRemovedOnCompletion(false);
    layer.addAnimation_forKey(&fade, Some(ns_string!("hex-exit-fade")));
    true
}

/// The layer that holds a panel's GPUI view and its glass.
fn panel_layer(window: &Window) -> Option<Retained<CALayer>> {
    let (_, content) = content_views(window)?;
    content.setWantsLayer(true);
    content.layer()
}

pub(crate) const PANEL_EXIT_SECONDS: f64 = 0.18;

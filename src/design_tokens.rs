//! The desktop design tokens: the only place raw colours, type sizes, line
//! heights, radii, and control heights are written down. Panes and components
//! name these tokens; `docs/design-system.md` explains when to use each one.

use gpui::{BoxShadow, FontFeatures, FontWeight, Pixels, Styled, point, px, rgb, rgba};

// Quiet native, dark. One window surface; structure comes from hairlines and
// the sidebar's material, never from boxes around panels. Values follow the
// macOS 27 dark system colours, precomposited over the window base so panes
// can keep painting opaque `rgb()` tokens.

/// The window base: `windowBackgroundColor`. Panes sit directly on it.
pub(crate) const CANVAS: u32 = 0x1e1e1e;
/// The navigation column where no material is available (Linux, Windows),
/// and the picker's language list. On macOS the sidebar paints nothing and
/// an `NSVisualEffectView` (`.sidebar`) shows through.
pub(crate) const SIDEBAR: u32 = 0x252525;
/// Menus and popovers: the one floating surface inside the window.
pub(crate) const SURFACE: u32 = 0x2a2a2a;
/// A recessed control fill: segmented tracks, search fields, keycap wells
/// (white at 7%).
pub(crate) const FILL_QUIET: u32 = 0x2e2e2e;
/// The control fill: buttons, pop-up buttons, keycaps (white at 10%).
pub(crate) const FILL: u32 = 0x353535;
/// A pressed or open control (white at 16%).
pub(crate) const FILL_STRONG: u32 = 0x454545;
/// The selected row in a list: `unemphasizedSelectedContentBackgroundColor`.
pub(crate) const SURFACE_SELECTED: u32 = 0x464646;
/// Separators: `separatorColor` (white at 10%), drawn one device pixel wide.
pub(crate) const LINE: u32 = 0x343434;
/// Outlines of floating surfaces and the off toggle track.
pub(crate) const LINE_STRONG: u32 = 0x4a4a4a;
/// Modal backdrops.
pub(crate) const SCRIM: u32 = 0x00000099;

// Translucent tokens (RGBA, used with `rgba`) for content over a material:
// the macOS sidebar and the floating capsules.
/// The selected sidebar row over the sidebar material.
pub(crate) const SIDEBAR_SELECTED: u32 = 0xffffff1c;
/// The selected sidebar row while the window is inactive.
pub(crate) const SIDEBAR_SELECTED_INACTIVE: u32 = 0xffffff10;
/// The sidebar's trailing edge over its material.
pub(crate) const SIDEBAR_EDGE: u32 = 0x00000080;
/// A button fill on HUD glass.
pub(crate) const GLASS_FILL: u32 = 0xffffff1c;
/// The inner highlight that rims HUD glass.
pub(crate) const GLASS_RIM: u32 = 0xffffff2e;
/// The recording capsule's Stop button: system red at 18%.
pub(crate) const RECORDING_STOP_FILL: u32 = 0xff453a2e;
/// The graphite laid over the HUD material so capsules stay legible over
/// bright windows.
pub(crate) const GLASS_OVER_MATERIAL: u32 = 0x1c1c1eb3;
/// The graphite that tints HUD glass where no material is available.
pub(crate) const GLASS_TINT: u32 = 0x1c1c1eeb;

// Text, strongest to weakest. Tertiary is the weakest colour that still meets
// 4.5:1 on the window and sidebar; disabled is reserved for unavailable
// controls. (The system's own tertiaryLabel, white at 25%, is decorative.)
pub(crate) const TEXT_PRIMARY: u32 = 0xdedede;
pub(crate) const TEXT_SECONDARY: u32 = 0xb3b3b3;
pub(crate) const TEXT_TERTIARY: u32 = 0x919191;
pub(crate) const TEXT_DISABLED: u32 = 0x5c5c5c;

/// Switch and slider knobs, and text on an accent fill.
pub(crate) const KNOB: u32 = 0xffffff;
/// The bottom edge that seats a keycap (RGBA).
pub(crate) const KEYCAP_EDGE: u32 = 0x00000059;

/// The Primary button: a white fill with an ink label. No hue anywhere else
/// marks an action.
pub(crate) const PRIMARY_FILL: u32 = 0xf2f2f4;
pub(crate) const PRIMARY_INK: u32 = 0x1d1d1f;

// Roles. Each has a text/fill colour, a tinted surface, and a border.
/// Links and accent-tinted text (`linkColor`).
pub(crate) const ACCENT_TEXT: u32 = 0x419cff;
pub(crate) const ACCENT_SURFACE: u32 = 0x1b273a;
pub(crate) const ACCENT_BORDER: u32 = 0x2a4a78;
/// Text selection highlight (`selectedTextBackgroundColor`).
pub(crate) const SELECTION: u32 = 0x3f638bff;
/// The keyboard focus ring (`keyboardFocusIndicatorColor`).
pub(crate) const FOCUS_RING: u32 = 0x1aa9ff80;
/// The recording dot: `systemRed`. Only ever a dot.
pub(crate) const RECORDING: u32 = 0xff453a;
pub(crate) const SUCCESS: u32 = 0x30d158;
pub(crate) const SUCCESS_SURFACE: u32 = 0x16261b;
/// Destructive and failed text: a lighter red that stays readable.
pub(crate) const DANGER: u32 = 0xff6961;
pub(crate) const DANGER_SURFACE: u32 = 0x33191a;

/// The user's accent colour (`controlAccentColor`): the toggle's "on" state
/// and progress. Read once at launch; `#007AFF` elsewhere.
pub(crate) fn accent() -> u32 {
    static ACCENT: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ACCENT.get_or_init(system_accent)
}

#[cfg(target_os = "macos")]
fn system_accent() -> u32 {
    const FALLBACK: u32 = 0x007aff;
    if cfg!(test) || objc2::MainThreadMarker::new().is_none() {
        return FALLBACK;
    }
    let color = objc2_app_kit::NSColor::controlAccentColor();
    let Some(srgb) = color.colorUsingColorSpace(&objc2_app_kit::NSColorSpace::sRGBColorSpace())
    else {
        return FALLBACK;
    };
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
    (channel(srgb.redComponent()) << 16)
        | (channel(srgb.greenComponent()) << 8)
        | channel(srgb.blueComponent())
}

#[cfg(not(target_os = "macos"))]
fn system_accent() -> u32 {
    0x007aff
}

/// One step of the type scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TypeStep {
    pub(crate) size: f32,
    pub(crate) line_height: f32,
    /// `None` inherits the surrounding weight.
    pub(crate) weight: Option<FontWeight>,
}

/// Metadata, timestamps, tags, and section labels.
pub(crate) const CAPTION: TypeStep = TypeStep {
    size: 11.0,
    line_height: 14.0,
    weight: None,
};
/// Descriptions, control labels, and secondary copy.
pub(crate) const FOOTNOTE: TypeStep = TypeStep {
    size: 12.0,
    line_height: 16.0,
    weight: None,
};
/// The window default: values, inputs, list titles, and reading text.
pub(crate) const BODY: TypeStep = TypeStep {
    size: 13.0,
    line_height: 18.0,
    weight: None,
};
/// Long-form reading: meeting transcripts and summaries.
pub(crate) const READING: TypeStep = TypeStep {
    size: 14.0,
    line_height: 20.0,
    weight: None,
};
/// Toolbar (pane) titles and dialog titles: Apple's Title 3.
pub(crate) const HEADLINE: TypeStep = TypeStep {
    size: 15.0,
    line_height: 20.0,
    weight: Some(FontWeight::SEMIBOLD),
};
/// Detail titles and onboarding page titles.
pub(crate) const TITLE: TypeStep = TypeStep {
    size: 20.0,
    line_height: 24.0,
    weight: Some(FontWeight::SEMIBOLD),
};

/// The only text below the scale: the L/R badge in a sided modifier keycap.
pub(crate) const KEYCAP_SIDE_BADGE: TypeStep = TypeStep {
    size: 7.0,
    line_height: 8.0,
    weight: Some(FontWeight::SEMIBOLD),
};

pub(crate) const MONOSPACE_FAMILY: &str = "SF Mono";

/// SF Pro's ascender and descender as fractions of the font size.
const FONT_ASCENT: f32 = 0.952;
const FONT_DESCENT: f32 = 0.241;

/// The top inset that puts `smaller`'s first baseline on `larger`'s when both
/// lines start at the same top, such as a timestamp beside a row title.
pub(crate) fn baseline_offset(larger: TypeStep, smaller: TypeStep) -> f32 {
    // GPUI centres the font's ascent + descent in the line box.
    let baseline =
        |step: TypeStep| step.line_height / 2.0 + step.size * (FONT_ASCENT - FONT_DESCENT) / 2.0;
    baseline(larger) - baseline(smaller)
}

/// Applies type steps to any styled element. Weight is applied only by steps
/// that own one, so call `font_weight` after the step to emphasise text.
pub(crate) trait Typography: Styled + Sized {
    fn text_step(self, step: TypeStep) -> Self {
        let styled = self
            .text_size(px(step.size))
            .line_height(px(step.line_height));
        match step.weight {
            Some(weight) => styled.font_weight(weight),
            None => styled,
        }
    }

    fn text_caption(self) -> Self {
        self.text_step(CAPTION)
    }

    fn text_footnote(self) -> Self {
        self.text_step(FOOTNOTE)
    }

    fn text_body(self) -> Self {
        self.text_step(BODY)
    }

    fn text_reading(self) -> Self {
        self.text_step(READING)
    }

    fn text_headline(self) -> Self {
        self.text_step(HEADLINE)
    }

    fn text_title(self) -> Self {
        self.text_step(TITLE)
    }

    fn text_monospace(self) -> Self {
        self.font_family(MONOSPACE_FAMILY)
    }

    /// Tabular figures, for anything that counts or sits in a column.
    fn text_tabular(mut self) -> Self {
        self.text_style()
            .get_or_insert_with(Default::default)
            .font_features = Some(tabular_figures());
        self
    }
}

impl<E: Styled> Typography for E {}

fn tabular_figures() -> FontFeatures {
    static FEATURES: std::sync::OnceLock<FontFeatures> = std::sync::OnceLock::new();
    FEATURES
        .get_or_init(|| FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])))
        .clone()
}

/// One device pixel on a Retina display: every separator's width.
pub(crate) const HAIRLINE: Pixels = px(0.5);

/// Separators. Structure comes from these, never from boxed panels.
pub(crate) trait Hairlines: Styled + Sized {
    fn hairline_t(self) -> Self {
        self.border_t(HAIRLINE).border_color(rgb(LINE))
    }

    fn hairline_b(self) -> Self {
        self.border_b(HAIRLINE).border_color(rgb(LINE))
    }

    fn hairline_r(self) -> Self {
        self.border_r(HAIRLINE).border_color(rgb(LINE))
    }

    /// The keyboard focus ring, drawn 3 px outside the control's corners.
    fn focus_ring(self) -> Self {
        self.shadow(vec![BoxShadow {
            color: rgba(FOCUS_RING).into(),
            offset: point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(3.0),
        }])
    }

    /// The contact shadow under a switch or slider knob.
    fn knob_shadow(self) -> Self {
        self.shadow(vec![BoxShadow {
            color: rgba(0x00000059).into(),
            offset: point(px(0.0), px(1.0)),
            blur_radius: px(2.0),
            spread_radius: px(0.0),
        }])
    }

    /// A capsule's shadow, kept inside the 8 pt margin of its panel window.
    fn capsule_shadow(self) -> Self {
        self.shadow(vec![
            BoxShadow {
                color: rgba(0x0000004d).into(),
                offset: point(px(0.0), px(1.0)),
                blur_radius: px(3.0),
                spread_radius: px(0.0),
            },
            BoxShadow {
                color: rgba(0x00000033).into(),
                offset: point(px(0.0), px(3.0)),
                blur_radius: px(5.0),
                spread_radius: px(0.0),
            },
        ])
    }

    /// HUD glass: a contact shadow plus a wide ambient one.
    fn floating_shadow(self) -> Self {
        self.shadow(vec![
            BoxShadow {
                color: rgba(0x00000040).into(),
                offset: point(px(0.0), px(2.0)),
                blur_radius: px(6.0),
                spread_radius: px(0.0),
            },
            BoxShadow {
                color: rgba(0x00000059).into(),
                offset: point(px(0.0), px(12.0)),
                blur_radius: px(32.0),
                spread_radius: px(0.0),
            },
        ])
    }
}

impl<E: Styled> Hairlines for E {}

// Spacing: a 4 px grid. Prefer GPUI's spacing utilities (`*_1` 4, `*_2` 8,
// `*_3` 12, `*_4` 16, `*_5` 20, `*_6` 24, `*_8` 32); `SPACE_2` is named for
// arithmetic on control heights and negative icon-button margins.
pub(crate) const SPACE_2: f32 = 8.0;

// Radii.
pub(crate) const RADIUS_SMALL: Pixels = px(4.0);
pub(crate) const RADIUS_CONTROL: Pixels = px(6.0);
/// Menus, popovers, and the selected sidebar row's family.
pub(crate) const RADIUS_PANEL: Pixels = px(8.0);
pub(crate) const RADIUS_MODAL: Pixels = px(12.0);
pub(crate) const RADIUS_KNOB: Pixels = px(2.0);

/// The corner radius of an application icon or monogram of `size`.
pub(crate) fn icon_radius(size: f32) -> Pixels {
    px(size * 0.22)
}

// Control geometry: macOS regular (24), small (20), and large (28) controls.
pub(crate) const CONTROL_HEIGHT: f32 = 24.0;
pub(crate) const CONTROL_HEIGHT_SMALL: f32 = 20.0;
/// The toolbar's main action and search field.
pub(crate) const CONTROL_HEIGHT_LARGE: f32 = 28.0;
pub(crate) const KEYCAP_WIDTH: f32 = 24.0;
/// The narrowest a settings row's title and description get before its
/// control wraps below them.
pub(crate) const SETTINGS_COPY_MIN_WIDTH: f32 = 180.0;
pub(crate) const TAG_HEIGHT: f32 = 18.0;
/// A menu row (macOS menus use 22 pt rows).
pub(crate) const MENU_ITEM_HEIGHT: f32 = 22.0;
pub(crate) const MENU_WIDTH: f32 = 200.0;
/// The gap between a control and the menu it opens.
pub(crate) const MENU_OFFSET: f32 = 4.0;
pub(crate) const NAV_ITEM_HEIGHT: f32 = 28.0;
pub(crate) const MULTILINE_INPUT_HEIGHT: f32 = 76.0;
pub(crate) const DISCLOSURE_WIDTH: f32 = 220.0;
pub(crate) const SEARCH_FIELD_WIDTH: f32 = 200.0;
/// The widest an inline field beside a row label grows.
pub(crate) const MODE_FIELD_WIDTH: f32 = 300.0;
pub(crate) const ICON_SIZE: f32 = 14.0;
pub(crate) const ICON_SIZE_SMALL: f32 = 11.0;
/// The switch: a capsule track with a round thumb.
pub(crate) const TOGGLE_WIDTH: f32 = 36.0;
pub(crate) const TOGGLE_HEIGHT: f32 = 20.0;
/// Opacity of a control that cannot currently be used.
pub(crate) const DISABLED_OPACITY: f32 = 0.4;

// Pane geometry.
pub(crate) const SIDEBAR_WIDTH: f32 = 216.0;
/// Space above the sidebar's first item: the unified toolbar's height, so
/// the first item clears the traffic lights.
pub(crate) const TITLEBAR_INSET: f32 = 52.0;
/// The toolbar row: pane title and its controls, level with the traffic
/// lights.
pub(crate) const PANE_HEADER_HEIGHT: f32 = 52.0;
/// Horizontal inset shared by every pane header and body.
pub(crate) const PANE_GUTTER: f32 = 28.0;
/// The one content width every pane is bounded to.
pub(crate) const PANE_CONTENT_WIDTH: f32 = 760.0;
/// The list column of a list+detail pane.
pub(crate) const PANE_LIST_WIDTH: f32 = 272.0;
/// The narrower list column for panes whose detail is an editor (Modes).
pub(crate) const PANE_LIST_WIDTH_COMPACT: f32 = 220.0;
pub(crate) const MODAL_WIDTH: f32 = 780.0;
pub(crate) const MODAL_HEIGHT: f32 = 520.0;
/// A determinate or indeterminate progress track.
pub(crate) const PROGRESS_HEIGHT: f32 = 4.0;
pub(crate) const PANEL_HEADER_HEIGHT: f32 = 36.0;
pub(crate) const SETTINGS_ROW_MIN_HEIGHT: f32 = 48.0;
/// The speaker column of a meeting transcript.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const TRANSCRIPT_SPEAKER_WIDTH: f32 = 52.0;
/// The timestamp column of a meeting transcript.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const TRANSCRIPT_TIME_WIDTH: f32 = 44.0;
/// Fixed width of a meeting row's inline Copy / Copied button.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const MEETING_ROW_COPY_WIDTH: f32 = 58.0;

// Meeting capsule geometry. Capsules are concentric: a 60 pt pill, 12 pt
// padding, 36 pt pill buttons.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const MEETING_PANEL_WIDTH: f32 = 468.0;
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const MEETING_PANEL_HEIGHT: f32 = 76.0;
/// The capsule's own height inside its panel window.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const CAPSULE_HEIGHT: f32 = 60.0;
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const CAPSULE_BUTTON_HEIGHT: f32 = 36.0;
/// The round well holding a capsule's leading glyph.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const CAPSULE_LENS_SIZE: f32 = 32.0;
/// Expanded width of the floating meeting recording capsule.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const RECORDING_PANEL_WIDTH: f32 = 600.0;
/// Compact width of the floating meeting recording capsule at rest.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const RECORDING_COMPACT_WIDTH: f32 = 252.0;
/// Height of the floating meeting recording capsule window.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const RECORDING_PANEL_HEIGHT: f32 = 60.0;
/// Height of the recording pill inside its window.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const RECORDING_PILL_HEIGHT: f32 = 44.0;
/// Height of a button inside the recording pill.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const RECORDING_BUTTON_HEIGHT: f32 = 32.0;
/// Fixed width of the recording capsule's Copy new / Copied button.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const RECORDING_COPY_NEW_WIDTH: f32 = 126.0;
/// Fixed width of the recording capsule's Copy all / Copied button.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const RECORDING_COPY_ALL_WIDTH: f32 = 84.0;
/// Fixed width of the ready capsule's Copy / Copied button.
#[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
pub(crate) const READY_COPY_WIDTH: f32 = 84.0;

// Onboarding geometry.
/// The tour's one column: page art, copy, cards, and footer share its edges.
pub(crate) const ONBOARDING_WIDTH: f32 = 520.0;
/// The illustration band above a centred page's title.
pub(crate) const ONBOARDING_ART_HEIGHT: f32 = 148.0;
/// The shortcut drawn as a large keycap in onboarding art.
pub(crate) const HERO_KEYCAP_SIZE: f32 = 56.0;
/// An icon drawn as onboarding art, such as the finished tour's check.
pub(crate) const HERO_ICON_SIZE: f32 = 28.0;
/// How far a hero keycap stands above its base before it is pressed.
pub(crate) const HERO_KEYCAP_DEPTH: f32 = 3.0;
/// The measure of a page's centred body copy.
pub(crate) const ONBOARDING_BODY_WIDTH: f32 = 400.0;
/// The illustration column of a "How Hex works" row.
pub(crate) const ONBOARDING_ART_COLUMN: f32 = 112.0;
/// A setup row's download progress, track and percentage together.
pub(crate) const ONBOARDING_PROGRESS_WIDTH: f32 = 140.0;
/// The tour's main button, centred under each page.
pub(crate) const ONBOARDING_PRIMARY_WIDTH: f32 = 180.0;
/// The round badge holding a setup row's icon.
pub(crate) const ONBOARDING_BADGE_SIZE: f32 = 28.0;
/// The tour's step dots; the current one stretches to the wide width.
pub(crate) const STEP_DOT_SIZE: f32 = 6.0;
pub(crate) const STEP_DOT_ACTIVE_WIDTH: f32 = 16.0;

// Motion presets (`duration` + `bounce`, SwiftUI mapping) and transition offsets.
pub(crate) use crate::motion::Spec as SpringSpec;
#[allow(dead_code)]
pub(crate) const SPRING_INSTANT: SpringSpec = crate::motion::presets::INSTANT;
pub(crate) const SPRING_SNAP: SpringSpec = crate::motion::presets::SNAP;
#[allow(dead_code)]
pub(crate) const SPRING_SNAP_EXIT: SpringSpec = crate::motion::presets::SNAP_EXIT;
pub(crate) const SPRING_SETTLE: SpringSpec = crate::motion::presets::SETTLE;
#[allow(dead_code)]
pub(crate) const SPRING_SETTLE_EXIT: SpringSpec = crate::motion::presets::SETTLE_EXIT;
pub(crate) const SPRING_REFLOW: SpringSpec = crate::motion::presets::REFLOW;
#[allow(dead_code)]
pub(crate) const SPRING_REFLOW_EXIT: SpringSpec = crate::motion::presets::REFLOW_EXIT;
#[allow(dead_code)]
pub(crate) const SPRING_GLIDE: SpringSpec = crate::motion::presets::GLIDE;
#[allow(dead_code)]
pub(crate) const SPRING_GLIDE_EXIT: SpringSpec = crate::motion::presets::GLIDE_EXIT;
#[allow(dead_code)]
pub(crate) const SPRING_MORPH: SpringSpec = crate::motion::presets::MORPH;
#[allow(dead_code)]
pub(crate) const SPRING_MORPH_EXIT: SpringSpec = crate::motion::presets::MORPH_EXIT;
#[allow(dead_code)]
pub(crate) const SPRING_POP: SpringSpec = crate::motion::presets::POP;
#[allow(dead_code)]
pub(crate) const SPRING_POP_EXIT: SpringSpec = crate::motion::presets::POP_EXIT;
#[allow(dead_code)]
pub(crate) const SPRING_REDUCED_FADE: SpringSpec = crate::motion::presets::REDUCED_FADE;
/// Vertical travel (in px) of a pane's entrance transition when Reduce Motion is off.
pub(crate) const PANE_ENTER_OFFSET: f32 = 4.0;
/// Vertical travel (in px) of a dropdown menu opening from its trigger.
pub(crate) const MENU_ENTER_OFFSET: f32 = 4.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(hex: u32) -> f64 {
        let channel = |shift: u32| {
            let value = f64::from((hex >> shift) & 0xff) / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }

    fn contrast(foreground: u32, background: u32) -> f64 {
        let (light, dark) = {
            let (a, b) = (luminance(foreground), luminance(background));
            if a > b { (a, b) } else { (b, a) }
        };
        (light + 0.05) / (dark + 0.05)
    }

    #[test]
    fn readable_text_meets_aa_contrast_on_every_surface() {
        // Every readable tier holds 4.5:1 where text sits at rest.
        for background in [CANVAS, SIDEBAR, SURFACE] {
            for text in [TEXT_PRIMARY, TEXT_SECONDARY, TEXT_TERTIARY] {
                let ratio = contrast(text, background);
                assert!(
                    ratio >= 4.5,
                    "{text:06x} on {background:06x} is {ratio:.2}:1"
                );
            }
        }
        // On controls and selected rows, metadata switches to secondary.
        for background in [FILL_QUIET, FILL, FILL_STRONG, SURFACE_SELECTED] {
            for text in [TEXT_PRIMARY, TEXT_SECONDARY] {
                let ratio = contrast(text, background);
                assert!(
                    ratio >= 4.5,
                    "{text:06x} on {background:06x} is {ratio:.2}:1"
                );
            }
        }
        for (text, surface) in [
            (ACCENT_TEXT, ACCENT_SURFACE),
            (SUCCESS, SUCCESS_SURFACE),
            (DANGER, DANGER_SURFACE),
            (DANGER, CANVAS),
            (PRIMARY_INK, PRIMARY_FILL),
        ] {
            let ratio = contrast(text, surface);
            assert!(ratio >= 4.5, "{text:06x} on {surface:06x} is {ratio:.2}:1");
        }
    }

    #[test]
    fn type_scale_ascends() {
        let steps = [CAPTION, FOOTNOTE, BODY, READING, HEADLINE, TITLE];
        for pair in steps.windows(2) {
            assert!(pair[0].size < pair[1].size);
            assert!(pair[0].line_height <= pair[1].line_height);
        }
        for step in steps {
            assert!(step.line_height >= step.size * 1.2);
        }
    }

    #[test]
    fn baseline_offset_lowers_smaller_text_onto_the_larger_baseline() {
        assert_eq!(baseline_offset(BODY, BODY), 0.0);
        let caption = baseline_offset(BODY, CAPTION);
        assert!((2.5..3.0).contains(&caption), "{caption}");
        assert!(baseline_offset(HEADLINE, CAPTION) > caption);
    }

    /// Raw styling values belong here. A pane that writes a literal colour,
    /// font size, line height, or radius, or truncates text by hand, bypasses
    /// the design system.
    #[test]
    fn desktop_sources_use_tokens_instead_of_raw_styles() {
        const SOURCES: &[(&str, &str)] = &[
            ("app_window.rs", include_str!("app_window.rs")),
            ("app_window/macos.rs", include_str!("app_window/macos.rs")),
            ("app_window/linux.rs", include_str!("app_window/linux.rs")),
            ("app_window/tour.rs", include_str!("app_window/tour.rs")),
            (
                "app_window/windows.rs",
                include_str!("app_window/windows.rs"),
            ),
            ("desktop_ui.rs", include_str!("desktop_ui.rs")),
            (
                "desktop_transcription_picker.rs",
                include_str!("desktop_transcription_picker.rs"),
            ),
            ("linux_app.rs", include_str!("linux_app.rs")),
            ("meeting_watcher.rs", include_str!("meeting_watcher.rs")),
            ("text_input.rs", include_str!("text_input.rs")),
        ];
        const FORBIDDEN: &[&str] = &[
            "rgb(0x",
            "rgba(0x",
            ".text_size(",
            ".line_height(px(",
            ".line_height(relative(",
            ".line_height(rems(",
            ".font_family(",
            ".rounded(px(",
            ".rounded_sm()",
            ".rounded_md()",
            ".rounded_lg()",
            ".text_xs()",
            ".text_sm()",
            ".text_base()",
            ".text_lg()",
            ".text_xl()",
            // Separators go through `Hairlines`, one device pixel wide.
            ".border_b_1()",
            ".border_t_1()",
            ".border_r_1()",
            ".border_l_1()",
            // macOS controls keep the arrow cursor.
            ".cursor_pointer()",
            // One-line text goes through `desktop_ui::truncated_line`.
            ".truncate()",
        ];
        let mut violations = Vec::new();
        for (file, source) in SOURCES {
            for (index, line) in source.lines().enumerate() {
                for pattern in FORBIDDEN {
                    if line.contains(pattern) {
                        violations.push(format!("{file}:{}: {}", index + 1, line.trim()));
                    }
                }
            }
        }
        assert!(
            violations.is_empty(),
            "use design_tokens instead of raw styles:\n{}",
            violations.join("\n")
        );
    }
}

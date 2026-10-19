//! Shared mode collection and selection list for desktop roots that implement
//! contextual dictation modes.
//!
//! Entries contain presentation data only. Native roots still own mode schemas,
//! matching semantics, persistence, and the detail editor behind each row.

use std::sync::Arc;

use crate::design_tokens::*;
use crate::desktop_i18n::tr;
use crate::desktop_ui::{Icon, Tone, icon, list_row, list_row_meta, list_row_title, panel, tag};
use gpui::{
    AnyElement, Context, FontWeight, Image, MouseButton, MouseDownEvent, Pixels, Point,
    SharedString, Window, div, img, prelude::*, px, rgb,
};

pub(crate) use crate::desktop_mode_target::ModeTarget;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ModeActivationKind {
    Application,
    Website,
}

#[derive(Clone)]
pub(crate) struct ModeActivation {
    label: String,
    icon: Option<Arc<Image>>,
    kind: ModeActivationKind,
}

impl ModeActivation {
    pub(crate) fn application(label: impl Into<String>, icon: Option<Arc<Image>>) -> Self {
        Self {
            label: label.into(),
            icon,
            kind: ModeActivationKind::Application,
        }
    }

    pub(crate) fn website(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            kind: ModeActivationKind::Website,
        }
    }
}

pub(crate) struct ModeListEntry {
    pub(crate) target: ModeTarget,
    pub(crate) title: String,
    pub(crate) empty_subtitle: &'static str,
    pub(crate) activations: Vec<ModeActivation>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ModeListAction {
    Select(ModeTarget),
    OpenContextMenu {
        target: ModeTarget,
        position: Point<Pixels>,
    },
}

pub(crate) trait ModeListDelegate: Sized + 'static {
    fn handle_mode_list_action(
        &mut self,
        action: ModeListAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
}

pub(crate) struct ModeListView {
    pub(crate) entries: Vec<ModeListEntry>,
    pub(crate) selected: ModeTarget,
    /// Whether custom rows expose a root-owned secondary-click action.
    pub(crate) secondary_action: bool,
}

pub(crate) fn render_mode_list<T: ModeListDelegate>(
    view: ModeListView,
    cx: &mut Context<T>,
) -> AnyElement {
    let rows = view
        .entries
        .into_iter()
        .map(|entry| {
            let target = entry.target;
            let selected = view.selected == target;
            let row_id = SharedString::from(format!("mode-list-{}", entry.target.id_fragment()));
            mode_row(entry, selected)
                .id(row_id)
                .when(
                    view.secondary_action && matches!(target, ModeTarget::Mode(_)),
                    |row| {
                        row.on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                this.handle_mode_list_action(
                                    ModeListAction::OpenContextMenu {
                                        target,
                                        position: event.position,
                                    },
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        )
                    },
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.handle_mode_list_action(ModeListAction::Select(target), window, cx);
                }))
        })
        .collect::<Vec<_>>();

    panel()
        .id("mode-list-card")
        .h_full()
        .overflow_y_scroll()
        .child(div().p_2().flex().flex_col().gap_1().children(rows))
        .into_any_element()
}

fn mode_row(entry: ModeListEntry, selected: bool) -> gpui::Div {
    let has_activations = !entry.activations.is_empty();
    list_row(selected)
        .gap_1()
        .child(list_row_title(entry.title, selected))
        .when(has_activations, |row| {
            row.child(mode_activation_icons(&entry.activations))
        })
        .when(!has_activations, |row| {
            row.child(list_row_meta(tr(entry.empty_subtitle), selected))
        })
}

const ACTIVATION_ICON_SIZE: f32 = 18.0;

fn mode_activation_icons(activations: &[ModeActivation]) -> AnyElement {
    let visible = activations.len().min(5);
    let mut icons = activations
        .iter()
        .take(visible)
        .map(|activation| match activation.kind {
            ModeActivationKind::Application => render_application_icon(
                activation.icon.clone(),
                &activation.label,
                ACTIVATION_ICON_SIZE,
            ),
            ModeActivationKind::Website => icon_tile(ACTIVATION_ICON_SIZE)
                .child(icon(Icon::Globe, TEXT_SECONDARY))
                .into_any_element(),
        })
        .collect::<Vec<_>>();
    if activations.len() > visible {
        icons.push(
            tag(Tone::Neutral, format!("+{}", activations.len() - visible)).into_any_element(),
        );
    }
    div()
        .h(px(ACTIVATION_ICON_SIZE))
        .flex()
        .items_center()
        .gap_1()
        .children(icons)
        .into_any_element()
}

/// An application's icon, or a monogram tile when its icon could not be read.
pub(crate) fn render_application_icon(
    icon: Option<Arc<Image>>,
    fallback_name: &str,
    size: f32,
) -> AnyElement {
    if let Some(icon) = icon {
        return img(icon)
            .size(px(size))
            .rounded(icon_radius(size))
            .into_any_element();
    }
    let initial = fallback_name
        .chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into());
    icon_tile(size)
        .map(|monogram| {
            if size >= CONTROL_HEIGHT {
                monogram.text_body()
            } else {
                monogram.text_caption()
            }
        })
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(TEXT_SECONDARY))
        .child(initial)
        .into_any_element()
}

fn icon_tile(size: f32) -> gpui::Div {
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(icon_radius(size))
        .bg(rgb(SURFACE_SELECTED))
}

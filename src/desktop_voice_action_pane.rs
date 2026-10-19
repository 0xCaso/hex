//! Shared Voice Action pane presentation.
//!
//! Native roots own shortcut capture, selected-text access, OpenCode discovery,
//! model selection, persistence, and execution. This module owns the common
//! pane scaffold, explanatory copy, setting rows, and unavailable-state card.

use crate::design_tokens::*;
use crate::desktop_i18n::tr;
use crate::desktop_ui::{
    ButtonKind, button, error_message, error_panel, pane_body, pane_content, pane_header,
    pane_scroll_body, panel, section_label, settings_copy,
};
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, prelude::*, px, rgb};

const EXPLAINER: &str = "Hold the shortcut and speak an instruction. Hex sends it, along with any text you have selected, to OpenCode and pastes the reply at your cursor. If the model returns nothing, nothing is pasted.";
pub(crate) const OPENCODE_SETUP_URL: &str = "https://v2.opencode.ai/";

pub(crate) struct VoiceActionSettingRow {
    title: &'static str,
    description: SharedString,
    control: AnyElement,
}

impl VoiceActionSettingRow {
    pub(crate) fn translated(
        title: &'static str,
        description: &'static str,
        control: impl IntoElement,
    ) -> Self {
        Self {
            title,
            description: tr(description).into(),
            control: control.into_any_element(),
        }
    }

    #[cfg_attr(any(target_os = "linux", target_os = "windows"), allow(dead_code))]
    pub(crate) fn dynamic(
        title: &'static str,
        description: impl Into<SharedString>,
        control: impl IntoElement,
    ) -> Self {
        Self {
            title,
            description: description.into(),
            control: control.into_any_element(),
        }
    }
}

pub(crate) struct VoiceActionError {
    pub(crate) title: &'static str,
    pub(crate) detail: String,
}

pub(crate) struct VoiceActionReadyView {
    pub(crate) shortcut: VoiceActionSettingRow,
    pub(crate) processing: Vec<VoiceActionSettingRow>,
    pub(crate) error: Option<VoiceActionError>,
}

pub(crate) struct VoiceActionUnavailableView {
    pub(crate) title: &'static str,
    pub(crate) description: &'static str,
    pub(crate) error: Option<VoiceActionError>,
    pub(crate) retry_label: Option<&'static str>,
    pub(crate) setup_label: Option<&'static str>,
}

pub(crate) enum VoiceActionView {
    Ready(Box<VoiceActionReadyView>),
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    Unavailable(VoiceActionUnavailableView),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VoiceActionPaneAction {
    RetryOpenCode,
    OpenOpenCodeSetup,
}

pub(crate) trait VoiceActionPaneDelegate: Sized + 'static {
    fn handle_voice_action_pane_action(
        &mut self,
        action: VoiceActionPaneAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
}

pub(crate) fn render_voice_action_pane<T: VoiceActionPaneDelegate>(
    view: VoiceActionView,
    window: &mut Window,
    cx: &mut Context<T>,
) -> AnyElement {
    match view {
        VoiceActionView::Ready(view) => render_ready(*view, window),
        VoiceActionView::Unavailable(view) => render_unavailable(view, cx),
    }
}

fn render_ready(view: VoiceActionReadyView, window: &mut Window) -> AnyElement {
    let compact = window.viewport_size().width < px(980.0);
    let processing_count = view.processing.len();
    let processing = view
        .processing
        .into_iter()
        .enumerate()
        .map(|(index, row)| render_setting_row(row, index + 1 < processing_count, compact));
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(pane_header(tr("Voice Action")))
        .child(pane_scroll_body(
            "voice-action-scroll",
            pane_content()
                .pt_5()
                .gap_4()
                .child(
                    div()
                        .px_1()
                        .pt_1()
                        .text_caption()
                        .text_color(rgb(TEXT_TERTIARY))
                        .child(tr(EXPLAINER)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(section_label(tr("Capture")))
                        .child(
                            panel().child(
                                render_setting_row(view.shortcut, false, compact)
                                    .id("voice-action-hotkey-setting"),
                            ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(section_label(tr("Processing")))
                        .child(panel().children(processing)),
                )
                .when_some(view.error, |content, error| {
                    content.child(error_message(tr(error.title), error.detail))
                }),
        ))
        .into_any_element()
}

fn render_unavailable<T: VoiceActionPaneDelegate>(
    view: VoiceActionUnavailableView,
    cx: &mut Context<T>,
) -> AnyElement {
    let retry = view.retry_label.map(|label| {
        button(ButtonKind::Secondary, tr(label))
            .id("retry-opencode")
            .on_click(cx.listener(|this, _, window, cx| {
                this.handle_voice_action_pane_action(
                    VoiceActionPaneAction::RetryOpenCode,
                    window,
                    cx,
                );
            }))
    });
    let setup = view.setup_label.map(|label| {
        button(ButtonKind::Secondary, tr(label))
            .id("open-opencode-setup")
            .on_click(cx.listener(|this, _, window, cx| {
                this.handle_voice_action_pane_action(
                    VoiceActionPaneAction::OpenOpenCodeSetup,
                    window,
                    cx,
                );
            }))
    });
    let show_actions = retry.is_some() || setup.is_some();
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(pane_header(tr("Voice Action")))
        .child(
            pane_body().child(
                div()
                    .id("voice-action-unavailable-scroll")
                    .size_full()
                    .p_5()
                    .overflow_y_scroll()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(420.0))
                            .p_5()
                            .rounded(RADIUS_CONTROL)
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(SURFACE))
                            .child(settings_copy(tr(view.title), tr(view.description)))
                            .when_some(view.error, |card, error| {
                                card.child(error_panel(tr(error.title), error.detail).mt_3())
                            })
                            .when(show_actions, |card| {
                                card.child(
                                    div().mt_4().flex().gap_2().children(retry).children(setup),
                                )
                            }),
                    ),
            ),
        )
        .into_any_element()
}

fn render_setting_row(row: VoiceActionSettingRow, divider: bool, compact: bool) -> gpui::Div {
    div()
        .w_full()
        .min_h(px(64.0))
        .px_3()
        .py_3()
        .flex()
        .when(compact, |row| row.flex_col().items_start().gap_3())
        .when(!compact, |row| row.items_center().justify_between().gap_4())
        .when(divider, |row| row.border_b_1().border_color(rgb(LINE)))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_footnote()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(TEXT_PRIMARY))
                        .child(tr(row.title)),
                )
                .child(
                    div()
                        .text_caption()
                        .text_color(rgb(TEXT_TERTIARY))
                        .child(row.description),
                ),
        )
        .child(
            div()
                .when(compact, |field| field.w_full())
                .when(!compact, |field| field.flex_none())
                .child(row.control),
        )
}

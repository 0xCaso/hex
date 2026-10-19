//! The shared Activity pane: the live listening session at a glance —
//! state, capture device, session age, the most recent transcripts, and
//! where the raw observations live. Read-only, so both port shells
//! render it from the snapshot without a delegate.

use gpui::{AnyElement, div, prelude::*, px, rgb};

use crate::design_tokens::*;
use crate::desktop_i18n::{event_age, tr, tr_fill};
use crate::desktop_ui::{
    empty_state, pane_content, pane_header, pane_scroll_body, panel, settings_row,
    settings_section_label,
};
use crate::windows_desktop_host::DesktopSnapshot;

/// A right-hand row value capped at 240 px. The text shrinks inside a flex
/// row so GPUI lays it out at the capped width and draws an ellipsis.
fn truncated_value(value: String, size: f32, color: u32) -> gpui::Div {
    div().max_w(px(240.0)).flex().child(
        div()
            .min_w(px(0.0))
            .overflow_x_hidden()
            .truncate()
            .text_size(px(size))
            .text_color(rgb(color))
            .child(value),
    )
}

pub(crate) fn render_activity_pane(snapshot: &DesktopSnapshot) -> AnyElement {
    let running = snapshot
        .listener
        .as_ref()
        .is_some_and(|listener| listener.running);
    let state = snapshot.listener.as_ref().map_or_else(
        || tr("Disabled").to_string(),
        |listener| tr(&listener.status).to_string(),
    );
    let device = snapshot
        .activity
        .device
        .clone()
        .unwrap_or_else(|| tr("Automatic microphone").into());
    let session_age = snapshot.activity.session_started_at.map(event_age);
    let transcripts = snapshot.transcripts.clone();

    div()
        .size_full()
        .flex()
        .flex_col()
        .child(pane_header(tr("Activity")))
        .child(pane_scroll_body(
            "activity-scroll",
            pane_content()
                .child(
                    div()
                        .pt(px(20.0))
                        .child(settings_section_label(tr("Session"))),
                )
                .child(
                    panel()
                        .child(
                            div()
                                .w_full()
                                .min_h(px(56.0))
                                .px_4()
                                .py_3()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(div().size(px(10.0)).flex_none().rounded_full().bg(
                                    if running {
                                        rgb(SUCCESS)
                                    } else {
                                        rgb(TEXT_TERTIARY)
                                    },
                                ))
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .child(
                                            div()
                                                .text_body()
                                                .text_color(rgb(TEXT_PRIMARY))
                                                .child(state),
                                        )
                                        .child(
                                            div()
                                                .text_footnote()
                                                .text_color(rgb(TEXT_TERTIARY))
                                                .truncate()
                                                .child(match &session_age {
                                                    Some(age) => tr_fill("Session started {}", age),
                                                    None => {
                                                        tr("No session recorded yet").to_string()
                                                    }
                                                }),
                                        ),
                                ),
                        )
                        .child(
                            settings_row(
                                tr("Microphone"),
                                tr("The device the active session captures from"),
                                truncated_value(device, 12.0, TEXT_TERTIARY),
                            )
                            .border_b_0(),
                        )
                        .when_some(snapshot.last_failure.clone(), |panel, failure| {
                            panel.child(
                                div()
                                    .w_full()
                                    .px_4()
                                    .pb_3()
                                    .text_footnote()
                                    .text_color(rgb(DANGER))
                                    .child(tr_fill("Last dictation failed: {}", &failure)),
                            )
                        }),
                )
                .child(settings_section_label(tr("Recent transcripts")))
                .child(if transcripts.is_empty() {
                    panel()
                        .child(empty_state(tr("Nothing transcribed this session yet.")))
                        .into_any_element()
                } else {
                    panel()
                        .children(transcripts.iter().enumerate().map(|(index, transcript)| {
                            let last = index + 1 == transcripts.len();
                            div()
                                .w_full()
                                .px_4()
                                .py_3()
                                .when(!last, |row| row.border_b_1().border_color(rgb(LINE)))
                                .text_footnote()
                                .text_color(rgb(TEXT_SECONDARY))
                                .child(transcript.clone())
                        }))
                        .into_any_element()
                })
                .child(settings_section_label(tr("Observations")))
                .child(
                    panel().child(
                        settings_row(
                            tr("Event log"),
                            tr("Newest-first session observations on this disk"),
                            truncated_value(
                                snapshot.observations_path.clone(),
                                11.0,
                                TEXT_TERTIARY,
                            ),
                        )
                        .border_b_0(),
                    ),
                ),
        ))
        .into_any_element()
}

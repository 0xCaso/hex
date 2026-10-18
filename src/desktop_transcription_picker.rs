use gpui::{
    AnyElement, Context, FontWeight, IntoElement, div, prelude::*, px, relative, rgb, rgba,
};

use crate::design_tokens::*;
use crate::desktop_ui::{
    ButtonKind, Icon, Tone, error_panel, icon_button, menu_item, small_button, tag,
};
use crate::transcription_models::{
    ModelChoice, ModelDefinition, TranscriptionModelId, TranscriptionSelection, language_name,
};

pub(crate) fn transcription_selection_is_active(
    selection: &TranscriptionSelection,
    model: &ModelDefinition,
    language: &str,
    installed: bool,
) -> bool {
    installed && selection.model == model.id && selection.language == language
}

#[derive(Clone)]
pub(crate) struct TranscriptionPickerModel {
    pub(crate) choice: ModelChoice,
    pub(crate) status: TranscriptionPickerStatus,
}

#[derive(Clone)]
pub(crate) enum TranscriptionPickerStatus {
    Active,
    Available {
        installed: bool,
        deletion: ModelDeletion,
    },
    Preparing {
        label: String,
        progress: Option<TranscriptionPickerProgress>,
    },
}

/// Whether a downloaded model card offers deleting its files.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub(crate) enum ModelDeletion {
    Unavailable,
    Offered,
    Confirming,
}

#[derive(Clone, Copy)]
pub(crate) enum TranscriptionPickerProgress {
    Downloading(f32),
    Loading(f32),
}

#[derive(Clone)]
pub(crate) struct TranscriptionPickerView {
    pub(crate) error: Option<String>,
    pub(crate) deletion_error: Option<String>,
    pub(crate) language: String,
    pub(crate) models: Vec<TranscriptionPickerModel>,
}

pub(crate) trait TranscriptionPickerDelegate: Sized + 'static {
    fn cancel_transcription_preparation(&mut self);
    fn choose_transcription_model(
        &mut self,
        model: TranscriptionModelId,
        language: String,
        cx: &mut Context<Self>,
    );
    /// The first call asks for confirmation; a repeated call deletes the model.
    /// Roots that never offer deletion keep this default.
    fn delete_transcription_model(
        &mut self,
        _model: TranscriptionModelId,
        _cx: &mut Context<Self>,
    ) {
    }
    fn dismiss_transcription_picker(&mut self, cx: &mut Context<Self>);
    fn select_transcription_language(&mut self, language: String, cx: &mut Context<Self>);
}

pub(crate) fn render_transcription_picker<T: TranscriptionPickerDelegate>(
    view: TranscriptionPickerView,
    cx: &mut Context<T>,
) -> AnyElement {
    let selected_language = view.language.clone();
    let language_rows = crate::transcription_models::LANGUAGES
        .iter()
        .enumerate()
        .map(|(index, (code, name))| {
            let selected = *code == selected_language;
            let code = (*code).to_string();
            menu_item(selected)
                .id(("transcription-language", index))
                .text_body()
                .child(*name)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.select_transcription_language(code.clone(), cx);
                }))
        });
    let model_cards = view.models.into_iter().enumerate().map(|(index, model)| {
        let language = view.language.clone();
        let definition = model.choice.model;
        let mut deletion = ModelDeletion::Unavailable;
        let (state, action, active, preparing, progress, installed) = match model.status {
            TranscriptionPickerStatus::Active => {
                (tag(Tone::Accent, "Active"), None, true, false, None, true)
            }
            TranscriptionPickerStatus::Available {
                installed,
                deletion: offered,
            } => {
                deletion = offered;
                (
                    tag(Tone::Neutral, model.choice.recommendation.label()),
                    Some(if installed { "Use" } else { "Download" }),
                    false,
                    false,
                    None,
                    installed,
                )
            }
            TranscriptionPickerStatus::Preparing { label, progress } => (
                tag(Tone::Accent, label),
                Some("Cancel"),
                false,
                true,
                progress,
                false,
            ),
        };
        let mut metadata = if definition.coverage == language_name(&language) {
            format!("{} · {}", definition.quality_context, definition.timestamps)
        } else {
            format!(
                "{} · {} · {}",
                definition.coverage, definition.quality_context, definition.timestamps
            )
        };
        if installed && !active {
            metadata.push_str(" · Installed");
        }
        let actions = div()
            .flex()
            .flex_none()
            .items_center()
            .gap_2()
            .child(state)
            .when_some(
                match deletion {
                    ModelDeletion::Unavailable => None,
                    ModelDeletion::Offered => Some(false),
                    ModelDeletion::Confirming => Some(true),
                },
                |actions, confirming| {
                    actions.child(
                        small_button(
                            if confirming {
                                ButtonKind::Danger
                            } else {
                                ButtonKind::Secondary
                            },
                            if confirming {
                                format!("Delete {}?", definition.size_label())
                            } else {
                                "Delete".into()
                            },
                        )
                        .id(("transcription-model-delete", index))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_transcription_model(definition.id, cx);
                            cx.notify();
                        })),
                    )
                },
            )
            .when_some(action, |actions, action| {
                actions.child(small_button(ButtonKind::Secondary, action))
            });
        div()
            .id(("transcription-model", index))
            .w_full()
            .px_4()
            .py_3()
            .flex()
            .flex_col()
            .gap_2()
            .rounded(RADIUS_PANEL)
            .border_1()
            .border_color(rgb(if active { ACCENT_BORDER } else { LINE }))
            .bg(rgb(if active { ACCENT_SURFACE } else { SURFACE }))
            .when(!active, |card| {
                card.cursor_pointer()
                    .hover(|card| card.bg(rgb(SURFACE_HOVER)))
            })
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_body()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(definition.name),
                            )
                            .child(
                                div()
                                    .text_caption()
                                    .text_color(rgb(TEXT_TERTIARY))
                                    .child(metadata),
                            ),
                    )
                    .child(actions),
            )
            .child(
                div()
                    .flex()
                    .gap_5()
                    .child(model_metric("Speed", definition.realtime))
                    .child(model_metric("Size", definition.size_label()))
                    .child(model_metric("WER", definition.quality)),
            )
            .when(definition.id == TranscriptionModelId::ParakeetV3, |card| {
                card.child(div().text_caption().text_color(rgb(TEXT_TERTIARY)).child(
                    "Detects language from audio, not the selected language. \
                             For language-guided transcription, try Whisper.",
                ))
            })
            .when_some(progress, |card, progress| {
                let indicator = match progress {
                    TranscriptionPickerProgress::Downloading(progress) => div()
                        .h_full()
                        .w(relative(progress))
                        .rounded(RADIUS_KNOB)
                        .bg(rgb(ACCENT)),
                    TranscriptionPickerProgress::Loading(progress) => div()
                        .ml(relative(progress * 0.75))
                        .h_full()
                        .w(relative(0.25))
                        .rounded(RADIUS_KNOB)
                        .bg(rgb(ACCENT)),
                };
                card.child(
                    div()
                        .h(px(PROGRESS_HEIGHT))
                        .w_full()
                        .rounded(RADIUS_KNOB)
                        .bg(rgb(CANVAS))
                        .child(indicator),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if preparing {
                    this.cancel_transcription_preparation();
                } else if !active {
                    this.choose_transcription_model(definition.id, language.clone(), cx);
                }
                cx.notify();
            }))
    });

    div()
        .id("transcription-picker-backdrop")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgba(SCRIM))
        .on_click(cx.listener(|this, _, _, cx| {
            this.dismiss_transcription_picker(cx);
        }))
        .child(
            div()
                .id("transcription-picker")
                .w(px(MODAL_WIDTH))
                .h(px(MODAL_HEIGHT))
                .flex()
                .flex_col()
                .rounded(RADIUS_MODAL)
                .border_1()
                .border_color(rgb(LINE))
                .bg(rgb(CANVAS))
                .overflow_hidden()
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .pl_5()
                        .pr_3()
                        .py_3()
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .child(div().text_headline().child("Local transcription"))
                        .child(
                            icon_button(ButtonKind::Ghost, Icon::Close)
                                .id("close-transcription-picker")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.dismiss_transcription_picker(cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .overflow_hidden()
                        .child(
                            div()
                                .id("transcription-language-list")
                                .w(px(SIDEBAR_WIDTH))
                                .h_full()
                                .p_3()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .overflow_y_scroll()
                                .bg(rgb(SIDEBAR))
                                .border_r_1()
                                .border_color(rgb(LINE))
                                .children(language_rows),
                        )
                        .child(
                            div()
                                .id("transcription-model-list")
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .p_4()
                                .flex()
                                .flex_col()
                                .gap_2()
                                .overflow_y_scroll()
                                .when_some(view.error, |list, error| {
                                    list.child(error_panel("Model could not be installed.", error))
                                })
                                .when_some(view.deletion_error, |list, error| {
                                    list.child(error_panel("Model could not be deleted.", error))
                                })
                                .children(model_cards),
                        ),
                ),
        )
        .into_any_element()
}

/// One inline statistic: a tertiary label beside its value.
fn model_metric(label: &'static str, value: impl IntoElement) -> gpui::Div {
    div()
        .flex()
        .items_baseline()
        .gap_1p5()
        .child(
            div()
                .text_caption()
                .text_color(rgb(TEXT_TERTIARY))
                .child(label),
        )
        .child(
            div()
                .text_footnote()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT_SECONDARY))
                .child(value),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_language_is_a_distinct_model_selection() {
        let model =
            crate::transcription_models::definition(TranscriptionModelId::WhisperLargeV3Turbo);
        let selection = TranscriptionSelection {
            model: model.id,
            language: crate::transcription_models::AUTO_LANGUAGE.into(),
            recognition_hints: String::new(),
        };

        assert!(transcription_selection_is_active(
            &selection,
            model,
            crate::transcription_models::AUTO_LANGUAGE,
            true,
        ));
        assert!(!transcription_selection_is_active(
            &selection, model, "en", true,
        ));
    }
}

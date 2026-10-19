// TODO(consolidate-screens): only the Windows shell uses this. macOS draws the
// same screen in the Modes pane in app_window.rs. Both now build on the shared
// design system from 75f859b (#138, "consolidate the desktop design system"),
// so converge them into one shared screen; see docs/plans/shared-desktop-ui.md.
//! Shared identity for the global fallback and optional custom mode editors.
//!
//! All three roots use the global and indexed targets for their concrete Modes
//! panes; later processing cards reuse the same identity without coupling it
//! to a platform settings schema.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ModeTarget {
    Global,
    Mode(usize),
}

impl ModeTarget {
    pub(crate) fn id_fragment(self) -> String {
        match self {
            Self::Global => "global".into(),
            Self::Mode(index) => format!("mode-{index}"),
        }
    }
}

//! Document commands. Physical bindings belong to the application keymap.

use gpui_kit as gpui;

gpui::actions!(
    kver,
    [
        NextPage,
        PreviousPage,
        ScrollDown,
        ScrollUp,
        ScrollLeft,
        ScrollRight,
        ScrollScreenDown,
        ScrollScreenUp,
        ScrollHalfScreenDown,
        ScrollHalfScreenUp,
        GoToDocumentStart,
        GoToDocumentEnd,
        ZoomIn,
        ZoomOut,
        FitWidth,
        FitPage,
        SinglePage,
        Continuous,
        UseDefaultReadingMode,
        ApplyMarkup,
        Highlight,
        Underline,
        StrikeOut,
        Squiggly,
        DeleteHighlight,
        UndoAnnotation,
        RedoAnnotation,
        Copy
    ]
);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub struct RecolorHighlight(pub crate::pdf::annotations::HighlightColor);

/// Choose the creation color without mutating the selected annotation.
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub(crate) struct SetMarkupColor(pub(crate) crate::pdf::annotations::HighlightColor);

pub(crate) fn markup_action(kind: crate::pdf::annotations::MarkupKind) -> Box<dyn gpui::Action> {
    use crate::pdf::annotations::MarkupKind;
    match kind {
        MarkupKind::Highlight => Box::new(Highlight),
        MarkupKind::Underline => Box::new(Underline),
        MarkupKind::StrikeOut => Box::new(StrikeOut),
        MarkupKind::Squiggly => Box::new(Squiggly),
    }
}

//! Document commands. Physical bindings belong to the application keymap.

use gpui_kit as gpui;

gpui::actions!(
    kver,
    [
        OpenPdf,
        ShowMarks,
        ShowSetMark,
        ShowJumpMark,
        ShowDeleteMark,
        NextPage,
        PreviousPage,
        ZoomIn,
        ZoomOut,
        FitWidth,
        FitPage,
        SinglePage,
        Continuous,
        UseDefaultReadingMode,
        Copy
    ]
);

#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub struct SetLocalMark(pub char);
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub struct JumpLocalMark(pub char);
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kver, no_json)]
pub struct DeleteLocalMark(pub char);

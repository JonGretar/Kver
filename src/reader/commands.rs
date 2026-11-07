//! Document commands. Physical bindings belong to the application keymap.

use gpui_kit as gpui;

gpui::actions!(
    kver,
    [
        NextPage,
        PreviousPage,
        ScrollDown,
        ScrollUp,
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

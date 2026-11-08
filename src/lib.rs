#[cfg(feature = "gui")]
mod about;
#[cfg(feature = "gui")]
pub mod app;
// Keep existing public imports working while application internals move.
#[cfg(feature = "gui")]
pub use app::{
    document_window::sidebar as reader_sidebar, menu as app_menu, windows as reader_windows,
};
#[cfg(feature = "gui")]
pub mod assets;
pub mod coordinates;
pub mod find_search;
pub mod pdf;
#[cfg(feature = "gui")]
pub mod reader;
pub mod reading_history;
pub use reading_history as recent;
pub mod settings;
#[cfg(feature = "gui")]
pub mod settings_ui;
#[cfg(all(feature = "gui", target_os = "macos"))]
mod updater;

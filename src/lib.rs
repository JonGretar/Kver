#[cfg(feature = "gui")]
mod about;
#[cfg(feature = "gui")]
pub mod app_menu;
#[cfg(feature = "gui")]
pub mod assets;
#[cfg(feature = "gui")]
mod contents_panel;
pub mod coordinates;
#[cfg(feature = "gui")]
mod find_panel;
pub mod find_search;
pub mod pdf;
#[cfg(feature = "gui")]
pub mod reader;
#[cfg(feature = "gui")]
pub mod reader_sidebar;
#[cfg(feature = "gui")]
pub mod reader_windows;
pub mod settings;
#[cfg(feature = "gui")]
pub mod settings_ui;
#[cfg(all(feature = "gui", target_os = "macos"))]
mod updater;

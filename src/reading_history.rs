//! Shared reading history: document identity, saved state, and session-only state.
//! Store and worker internals remain private behind stable feature interfaces.

mod store;
#[cfg(feature = "gui")]
mod worker;

#[cfg(feature = "gui")]
pub(crate) use store::LocalMark;
pub use store::{ReadingPosition, RecentFile, RecentStore};

#[cfg(feature = "gui")]
pub use worker::{history, install, notify_changed, revision};

#[cfg(feature = "gui")]
pub(crate) mod recent_picker;
#[cfg(feature = "gui")]
pub use recent_picker::ShowRecent;

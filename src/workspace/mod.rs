//! Filesystem concerns: the workspace root, dry-run isolation, change
//! detection, and durable `Execution` storage. No Git dependency anywhere
//! in this module (section 20/27).

pub mod isolate;
pub mod snapshot;
pub mod store;
pub mod story_archive;

pub use isolate::{IsolatedWorkspace, resolve_root};
pub use store::ExecutionStore;
pub use story_archive::{StoryArchive, StoryRecord};

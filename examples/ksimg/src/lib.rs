//! `ksimg`: a reference architecture for turning a user's intent into a
//! validated, deterministically executed image-processing workflow
//! without any agent generating the executable workflow directly.
//!
//! See `README.md` for the architecture; `src/main.rs` for a runnable
//! walkthrough.

pub mod agents;
pub mod domain;
pub mod runtime;
pub mod techniques;
pub mod workflow;

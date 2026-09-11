//! `kstest-core`: a small, generic, deterministic, continuously running
//! processing/execution runtime, controlled by an explicit state machine.
//!
//! No dependency on any LLM, agent, prompt, or external intelligence
//! service — the runtime is fully operational with none registered (see
//! `intelligence`). Such a provider may optionally be attached later as an
//! advisory layer; it is never a requirement for normal operation.
//!
//! See `README.md` for the architecture; `src/main.rs` for a runnable
//! walkthrough.

pub mod action;
pub mod capability;
pub mod context;
pub mod event;
pub mod intelligence;
pub mod lifecycle;
pub mod observability;
pub mod recovery;
pub mod resource;
pub mod runtime;
pub mod scheduler;
pub mod state;
pub mod state_machine;
pub mod validation;

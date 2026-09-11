//! `ksforge` — turns change requests into controlled, resumable Claude Code
//! workflows. See docs/03-architecture.md for the layering this crate
//! follows: `domain` (vocabulary) → `agent` (execution-engine port) →
//! `application` (the shared pipeline + one `Capability` impl per verb) →
//! `workspace`/`validation`/`github` (infrastructure) → `cli` (surface).

pub mod agent;
pub mod application;
pub mod cli;
pub mod domain;
pub mod github;
pub mod validation;
pub mod workspace;

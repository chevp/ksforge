//! Layer 1 of VALIDATE: exact, user-authored commands run after ACT
//! reports success, before ksforge trusts the result — deterministic and
//! opt-in. Layer 2 (the agent-driven check) is
//! `prompts/phases/validate.md` / `application::execute`, not here.

pub mod runner;

pub use runner::run;

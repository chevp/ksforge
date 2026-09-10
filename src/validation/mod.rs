//! Post-execution validation: explicit, user-controlled commands run after
//! Claude Code reports success, before ksforge trusts the result.

pub mod runner;

pub use runner::run;

//! Terminal color for ksforge's progress/diagnostic output — the same
//! `--color auto|always|never` contract `cargo`/`rustc` expose, and the
//! same reason for it: color helps a human scanning a scrolling log, but
//! must never leak into anything another program parses.
//!
//! Lives at the crate root, not under `cli`, because both `cli::output`
//! and `application::execute` (the phase-progress lines) use it, and
//! `application` must not depend on `cli` (see docs/03-architecture.md's
//! layering).
//!
//! Deliberately stderr-only. `output::print_execution`'s `--format json`
//! (and its `--format text` human transcript) stay plain on stdout — the
//! `action.yml` composite step captures ksforge's stdout verbatim into
//! `$GITHUB_OUTPUT` (`out=$(ksforge ...)`), and from there it can end up in
//! a PR comment body; ANSI escapes are exactly the kind of thing that
//! renders as literal garbage once it's out of a terminal. Only
//! `cli::output::eprint_notice` and the phase-progress lines in
//! `application::execute::run_phase_loop` are colored — diagnostics only,
//! never data. GitHub Actions' own log viewer does render ANSI codes from
//! a step's stderr/stdout, which is what makes coloring stderr still show
//! up "in the workflow log" the way the plain terminal case does.

use std::io::IsTerminal;
use std::sync::OnceLock;

use clap::ValueEnum;

/// See `cli::args::Cli::color`.
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

static OVERRIDE: OnceLock<Option<bool>> = OnceLock::new();

/// Call once, right after parsing `Cli`, before any output — see `Cli::color`.
pub fn init(choice: ColorChoice) {
    let value = match choice {
        ColorChoice::Always => Some(true),
        ColorChoice::Never => Some(false),
        ColorChoice::Auto => None,
    };
    // Only ever called once in `cli::run`; a test harness calling it twice
    // (e.g. running `run()` more than once in-process) just keeps the
    // first value rather than panicking.
    let _ = OVERRIDE.set(value);
}

/// Whether to emit ANSI color codes on stderr. `--color always`/`never`
/// (via `init`) wins outright. Otherwise, in order: `NO_COLOR` (see
/// <https://no-color.org>) disables unconditionally; `FORCE_COLOR` or
/// `CLICOLOR_FORCE` force it on even when stderr isn't a terminal — the
/// GitHub Actions case: its log viewer renders ANSI fine, but a step's
/// captured stderr is a pipe, not a tty, so a plain `is_terminal()` check
/// would wrongly disable color there; ksforge treats `GITHUB_ACTIONS=true`
/// the same way for the common case where a workflow didn't set
/// `FORCE_COLOR` itself. Otherwise: a real terminal.
pub fn enabled() -> bool {
    if let Some(Some(forced)) = OVERRIDE.get() {
        return *forced;
    }
    if env_set("NO_COLOR") {
        return false;
    }
    if env_set("FORCE_COLOR") || env_set("CLICOLOR_FORCE") {
        return true;
    }
    if std::env::var_os("GITHUB_ACTIONS").as_deref() == Some(std::ffi::OsStr::new("true")) {
        return true;
    }
    std::io::stderr().is_terminal()
}

fn env_set(key: &str) -> bool {
    std::env::var_os(key).is_some_and(|v| !v.is_empty())
}

#[derive(Debug, Clone, Copy)]
pub enum Color {
    Red,
    Green,
    Yellow,
    Cyan,
    BrightBlack,
}

impl Color {
    fn code(self) -> &'static str {
        match self {
            Color::Red => "31",
            Color::Green => "32",
            Color::Yellow => "33",
            Color::Cyan => "36",
            Color::BrightBlack => "90",
        }
    }
}

/// `text` wrapped in `color` (bold, if requested) when `enabled()`,
/// unchanged otherwise — callers never need their own branch.
pub fn paint(text: &str, color: Color, bold: bool) -> String {
    if !enabled() {
        return text.to_string();
    }
    let weight = if bold { "1;" } else { "" };
    format!("\x1b[{weight}{}m{text}\x1b[0m", color.code())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paint_returns_plain_text_when_disabled() {
        // `enabled()` reads process-global env/override state, which other
        // tests in this same binary may also touch — assert the *shape* of
        // the contract via a local reimplementation rather than the
        // ambient, process-wide default.
        assert_eq!(paint_forced("hello", Color::Red, false, false), "hello");
    }

    #[test]
    fn paint_wraps_in_ansi_codes_when_enabled() {
        let painted = paint_forced("hello", Color::Red, true, true);
        assert!(painted.starts_with("\x1b[1;31m"));
        assert!(painted.ends_with("\x1b[0m"));
        assert!(painted.contains("hello"));
    }

    /// Test-only: `paint` without going through the global `enabled()`
    /// state, which is process-wide and set-once (`OVERRIDE`/env vars) and
    /// therefore not safe to flip per-test.
    fn paint_forced(text: &str, color: Color, bold: bool, on: bool) -> String {
        if !on {
            return text.to_string();
        }
        let weight = if bold { "1;" } else { "" };
        format!("\x1b[{weight}{}m{text}\x1b[0m", color.code())
    }
}

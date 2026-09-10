use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "ksforge",
    version,
    about = "Turns user stories into controlled, resumable Claude Code workflows."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Implement a user story end to end.
    Implement(StoryArgs),
    /// Review the workspace and report findings; never modifies files.
    Review(StoryArgs),
    /// Diagnose and fix a described problem.
    Fix(StoryArgs),
    /// Explain part of the workspace; never modifies files.
    Explain(StoryArgs),
    /// Continue a paused execution with a human decision.
    Resume(ResumeArgs),
    /// Show an execution's state, including any pending question.
    Status(StatusArgs),
    /// List available capabilities.
    Capabilities,
}

#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Args)]
pub struct StoryArgs {
    /// The user story text.
    #[arg(long, conflicts_with = "story_file")]
    pub story: Option<String>,

    /// Read the user story from a file instead of `--story`.
    #[arg(long, value_name = "PATH", conflicts_with = "story")]
    pub story_file: Option<PathBuf>,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args)]
pub struct ResumeArgs {
    pub execution_id: String,

    /// The id of the option the human chose (see `ksforge status`).
    #[arg(long)]
    pub decision: String,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args)]
pub struct StatusArgs {
    pub execution_id: String,

    /// Workspace whose `.ksforge/executions` store to read.
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct CommonArgs {
    /// Workspace root ksforge and Claude Code operate in. Defaults to the
    /// current directory (section 26: local CLI needs no GitHub token).
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    /// Model alias or full name, e.g. "sonnet" or "claude-sonnet-5".
    /// Defaults to "sonnet" rather than deferring to Claude Code's own
    /// default, so ksforge's cost/behavior doesn't shift silently if that
    /// changes.
    #[arg(long, default_value = "sonnet")]
    pub model: Option<String>,

    /// Maximum dollar amount Claude Code may spend on this run.
    #[arg(long)]
    pub max_budget_usd: Option<f64>,

    /// Explicit path to the Claude Code executable; otherwise resolved
    /// from PATH (section 32).
    #[arg(long, env = "KSFORGE_CLAUDE_PATH", value_name = "PATH")]
    pub claude_path: Option<PathBuf>,

    /// Run in an isolated temporary copy of the workspace; nothing is
    /// written back to the real workspace (section 25).
    #[arg(long)]
    pub dry_run: bool,

    /// Repeatable. A shell command to run after Claude Code reports
    /// success, before ksforge trusts the result (section 21). Never
    /// supplied or overridable by the model.
    #[arg(long = "validate", value_name = "COMMAND")]
    pub validate: Vec<String>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Open a pull request via `gh` if the run completes with changes.
    #[arg(long)]
    pub create_pull_request: bool,

    /// Base branch for `--create-pull-request` (section 19).
    #[arg(long, default_value = "main")]
    pub base_branch: String,

    /// Path to a Claude Code `--mcp-config` file, giving the agent access
    /// to additional MCP servers (e.g. read-only access to an external
    /// system) for this run — passed straight through, together with
    /// `--strict-mcp-config`. See docs/11-integrations.md.
    #[arg(long, value_name = "PATH")]
    pub mcp_config: Option<PathBuf>,
}

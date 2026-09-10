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
    /// Stop an execution that will not be resumed.
    Cancel(CancelArgs),
    /// Render an execution's progress report and post/update it as a PR
    /// comment via `gh` (section 13-15).
    PostReport(PostReportArgs),
    /// Validate a `/ksforge choose <option>` PR/issue comment against a
    /// paused execution's open gate and print the option id to act on —
    /// does not itself resume (section 16-17). Chain with `ksforge resume`.
    HandleComment(HandleCommentArgs),
    /// List available capabilities.
    Capabilities,
}

#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

/// Which coding agent CLI ksforge spawns for this run.
#[derive(ValueEnum, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Engine {
    #[default]
    Claude,
    Codex,
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

    /// Who decided — a GitHub login when resuming from a `/ksforge choose`
    /// comment (see `ksforge handle-comment`). Omitted for a plain local
    /// resume.
    #[arg(long)]
    pub decided_by: Option<String>,

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
pub struct CancelArgs {
    pub execution_id: String,

    /// Why the execution is being cancelled — recorded on the execution.
    #[arg(long, default_value = "cancelled by operator")]
    pub reason: String,

    /// Workspace whose `.ksforge/executions` store to read/write.
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

#[derive(Args)]
pub struct PostReportArgs {
    pub execution_id: String,

    /// Pull request number to post/update the report comment on.
    #[arg(long)]
    pub pr: u64,

    /// Workspace whose `.ksforge/executions` store to read, and whose `gh`
    /// repo context to post into.
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,
}

#[derive(Args)]
pub struct HandleCommentArgs {
    /// The execution the comment is replying to.
    #[arg(long)]
    pub execution_id: String,

    /// The GitHub comment's own numeric id — the idempotency key (section
    /// 17): the same comment delivered twice must not be acted on twice.
    #[arg(long)]
    pub comment_id: String,

    /// The commenter's GitHub login. Re-verified against the repository's
    /// own permission list before the decision is accepted (section 23) —
    /// never trusted just because a workflow's `if:` already checked it.
    #[arg(long)]
    pub commenter: String,

    /// The raw comment body, e.g. `/ksforge choose oauth2`.
    #[arg(long)]
    pub body: String,

    /// Workspace whose `.ksforge/executions` store to read, and whose `gh`
    /// repo context to check permissions against.
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,
}

#[derive(Args)]
pub struct CommonArgs {
    /// Workspace root ksforge and Claude Code operate in. Defaults to the
    /// current directory (section 26: local CLI needs no GitHub token).
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    /// Which coding agent CLI to spawn. "codex" cannot honor
    /// `--mcp-config`/`--max-budget-usd` (no Codex CLI equivalent) — passing
    /// either with `--engine codex` fails the run rather than silently
    /// dropping them.
    #[arg(long, value_enum, default_value_t = Engine::Claude)]
    pub engine: Engine,

    /// Model alias or full name, e.g. "sonnet"/"claude-sonnet-5" for
    /// `--engine claude`, or a Codex model name for `--engine codex`. With
    /// `--engine claude` and no explicit value, defaults to "sonnet" rather
    /// than deferring to Claude Code's own default, so ksforge's
    /// cost/behavior doesn't shift silently if that changes; with
    /// `--engine codex`, an unset value defers to Codex's own configured
    /// default instead of wrongly passing it the Claude alias "sonnet".
    #[arg(long)]
    pub model: Option<String>,

    /// Maximum dollar amount the agent may spend on this run. Claude Code
    /// only; the Codex CLI has no equivalent flag.
    #[arg(long)]
    pub max_budget_usd: Option<f64>,

    /// Explicit path to the Claude Code executable; otherwise resolved
    /// from PATH (section 32).
    #[arg(long, env = "KSFORGE_CLAUDE_PATH", value_name = "PATH")]
    pub claude_path: Option<PathBuf>,

    /// Explicit path to the Codex CLI executable; otherwise resolved from
    /// PATH. Only used with `--engine codex`.
    #[arg(long, env = "KSFORGE_CODEX_PATH", value_name = "PATH")]
    pub codex_path: Option<PathBuf>,

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

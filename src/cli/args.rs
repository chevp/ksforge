use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "ksforge",
    version,
    about = "Turns change requests into controlled, resumable Claude Code workflows."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Implement a change request end to end.
    Implement(ChangeRequestArgs),
    /// Review the workspace and report findings; never modifies files.
    Review(ChangeRequestArgs),
    /// Diagnose and fix a described problem.
    Fix(ChangeRequestArgs),
    /// Explain part of the workspace; never modifies files.
    Explain(ChangeRequestArgs),
    /// Continue a paused execution with a human decision.
    Resume(ResumeArgs),
    /// Show an execution's state, including any pending question.
    Status(StatusArgs),
    /// Stop an execution that will not be resumed.
    Cancel(CancelArgs),
    /// Render an execution's progress report and post/update it as a PR
    /// comment via `gh` (section 13-15).
    PostReport(PostReportArgs),
    /// Classify a PR/issue comment: `/ksforge choose <option>` against a
    /// paused execution's open gate (prints the option id — chain with
    /// `ksforge resume`), or `/ksforge implement|fix <change-request>` as a new
    /// follow-up run (prints the capability and change request — chain with
    /// `ksforge implement`/`ksforge fix --push-to-branch`). Never itself
    /// starts or resumes anything (section 16-17).
    HandleComment(HandleCommentArgs),
    /// Analyze a new change request against the workspace's other active
    /// executions and report overlap/conflict risk — never implements
    /// anything itself (see prompts/coordinator/system-prompt.md).
    Coordinate(CoordinateArgs),
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
pub struct ChangeRequestArgs {
    /// The change request text.
    #[arg(long, conflicts_with = "change_request_file")]
    pub change_request: Option<String>,

    /// Read the change request from a file instead of `--change-request`.
    #[arg(long, value_name = "PATH", conflicts_with = "change_request")]
    pub change_request_file: Option<PathBuf>,

    #[command(flatten)]
    pub common: CommonArgs,
}

/// Deliberately narrower than `ChangeRequestArgs` — `coordinate` never
/// writes, so it has no `--dry-run`/`--validate`/`--create-pull-request`/
/// `--push-to-branch`/`--mcp-config` to expose.
#[derive(Args)]
pub struct CoordinateArgs {
    /// The change request text.
    #[arg(long, conflicts_with = "change_request_file")]
    pub change_request: Option<String>,

    /// Read the change request from a file instead of `--change-request`.
    #[arg(long, value_name = "PATH", conflicts_with = "change_request")]
    pub change_request_file: Option<PathBuf>,

    /// Workspace whose active executions to analyze against.
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    /// Which coding agent CLI to spawn.
    #[arg(long, value_enum, default_value_t = Engine::Claude)]
    pub engine: Engine,

    /// Model alias or full name — see `CommonArgs::model`'s doc comment
    /// for the same `--engine`-dependent default behavior.
    #[arg(long)]
    pub model: Option<String>,

    /// Maximum dollar amount the agent may spend on this analysis.
    #[arg(long)]
    pub max_budget_usd: Option<f64>,

    #[arg(long, env = "KSFORGE_CLAUDE_PATH", value_name = "PATH")]
    pub claude_path: Option<PathBuf>,

    #[arg(long, env = "KSFORGE_CODEX_PATH", value_name = "PATH")]
    pub codex_path: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
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
    /// The execution the comment is replying to. Required for a
    /// `/ksforge choose <option>` reply to a pending decision; omit for a
    /// `/ksforge implement|fix <change-request>` follow-up, which has no execution
    /// yet — start one with a normal `ksforge implement`/`ksforge fix`
    /// call using this command's output (section: does not itself act,
    /// same as the choose-flow — chain the two).
    #[arg(long)]
    pub execution_id: Option<String>,

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
    #[arg(long, conflicts_with = "push_to_branch")]
    pub create_pull_request: bool,

    /// Base branch for `--create-pull-request` (section 19).
    #[arg(long, default_value = "main")]
    pub base_branch: String,

    /// Commit and push changes directly to the already-checked-out branch
    /// instead of opening a new PR — for a PR follow-up run (`ksforge
    /// handle-comment` classified a `/ksforge implement|fix <change-request>`
    /// comment): the workspace is already a checkout of that PR's own
    /// head branch, so this updates the existing PR rather than opening
    /// a new one the way `--create-pull-request` would. See
    /// docs/07-github-actions.md.
    #[arg(long, conflicts_with = "create_pull_request")]
    pub push_to_branch: bool,

    /// Path to a Claude Code `--mcp-config` file, giving the agent access
    /// to additional MCP servers (e.g. read-only access to an external
    /// system) for this run — passed straight through, together with
    /// `--strict-mcp-config`. See docs/11-integrations.md.
    #[arg(long, value_name = "PATH")]
    pub mcp_config: Option<PathBuf>,
}

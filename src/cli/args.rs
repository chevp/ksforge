use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::color::ColorChoice;

#[derive(Parser)]
#[command(
    name = "ksforge",
    version,
    about = "Turns change requests into controlled, resumable Claude Code workflows."
)]
pub struct Cli {
    /// `None` means `ksforge` was invoked with no subcommand at all — the
    /// only case that carries meaning of its own: it starts a `chat`
    /// session with every flag at its default (docs/12-interactive-chat.md).
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Colorize progress/diagnostic output on stderr.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start an interactive chat session. Also what bare `ksforge` runs.
    Chat(ChatArgs),
    /// Implement a change request end to end.
    Implement(ChangeRequestArgs),
    /// Review the workspace and report findings; never modifies files.
    Review(ChangeRequestArgs),
    /// Diagnose and fix a described problem.
    Fix(ChangeRequestArgs),
    /// Explain part of the workspace; never modifies files.
    Explain(ChangeRequestArgs),
    /// Generate an image from a text prompt (stub, no real backend yet).
    Txt2img(ChangeRequestArgs),
    /// Add or update tests only; rejects non-test file changes.
    Test(ChangeRequestArgs),
    /// Continue a paused execution with a human decision.
    Resume(ResumeArgs),
    /// Show an execution's state, including any pending question.
    Status(StatusArgs),
    /// Stop an execution that will not be resumed.
    Cancel(CancelArgs),
    /// Post/update an execution's progress report as a PR comment via `gh`.
    PostReport(PostReportArgs),
    /// Classify a PR/issue comment as a decision or a new follow-up run. Never acts itself.
    HandleComment(HandleCommentArgs),
    /// Check a change request against other active executions for conflicts. Never implements.
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

#[derive(Args)]
pub struct ChangeRequestArgs {
    /// The change request text, or `@<path>` to read it from a file instead.
    #[arg(long = "change")]
    pub change_request: Option<String>,

    #[command(flatten)]
    pub common: CommonArgs,
}

/// Deliberately narrower than `ChangeRequestArgs` — `coordinate` never
/// writes, so it has no `--dry-run`/`--validate`/`--create-pull-request`/
/// `--push-to-branch`/`--mcp-config` to expose.
#[derive(Args)]
pub struct CoordinateArgs {
    /// The change request text, or `@<path>` to read it from a file instead.
    #[arg(long = "change")]
    pub change_request: Option<String>,

    /// Workspace whose active executions to analyze against.
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    /// Model alias or full name — see `CommonArgs::model`'s doc comment for
    /// the default-value behavior.
    #[arg(long)]
    pub model: Option<String>,

    /// Maximum dollar amount the agent may spend on this analysis.
    #[arg(long)]
    pub max_budget_usd: Option<f64>,

    #[arg(long, env = "KSFORGE_CLAUDE_PATH", value_name = "PATH")]
    pub claude_path: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

/// A subset of `CommonArgs` (docs/12-interactive-chat.md): chat has exactly
/// one output mode (a text transcript) and exactly one merge-back mechanism
/// (local), so `--format`/`--create-pull-request`/`--push-to-branch` are not
/// exposed here — GitHub/`gh` are not involved in chat at all.
#[derive(Args)]
pub struct ChatArgs {
    /// Workspace root; must be a git repository.
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    /// Model alias or full name — same default behavior as
    /// `CommonArgs::model`.
    #[arg(long)]
    pub model: Option<String>,

    /// Maximum dollar amount the agent may spend on a single chat turn, not
    /// the whole session.
    #[arg(long)]
    pub max_budget_usd: Option<f64>,

    #[arg(long, env = "KSFORGE_CLAUDE_PATH", value_name = "PATH")]
    pub claude_path: Option<PathBuf>,

    /// Run every turn in an isolated temporary copy of the workspace.
    #[arg(long)]
    pub dry_run: bool,

    /// Repeatable; exact commands to run after ACT, in addition to the
    /// agent-driven VALIDATE turn every run gets by default. Applied to
    /// every turn, same as the other capabilities.
    #[arg(long = "validate", value_name = "COMMAND")]
    pub validate: Vec<String>,

    /// Skip the agent-driven VALIDATE turn (but not `--validate` commands,
    /// which still run if given).
    #[arg(long)]
    pub no_validate: bool,

    /// The branch chat merges completed changes back into, after
    /// confirmation.
    #[arg(long, default_value = "main")]
    pub base_branch: String,

    #[arg(long, value_name = "PATH")]
    pub mcp_config: Option<PathBuf>,
}

/// Built when `ksforge` is invoked with no subcommand at all — bypasses
/// clap parsing entirely, so `KSFORGE_CLAUDE_PATH` is read directly here to
/// keep that one behavior clap's `env` attribute would otherwise provide.
impl Default for ChatArgs {
    fn default() -> Self {
        Self {
            workspace: PathBuf::from("."),
            model: None,
            max_budget_usd: None,
            claude_path: std::env::var_os("KSFORGE_CLAUDE_PATH").map(PathBuf::from),
            dry_run: false,
            validate: Vec::new(),
            no_validate: false,
            base_branch: "main".to_string(),
            mcp_config: None,
        }
    }
}

#[derive(Args)]
pub struct ResumeArgs {
    pub execution_id: String,

    /// The id of the option the human chose (see `ksforge status`).
    #[arg(long)]
    pub decision: String,

    /// Who decided; a GitHub login when resuming via a PR comment.
    #[arg(long)]
    pub decided_by: Option<String>,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Args)]
pub struct StatusArgs {
    /// Execution to show. Omit for a workspace-wide overview instead: every
    /// git repo found under `--workspace` plus each one's active
    /// executions (see docs/04-cli-reference.md).
    pub execution_id: Option<String>,

    /// Workspace whose `.ksforge/executions` store to read — or, with no
    /// `execution_id`, the root to discover repos under.
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
    /// The execution the comment is replying to; omit for a new follow-up run.
    #[arg(long)]
    pub execution_id: Option<String>,

    /// The GitHub comment's numeric id; prevents double-processing.
    #[arg(long)]
    pub comment_id: String,

    /// The commenter's GitHub login; re-verified against repo permissions.
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
    /// Workspace root ksforge and Claude Code operate in.
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,

    /// Model alias or full name. Defaults to "sonnet".
    #[arg(long)]
    pub model: Option<String>,

    /// Maximum dollar amount the agent may spend on this run.
    #[arg(long)]
    pub max_budget_usd: Option<f64>,

    /// Explicit path to the Claude Code executable; otherwise resolved from PATH.
    #[arg(long, env = "KSFORGE_CLAUDE_PATH", value_name = "PATH")]
    pub claude_path: Option<PathBuf>,

    /// Run in an isolated temporary copy of the workspace; nothing is written back.
    #[arg(long)]
    pub dry_run: bool,

    /// Repeatable. An exact shell command to run after ACT reports success,
    /// in addition to (not instead of) the agent-driven VALIDATE turn every
    /// run gets by default — that turn works out what actually needs
    /// checking for this specific change and does it, rather than ksforge
    /// hardcoding a per-project-type command. Use `--no-validate` to skip
    /// just that turn; commands passed here still run either way.
    #[arg(long = "validate", value_name = "COMMAND")]
    pub validate: Vec<String>,

    /// Skip the agent-driven VALIDATE turn (but not `--validate` commands,
    /// which still run if given).
    #[arg(long)]
    pub no_validate: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    /// Open a pull request via `gh` if the run completes with changes.
    #[arg(long, conflicts_with = "push_to_branch")]
    pub create_pull_request: bool,

    /// Base branch for `--create-pull-request`.
    #[arg(long, default_value = "main")]
    pub base_branch: String,

    /// Commit and push directly to the checked-out branch instead of
    /// opening a new PR.
    #[arg(long, conflicts_with = "create_pull_request")]
    pub push_to_branch: bool,

    /// Path to a Claude Code `--mcp-config` file, passed straight through.
    #[arg(long, value_name = "PATH")]
    pub mcp_config: Option<PathBuf>,
}

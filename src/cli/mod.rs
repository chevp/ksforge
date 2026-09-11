//! CLI surface: argument parsing (`args`), dispatch (`commands`), the
//! interactive front end (`chat`), and output formatting (`output`). `run`
//! is the only thing `main.rs` calls.

pub mod args;
pub mod chat;
pub mod commands;
pub mod output;

use clap::Parser;

use crate::color::{self, Color};
use args::{ChatArgs, Cli, Command};

/// Parse args, run the requested command, and return the process exit code
/// (§pewY5yG of the base spec — never 0 on a failed operation). No
/// subcommand at all (`cli.command` is `None`) means bare `ksforge`, which
/// is equivalent to `ksforge chat` with every flag at its default
/// (docs/12-interactive-chat.md) — the only command that changes meaning
/// when no subcommand is given.
pub async fn run() -> i32 {
    let cli = Cli::parse();
    color::init(cli.color);
    let result = match cli.command {
        None => chat::run(ChatArgs::default()).await,
        Some(Command::Chat(args)) => chat::run(args).await,
        Some(Command::Implement(args)) => {
            commands::change_request_capability("implement", args).await
        }
        Some(Command::Review(args)) => commands::change_request_capability("review", args).await,
        Some(Command::Fix(args)) => commands::change_request_capability("fix", args).await,
        Some(Command::Explain(args)) => commands::change_request_capability("explain", args).await,
        Some(Command::Txt2img(args)) => commands::change_request_capability("txt2img", args).await,
        Some(Command::Img2img(args)) => commands::change_request_capability("img2img", args).await,
        Some(Command::Test(args)) => commands::change_request_capability("test", args).await,
        Some(Command::Resume(args)) => commands::resume(args).await,
        Some(Command::Status(args)) => commands::status(args).await,
        Some(Command::Cancel(args)) => commands::cancel(args),
        Some(Command::PostReport(args)) => commands::post_report(args).await,
        Some(Command::HandleComment(args)) => commands::handle_comment(args).await,
        Some(Command::Coordinate(args)) => commands::coordinate(args).await,
        Some(Command::Capabilities) => Ok(commands::capabilities()),
    };

    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{} {e}", color::paint("ksforge:", Color::Red, true));
            e.exit_code()
        }
    }
}

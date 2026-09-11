//! CLI surface: argument parsing (`args`), dispatch (`commands`), and
//! output formatting (`output`). `run` is the only thing `main.rs` calls.

pub mod args;
pub mod commands;
pub mod output;

use clap::Parser;

use args::{Cli, Command};

/// Parse args, run the requested command, and return the process exit code
/// (section 16 of the base spec — never 0 on a failed operation).
pub async fn run() -> i32 {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Implement(args) => commands::change_request_capability("implement", args).await,
        Command::Review(args) => commands::change_request_capability("review", args).await,
        Command::Fix(args) => commands::change_request_capability("fix", args).await,
        Command::Explain(args) => commands::change_request_capability("explain", args).await,
        Command::Resume(args) => commands::resume(args).await,
        Command::Status(args) => commands::status(args),
        Command::Cancel(args) => commands::cancel(args),
        Command::PostReport(args) => commands::post_report(args).await,
        Command::HandleComment(args) => commands::handle_comment(args).await,
        Command::Coordinate(args) => commands::coordinate(args).await,
        Command::Capabilities => Ok(commands::capabilities()),
    };

    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("ksforge: {e}");
            e.exit_code()
        }
    }
}

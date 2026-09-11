# Interactive chat

**Status: implemented.** `ksforge chat` (and bare `ksforge`) is live —
`src/cli/chat.rs`, plus `github::commit_to_new_branch`/
`github::merge_branch_into` in `src/github/pull_request.rs` for the local
merge-back. This document is the reference for how it behaves; see
[04-cli-reference.md](04-cli-reference.md) for the option table.

## Motivation

Run `ksforge` (no subcommand) inside a local git repository and get a
conversational, natural-language front end over the existing capability
pipeline instead of one `implement`/`review`/`fix`/`explain` invocation
per shell command — including branching, committing, and merging back to
`main`, driven from the same chat feed.

Chat is a conversational front end over the same capability pipeline every
other command uses: every chat turn is one ordinary, resumable `Execution`
going through the same `application::execute::run` pipeline as
`ksforge implement` — same prompts, same validation, same persistence,
just driven from a REPL instead of one invocation per shell command.

## Invocation

```text
ksforge                    # starts the interactive session with defaults
ksforge chat [options]     # same session, explicit subcommand + flags
```

`ksforge` with no subcommand is equivalent to `ksforge chat` with every
flag at its default. This is the only command that changes meaning when
no subcommand is given — every other verb (`implement`, `review`, ...)
keeps requiring its subcommand explicitly.

### Flags

A subset of the flags shared by `implement`/`review`/`fix`/`explain`
today (see `CommonArgs` in `04-cli-reference.md`):

| Flag | Default | Meaning |
|---|---|---|
| `--workspace <path>` | `.` | Workspace root. Any directory — a single repo, a plain non-git folder, or a folder holding several independent repos (each changed file is attributed to its own nearest repo at merge-back time, see step 4 below). |
| `--model <name>` | `sonnet` | Same as today. |
| `--max-budget-usd <n>` | none | Same as today. Applies per chat turn, not to the whole session. |
| `--claude-path <path>` | PATH lookup | Same as today. |
| `--dry-run` | off | Same isolation as today — see [03-architecture.md](03-architecture.md). With `--dry-run`, nothing is ever branched/committed/merged either, since there is no real workspace to touch. |
| `--validate <cmd>` | none | Repeatable. Same as today, applied to every turn. |
| `--base-branch <name>` | `main` | The branch chat merges completed changes back into, after confirmation. |
| `--mcp-config <path>` | none | Same as today. |

Deliberately **not** carried over: `--create-pull-request`,
`--push-to-branch`, `--format`. Chat has exactly one output mode (a text
transcript on stdout) and exactly one merge-back mechanism (local, see
below) — GitHub/`gh` are not involved at all.

## Session flow

```text
$ ksforge
ksforge chat — describe a change in plain language.
Prefix with 'review:', 'fix:', or 'explain:' to pick a capability other
than the default ('implement'). Type 'exit' to quit.

ksforge> add a "forgot password" link to the login page
Starte implement...
...
Changed:
  src/auth.rs
  templates/login.html
...
Auf Branch 'ksforge/2f8b6a1c9d3e' committet.
Nach 'main' mergen? [y/N] y
Gemergt nach 'main', Branch 'ksforge/2f8b6a1c9d3e' gelöscht.

ksforge> exit
```

1. **Capability routing.** Free text with no recognized prefix runs
   `implement`. A leading `review:`, `fix:`, or `explain:` (case
   insensitive, before the first `:`) runs that capability instead with
   the remaining text as the change request. This is string routing, not
   an LLM classification step — kept intentionally simple and
   deterministic.
2. **One turn, one `Execution`.** Each chat line that isn't a control
   command (`exit`/`quit`) starts a normal `Execution` via the existing
   pipeline — same prompt construction, same `--validate` handling, same
   `.ksforge/executions/<id>/` persistence as `ksforge implement` today.
   Nothing about the pipeline itself is chat-specific.
3. **Waiting-for-human gates inline.** If a turn's `Execution` pauses with
   `status: waiting_for_human`, chat prints the question/options (the same
   text `output::print_human` renders today) and treats the *next* line of
   input as the decision, resuming that execution via the existing
   `application::resume::resume` — no separate `ksforge resume` shell
   command needed inside a chat session. A line that doesn't match any
   offered option id is rejected with an error and the gate stays open.
4. **Local branch, commit — automatic, once per repo touched.** Once a
   turn's `Execution` reaches `status: completed` with a non-empty
   `changed_files`, chat groups those files by the git repo that actually
   owns each one (`github::repo::group_by_repo` — nearest `.git` above
   each file, never searching above `--workspace`) and, for each repo,
   immediately creates a new local branch (`ksforge/<short execution id>`,
   the same naming `github::create_from_execution` already uses) and
   commits that repo's changed files onto it — unattended, no
   confirmation. This step never touches a remote and never calls `gh`. A
   changed file with no `.git` anywhere above it (a plain non-git
   `--workspace` folder) is reported, not silently dropped or an error —
   the turn itself already completed successfully.
5. **Merge back — confirmed, per repo.** For each repo branched/committed
   in step 4, chat asks once, in the same feed:
   `'<branch>' nach '<base-branch>' mergen? [y/N]`. Only `y`/`yes`/`j`/`ja`
   (case-insensitive) proceeds. On confirmation: checkout
   `--base-branch`, `git merge --no-ff <branch>`, delete `<branch>` — all
   inside that repo. On anything else: nothing further happens for that
   repo — the commit stays on the feature branch, which is left checked
   out, so no work is lost and the user can inspect or discard it manually
   before the next chat turn. Other touched repos are asked about
   independently, in the same order they were committed.
6. **Capabilities with no changes** (`review`, `explain`, or a turn that
   made no edits) skip steps 4-5 entirely — same "nothing to open a PR
   for" semantics `create_from_execution` already has today, just without
   opening anything.
7. **Errors don't end the session.** A failed turn (agent error,
   validation failure, git command failure) is reported the same way
   `ksforge implement`'s own error path reports it, and the loop continues
   waiting for the next line — one bad turn should not force restarting
   `ksforge`.
8. **Exit.** `exit`, `quit`, or EOF (Ctrl-D on a POSIX shell, Ctrl-Z
   Enter on `cmd.exe`/PowerShell) ends the session with exit code `0`.

## Explicitly out of scope

- **No push, no PR, no `gh`.** The merge-back target is always the local
  `--base-branch` in the same working copy. Publishing that branch
  (`git push`) remains a manual step after leaving chat — pushing to a
  shared remote is exactly the kind of hard-to-reverse, shared-state
  action `ksforge`'s existing PR flow already gates behind an explicit
  `--create-pull-request`/`gh pr create`, and chat does not get a
  quieter path to the same effect.
- **No auto-merge without confirmation**, regardless of any future
  "non-interactive" flag — the confirmation in step 5 is not a
  convenience prompt to be flagged away, it is the safety boundary
  between "ksforge edited files" and "ksforge changed what `main` points
  at."
- **No chat history/session persistence beyond `Execution` itself.** Each
  turn's `Execution` is durable exactly as it is today
  (`ksforge status <id>` works on it after the fact); the chat loop's own
  in-memory state (which execution is pending a decision) is not — closing
  the terminal mid-gate loses only the chat prompt convenience, not the
  paused execution, which can still be resumed the normal way:
  `ksforge resume <id> --decision <option-id>`.
- **No natural-language intent classification.** Capability selection is
  the fixed `review:`/`fix:`/`explain:` prefix convention in step 1, not
  a model call — keeps routing deterministic and free.

## Impact on existing modules

Per [03-architecture.md](03-architecture.md)'s layering, this added:

- `cli`: a new `chat` submodule (`src/cli/chat.rs`: the REPL loop,
  `route`, and the local branch/merge confirmation), plus `ChatArgs` in
  `args.rs` and `Command::Chat`/`cli.command: Option<Command>` so bare
  `ksforge` resolves to `Chat(ChatArgs::default())`.
- `github`: `commit_to_new_branch`/`merge_branch_into` next to
  `create_from_execution`/`push_follow_up` in `pull_request.rs` — branches
  and commits exactly like `create_from_execution` does, but merges into
  `--base-branch` locally instead of pushing and calling `gh pr create`.
  Still the only module that runs `git`, keeping "Git/GitHub are
  infrastructure, never domain" intact — a third git-based outcome
  alongside the two that already exist, at the same layer.

`domain`, `agent`, `application`, `validation`, and the `Execution` state
machine stayed exactly as they were — chat drives the existing pipeline
through a new front end.

# Human-in-the-loop execution

## Why this exists

A GitHub Actions job is a bad place to block on a human. If Claude Code
hits a genuine fork in the road — "OAuth2 or JWT?" — the workflow
shouldn't sit there waiting; it should pause cleanly, tell someone what it
needs, and let a *separate* run pick the decision back up whenever it
arrives, possibly days later. That's the whole reason `Execution` exists as
a durable, resumable object instead of `implement` just being one blocking
function call.

```text
Workflow run 1                          Workflow run 2 (later, separate)
User story                              execution-id + decision
  |                                        |
ksforge implement                       ksforge resume
  |                                        |
Claude Code: "I need a decision"        Claude Code (resumed conversation)
  |                                        |
status = waiting_for_human              status = completed
  |                                        |
process exits, code 6                   changes validated, PR opened
(nothing was blocked)
```

## The state machine

```text
Running --(agent asks)--> WaitingForHuman --(resume)--> Running
Running --(agent finishes, validation passes)--> Completed
Running --(agent fails / validation fails)--> Failed
```

There's no separate state-machine implementation to keep in sync with
anything: `Execution.messages` is an append-only `Vec<ExecutionEvent>`
(`Started`, `AgentStarted`, `QuestionRaised`, `HumanDecided`,
`AgentResumed`, `ValidationStarted`/`Passed`/`Failed`, `Completed`,
`Failed`, `Cancelled`) and `Execution.status` is just the state that log
implies. `ksforge status <id>` reads the same struct a human would want to
inspect after the fact.

## How "I need a decision" is detected

Every agent turn is made with `claude -p --json-schema <schema>`
constraining the final response to:

```json
{
  "status": "completed | waiting_for_human | failed",
  "title": "...",
  "summary": "...",
  "changed_files": ["..."],
  "question": "...",
  "options": [{"id": "oauth2", "label": "OAuth 2"}],
  "failure_reason": "...",
  "completed": ["Analyzed the authentication module."],
  "open_items": ["..."],
  "recommendation": "The repo already has an OAuth-compatible identity boundary.",
  "recommended_option": "oauth2"
}
```

`status` is the discriminator; `question`/`options` are only populated
when it's `waiting_for_human`. This is enforced by Claude Code's own
structured-output validation, not by ksforge regex-matching a fenced code
block out of free text — that was the first design considered and
rejected once `--json-schema` turned out to be a real, documented flag on
the installed Claude Code CLI (see [03-architecture.md](03-architecture.md)
for what was verified vs. inferred about the envelope this arrives in).

`completed`/`open_items`/`recommendation`/`recommended_option` are the
structured progress-reporting fields (section 10-12 of the human-in-the-loop
spec this was built against): Claude Code, not ksforge, produces the
human-facing "what's done / what's open / what do you recommend" narrative,
and `github::report::render` turns it into a PR comment — ksforge itself
never invents progress text. `recommended_option` is dropped (not trusted
blindly) if it doesn't match one of the `options` actually offered
(`application::execute::finish`) — never fully trust a model-reported fact
that isn't independently checkable.

**Honesty about robustness**: this only works as well as Claude Code's
schema enforcement does, and as well as the system prompt's instruction to
use `waiting_for_human` *only when truly necessary* is followed (see
`application::prompt::system_prompt`). It has not been exercised against a
live model in this repository's own test suite (tests use
`MockAgentExecutor`, which returns canned JSON, not a model deciding
whether to ask). Watch the first few real `waiting_for_human` runs in your
own project before trusting the pattern blindly.

## Persistence: local disk + GitHub's own history

There is deliberately **no database and no daemon**. An `Execution` (and
the `ImplementationRequest` it started from) is JSON on disk:

```text
<workspace>/.ksforge/executions/<id>/
  request.json   # the ImplementationRequest — workspace, constraints, validation policy
  state.json     # the Execution — status, messages, pending_question, result, agent_session_id
```

That's enough for local CLI use (pause today, `ksforge resume` next week
in the same clone) and for a single long-lived CI runner. It is **not**
enough on its own for GitHub-hosted Actions runners, where the filesystem
disappears the moment a job ends. Two complementary answers:

1. **`workflow_dispatch` resume (implemented, primary path)**: pass
   `execution-id` and `decision` as workflow inputs to a second run. The
   action re-checks out the repository and calls `ksforge resume
   <execution-id> --decision <decision>` — but on a bare fresh checkout
   there is no `.ksforge/executions/<id>/` directory, so this only works
   if something restored it. That something is either:
   - a cache/artifact step that persists `.ksforge/` between the two
     workflow runs (`actions/cache` keyed by execution id, or
     `actions/upload-artifact` + `download-artifact`), which also carries
     `agent_session_id` forward so `--resume <session-id>` can continue
     the *exact same* Claude Code conversation with full prior context —
     the strongest option when available; or
   - reconstructing just enough by hand: `ksforge resume` degrades
     gracefully when there's no cached `agent_session_id` by starting a
     fresh Claude Code turn that states the original story, the question
     that was asked, and the decision, and asks it to continue — less
     context than a true resumed conversation, but functional.

   A ready-to-copy two-job example (start → cache → resume) is in
   [07-github-actions.md](07-github-actions.md).

2. **Issue/PR comment round-trip (implemented)**: the ergonomic version of
   path 1 posts the pending question as a PR comment, marked with
   `<!-- ksforge:execution=<id>:report -->`, so a reply like `/ksforge
   choose oauth2` can trigger the resume workflow directly, without anyone
   hand-typing `workflow_dispatch` inputs.

   - `ksforge post-report <execution-id> --pr <number>` renders the
     execution (`github::report::render`) and posts it via `gh`, updating
     the existing marker comment in place rather than appending a new one
     each time (`github::comment::post_or_update_report`).
   - `ksforge handle-comment --execution-id <id> --comment-id <id>
     --commenter <login> --body <text>` parses a `/ksforge choose <option>`
     command (`github::decision::parse_choose_command`), re-checks the
     commenter's repository permission via `gh api
     .../collaborators/{login}/permission` (`authorize_commenter` —
     `admin`/`write` only; see [09-security.md](09-security.md)), validates
     the option against the execution's open gate, and records the GitHub
     comment id so a duplicate delivery of the same webhook is a no-op
     (section 17). On success it prints the validated option id and, when
     `GITHUB_OUTPUT` is set, writes `execution-id=`/`decision=` for the next
     step.
   - It does **not** call `resume` itself — chain it:
     `ksforge resume <execution-id> --decision <option> --decided-by
     <commenter>`. Exactly one code path (`application::resume::resume`)
     ever talks to Claude Code again.

   See the third example workflow in [07-github-actions.md](07-github-actions.md).

## What does *not* survive across a paused GitHub Actions job

Be precise with users about this: Claude Code's in-progress file edits
from before it asked its question are **not** guaranteed to persist across
two separate job runs unless you've cached the actual working tree (not
just `.ksforge/`) — a fresh `actions/checkout` starts clean. What
*reliably* carries over is the conversation/decision context (via a cached
Claude Code session, or via the reconstructed-prompt fallback), which is
usually enough for Claude Code to redo or continue the work coherently on
resume — but it is genuinely redoing/continuing, not literally restoring
half-written files from a deleted VM.

# GitHub Actions

Ready-to-copy template for the primary flow below:
[examples/workflows/change-request-to-ksforge.yml](../examples/workflows/change-request-to-ksforge.yml).

You do **not** need your own "configure git identity" step for
`--create-pull-request`/`--push-to-branch` to work — `git commit` needs
one, a fresh runner has none by default, and ksforge sets a local fallback
(`github-actions[bot]`) itself if nothing is already configured at any
level (`github::pull_request::ensure_git_identity`), rather than every
consumer workflow needing to remember this. A step that already sets one
(as some example workflows below still show, from before this existed)
is harmless — ksforge's fallback only applies when none exists yet.

A `checkout` with submodules (`with: { submodules: true }` or `recursive`)
works correctly: ksforge attributes each changed file to the repo that
actually owns it (`github::repo::group_by_repo`) and commits/pushes a
submodule's own changes before the superproject's, so the superproject's
commit picks up the updated gitlink automatically. This applies uniformly,
not only in Actions — the same code path handles a plain multi-repo CLI
workspace, see [12-interactive-chat.md](12-interactive-chat.md).

## Prerequisite: let Actions open pull requests

`create-pull-request: "true"` needs more than `permissions:
pull-requests: write` in the workflow YAML — GitHub disables **"Allow
GitHub Actions to create and approve pull requests"** by default on new
repositories, and that repo-level setting caps what the YAML's own
`permissions:` block can grant (a workflow's `permissions:` can only
narrow this ceiling, never raise it). Without it, the run fails at the PR
step with:

```text
GraphQL: GitHub Actions is not permitted to create or approve pull requests (createPullRequest)
```

— even though the capability itself ran and completed correctly first;
the failure is purely at PR creation, after the actual change was already
made (and, since ksforge already pushed the branch by then, that change
sits on an orphaned branch rather than being lost).

Enable it once per repository: **Settings → Actions → General → Workflow
permissions → "Allow GitHub Actions to create and approve pull
requests"** — or via the API:

```bash
gh api -X PUT repos/<owner>/<repo>/actions/permissions/workflow \
  -f default_workflow_permissions=read \
  -F can_approve_pull_request_reviews=true
```

Not needed for `review`/`explain` or any run with `create-pull-request:
"false"`.

## If the change might touch `.github/workflows/*`

`permissions: contents: write` is not enough to push a commit that adds or
modifies a workflow file — GitHub gates that separately, and (confirmed
the hard way) there is **no `permissions:` key for it at all**:

```text
! [remote rejected] ksforge/<id> -> ksforge/<id> (refusing to allow a GitHub App
to create or update workflow `.github/workflows/<name>.yml` without `workflows` permission)
```

reads like a `permissions: workflows: write` fix, but adding that key
breaks the workflow file outright — `Unexpected value 'workflows'`, a
parse error, worse than the runtime rejection it was meant to fix. The
scope this error names ("workflow") only exists on a **classic Personal
Access Token** (or a GitHub App installation with that permission) —
never on the auto-generated `GITHUB_TOKEN`, regardless of what the
workflow's own `permissions:` block grants it.

To let a change request actually push a workflow-file change, `git`
inside the job needs to authenticate as something holding that scope
instead of the default `GITHUB_TOKEN` — in practice, a classic Personal
Access Token (scopes `workflow` + `repo`) supplied to `actions/checkout`'s
`token:` input, since checkout persists whatever token it's given as the
job's own git credential (so ksforge's own later `git push` picks it up
too; `gh pr create` itself needs no extra scope, only the push of a commit
that touches `.github/workflows/*` does).

**Not yet a verified snippet to copy.** A `token: ${{
secrets.WORKFLOW_SCOPED_PAT || secrets.GITHUB_TOKEN }}` fallback (meant to
be a no-op until the secret is actually set) was tried in a real
`ksforge-playground` workflow and instead broke that checkout outright —
`fatal: could not read Username for 'https://github.com': terminal
prompts disabled` — even though no `WORKFLOW_SCOPED_PAT` secret existed
yet, i.e. even the fallback arm regressed the previously-working plain
default. Reverted there rather than left in place unverified. If you set
this up, test the exact `token:` expression you use against a run that
does *not* touch `.github/workflows/*` first, to confirm it doesn't
regress the common case, before relying on it for one that does.

## Primary example: `workflow_dispatch`

```yaml
name: Implement Change Request

on:
  workflow_dispatch:
    inputs:
      change-request:
        description: "Change request"
        required: true
        type: string

permissions:
  contents: write
  pull-requests: write

jobs:
  implement:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: chevp/ksforge@v1
        id: ksforge
        with:
          change-request: ${{ inputs.change-request }}
          capability: implement
          create-pull-request: "true"
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}

      - name: Note pending decision
        if: steps.ksforge.outputs.status == 'waiting_for_human'
        run: |
          echo "::notice::Paused. Re-run this workflow with a follow-up job passing execution-id=${{ steps.ksforge.outputs.execution-id }} and decision=<option-id>."
```

Read-only review-only workflow needs less:

```yaml
permissions:
  contents: read
  pull-requests: read

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: chevp/ksforge@v1
        with:
          change-request: "Review this pull request's diff for correctness issues."
          capability: review
          create-pull-request: "false"
          format: json
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
```

Never default to `permissions: write-all` — request `contents: write` +
`pull-requests: write` only for capabilities that actually open a PR.

## Two-run resume, with the session cached

This is the concrete pattern [06-human-in-the-loop.md](06-human-in-the-loop.md)
describes: cache `.ksforge/` between the initial run and the resume run so
`agent_session_id` (and therefore `claude --resume`) survives the gap.

```yaml
name: Implement Change Request (resumable)

on:
  workflow_dispatch:
    inputs:
      change-request:
        description: "Change request (leave empty when resuming)"
        required: false
        type: string
      execution-id:
        description: "Set together with decision to resume a paused run"
        required: false
        type: string
      decision:
        description: "Option id to resume with"
        required: false
        type: string

permissions:
  contents: write
  pull-requests: write

jobs:
  implement:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Restore paused execution state
        if: inputs.execution-id != ''
        uses: actions/cache/restore@v4
        with:
          path: .ksforge
          key: ksforge-${{ inputs.execution-id }}

      - uses: chevp/ksforge@v1
        id: ksforge
        with:
          change-request: ${{ inputs.change-request }}
          execution-id: ${{ inputs.execution-id }}
          decision: ${{ inputs.decision }}
          create-pull-request: "true"
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}

      - name: Save execution state for a possible resume
        if: steps.ksforge.outputs.status == 'waiting_for_human'
        uses: actions/cache/save@v4
        with:
          path: .ksforge
          key: ksforge-${{ steps.ksforge.outputs.execution-id }}
```

## Comment-driven resume: `issue_comment`

The two-run pattern above needs someone to copy `execution-id`/`decision`
into `workflow_dispatch` by hand. This wires the PR comment posted by
`ksforge post-report` (see [06-human-in-the-loop.md](06-human-in-the-loop.md))
up to a reply like `/ksforge choose oauth2` directly.

The `chevp/ksforge@v1` action already puts the `ksforge` binary on `PATH`
for the rest of the job — this workflow calls it directly rather than
through a second `uses:` step, so `action.yml`'s own inputs stay minimal.

```yaml
name: ksforge decision

on:
  issue_comment:
    types: [created]

permissions:
  contents: write
  pull-requests: write

jobs:
  decide:
    # Defense in depth, layer 1 (see docs/09-security.md) — the invoking
    # workflow's own gate. ksforge re-checks this independently via `gh
    # api .../collaborators/{login}/permission` before accepting the
    # decision, so this `if:` is a fast filter, not the only check.
    if: >
      github.event.issue.pull_request != null &&
      contains(fromJSON('["OWNER", "MEMBER", "COLLABORATOR"]'), github.event.comment.author_association)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Extract execution id from the ksforge comment
        id: exec
        run: |
          id=$(echo '${{ github.event.comment.body }}' | grep -oE 'ksf_[a-f0-9]+' | head -1)
          echo "id=$id" >> "$GITHUB_OUTPUT"

      - name: Restore paused execution state
        if: steps.exec.outputs.id != ''
        uses: actions/cache/restore@v4
        with:
          path: .ksforge
          key: ksforge-${{ steps.exec.outputs.id }}

      # This job never runs a capability, so it installs the `ksforge`
      # binary directly (same release download `action.yml` uses — see
      # [01-installation.md](01-installation.md)) rather than going through
      # the composite action for a step that wouldn't call a capability.
      - name: Install ksforge
        if: steps.exec.outputs.id != ''
        run: |
          curl -sSfL https://github.com/chevp/ksforge/releases/latest/download/ksforge-linux-x86_64.tar.gz \
            | tar -xz
          sudo install -m 0755 ksforge /usr/local/bin/ksforge

      - name: Validate the decision comment
        if: steps.exec.outputs.id != ''
        id: decision
        run: |
          option=$(ksforge handle-comment \
            --execution-id "${{ steps.exec.outputs.id }}" \
            --comment-id "${{ github.event.comment.id }}" \
            --commenter "${{ github.event.comment.user.login }}" \
            --body "${{ github.event.comment.body }}")
          echo "option=$option" >> "$GITHUB_OUTPUT"

      - name: Resume
        if: steps.exec.outputs.id != '' && steps.decision.outputs.option != ''
        run: |
          ksforge resume "${{ steps.exec.outputs.id }}" \
            --decision "${{ steps.decision.outputs.option }}" \
            --decided-by "${{ github.event.comment.user.login }}" \
            --create-pull-request

      - name: Post updated report
        if: steps.exec.outputs.id != ''
        run: |
          ksforge post-report "${{ steps.exec.outputs.id }}" \
            --pr "${{ github.event.issue.number }}"

      - name: Save execution state for a possible further pause
        if: steps.exec.outputs.id != ''
        uses: actions/cache/save@v4
        with:
          path: .ksforge
          key: ksforge-${{ steps.exec.outputs.id }}
```

`ksforge handle-comment` fails (non-zero exit) on an unrecognized command,
an unauthorized commenter, a gate that's already resolved, or an unknown
option — the workflow simply doesn't reach `resume` in any of those cases,
matching "never resume solely because a comment resembles a decision"
(security section, spec §pewY5yG).

## Comment-driven follow-up: free-text PR comments

The two flows above only ever act on the change request ksforge originally
received. This lets an authorized collaborator ask for *more* on an
already-open PR — a plain comment like `/ksforge fix the button is
misaligned on mobile` — by running that capability against the PR's own
branch and pushing the result back onto it, instead of opening a second
PR. `ksforge handle-comment` (called without `--execution-id` this time)
recognizes this shape too — see [09-security.md](09-security.md) for the
authorization rules it applies before acting on one.

```yaml
name: ksforge PR follow-up

on:
  issue_comment:
    types: [created]

permissions:
  contents: write
  pull-requests: write

jobs:
  follow-up:
    # Same gate as "Comment-driven resume" above — a fast filter, not the
    # only check; ksforge re-verifies the commenter independently.
    if: >
      github.event.issue.pull_request != null &&
      contains(fromJSON('["OWNER", "MEMBER", "COLLABORATOR"]'), github.event.comment.author_association)
    runs-on: ubuntu-latest
    steps:
      - name: Resolve the PR's own head branch
        id: pr
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          PR_NUMBER: ${{ github.event.issue.number }}
        run: |
          set -euo pipefail
          ref=$(gh pr view "$PR_NUMBER" --json headRefName -q .headRefName)
          echo "ref=$ref" >> "$GITHUB_OUTPUT"

      - uses: actions/checkout@v4
        with:
          ref: ${{ steps.pr.outputs.ref }}
          fetch-depth: 0

      # This job calls several ksforge subcommands, not one capability, so
      # (like "Comment-driven resume" above) it installs the binary
      # directly instead of going through the composite action.
      - name: Install ksforge
        run: |
          curl -sSfL https://github.com/chevp/ksforge/releases/latest/download/ksforge-linux-x86_64.tar.gz \
            | tar -xz
          sudo install -m 0755 ksforge /usr/local/bin/ksforge

      - name: Install Claude Code
        run: npm install -g @anthropic-ai/claude-code

      # Untrusted values (the comment body especially) go through `env:`
      # here, never interpolated directly into the script body — same
      # reasoning as the comment in action.yml's own run step.
      - name: Classify the comment
        id: handle
        env:
          COMMENT_ID: ${{ github.event.comment.id }}
          COMMENTER: ${{ github.event.comment.user.login }}
          COMMENT_BODY: ${{ github.event.comment.body }}
        run: |
          set +e
          capability=$(ksforge handle-comment \
            --comment-id "$COMMENT_ID" \
            --commenter "$COMMENTER" \
            --body "$COMMENT_BODY")
          echo "capability=$capability" >> "$GITHUB_OUTPUT"
          exit 0

      - name: Run the requested capability
        if: steps.handle.outputs.capability != ''
        id: run
        env:
          CAPABILITY: ${{ steps.handle.outputs.capability }}
          CHANGE_REQUEST: ${{ steps.handle.outputs.change_request }}
        run: |
          set -euo pipefail
          out=$(ksforge "$CAPABILITY" --change "$CHANGE_REQUEST" --push-to-branch --format json)
          echo "$out"
          echo "execution-id=$(echo "$out" | jq -r '.execution_id')" >> "$GITHUB_OUTPUT"

      - name: Post a report comment
        if: steps.handle.outputs.capability != ''
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          ksforge post-report "${{ steps.run.outputs.execution-id }}" \
            --pr "${{ github.event.issue.number }}"
```

An ordinary, unrelated PR comment makes `handle-comment` exit non-zero and
`capability` stay empty — `set +e` there keeps that from failing the job;
the two steps after it are simply skipped (`if: steps.handle.outputs.capability != ''`),
the same idiom "Comment-driven resume" uses for `steps.exec.outputs.id`.

`--push-to-branch` (instead of `--create-pull-request`) is what makes this
update the PR already open on `steps.pr.outputs.ref` rather than opening a
second one — see [`github::pull_request::push_follow_up`], and
[09-security.md](09-security.md) for why the authorization check here
matters more than it does for `/ksforge choose`: this starts a brand new,
write-capable, billed run from arbitrary comment text, not a pick among
options the agent already offered.

## Inputs / outputs

See `action.yml` for the authoritative list. Highlights: `change-request` xor
`execution-id`+`decision`; `dry-run` defaults to `false` at the action
layer (unlike leaving it implicit) so a workflow author has to opt in
explicitly to real writes either way, matching whatever they set. Outputs
(`status`, `execution-id`, `changed-files`, `summary`, `pull-request-url`)
are populated by parsing the CLI's own `--format json` output — see
`action.yml`'s run step.

**Note on the paused case and step status**: the composite action exits
with ksforge's own exit code, including `6` for `waiting_for_human` — so a
paused run shows as a failed step/red run in the Actions UI, which is
intentional (it flags "needs attention"), but means a workflow that treats
pausing as a normal, expected outcome should add `continue-on-error: true`
to the ksforge step and branch on `steps.ksforge.outputs.status` itself
rather than on step success.

## Distribution

The action downloads a prebuilt `linux-x86_64` binary from this repo's
GitHub Releases (see [01-installation.md](01-installation.md)) rather than
compiling ksforge on every run. It also installs Claude Code itself
(`npm install -g @anthropic-ai/claude-code`, pin a version with
`claude-code-version`) — skipped if `claude` is already on `PATH` (e.g. a
self-hosted runner that preinstalls a pinned version). Outside of this
action (running the CLI directly, see [START.md](../START.md)), Claude
Code is still your own responsibility to install — see
[01-installation.md](01-installation.md).

# GitHub Actions

## Primary example: `workflow_dispatch`

```yaml
name: Implement User Story

on:
  workflow_dispatch:
    inputs:
      story:
        description: "User story"
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
          story: ${{ inputs.story }}
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
          story: "Review this pull request's diff for correctness issues."
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
name: Implement User Story (resumable)

on:
  workflow_dispatch:
    inputs:
      story:
        description: "User story (leave empty when resuming)"
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
          story: ${{ inputs.story }}
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
(security section, spec §16).

## Inputs / outputs

See `action.yml` for the authoritative list. Highlights: `story` xor
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

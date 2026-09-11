# Capabilities

A capability is what kind of operation ksforge is orchestrating. All seven
share one execution pipeline (`application::execute::run`); each only
supplies policy.

A related but distinct concept is an **integration**
([11-integrations.md](11-integrations.md)): a capability picks *what
ksforge is doing* (implement/review/fix/explain); an integration only adds
*tools* (an MCP server) to whichever capability is running.

`ksforge coordinate` (04-cli-reference.md) is a separate, read-only
analysis command with its own Core prompt
(`prompts/coordinator/system-prompt.md`, see `application::coordinate`) —
it runs its own path rather than the shared
`application::execute::run`/`domain::Capability` machinery on this page.

| Capability | Tools (ACT phase) | Writes? | Change scope | Human-in-the-loop? |
|---|---|---|---|---|
| `implement` | default (full) | yes | none | yes |
| `fix` | default (full) | yes | none | yes |
| `review` | `Read,Grep,Glob` only | never | none | no |
| `explain` | `Read,Grep,Glob` only | never | none | no |
| `txt2img` | default (full) | yes | none | no |
| `img2img` | default (full) | yes | none | no |
| `test` | default (full) | yes | tests only | yes |

`review` and `explain` never offer the human-in-the-loop protocol in their
prompt — there's nothing for them to ask about, they only report. If a
`review`/`explain` run somehow comes back `waiting_for_human` anyway (the
agent not following instructions), ksforge treats that as a failure rather
than trusting it, since the read-only guarantee is a hard one.

Every capability's UNDERSTAND/LOCATE turns are read-only regardless of the
table above — only ACT ever gets the tool policy shown here (see
docs/03-architecture.md, "The phase loop").

## `test`: a write-capable capability with a checked scope

`test` (`src/application/test.rs`) adds or updates tests without touching
non-test source — but unlike every other write-capable capability, that
restriction isn't just requested in the prompt. `Capability::change_scope`
returns `Some(ChangeScope::TestsOnly)`, and after ACT, ksforge checks every
changed path (`domain::test_scope::violations`) against it: a dedicated
test file/directory by common per-language convention (Go's `_test.go`,
JS/TS's `.test.js`/`.spec.ts`, Python's `test_*.py`, Ruby's `_spec.rb`,
Java/C#'s `*Test(s).*`, anything under `tests`/`test`/`spec`/`__tests__`),
or — since not every language keeps tests in a separate file — a non-test
file that already contains a recognized inline-test marker after the
change (Rust's colocated `#[cfg(test)]`, ksforge's own convention
throughout this codebase). Any other changed file fails the run, with the
offending paths named in the failure, regardless of what the agent itself
reported. This is a heuristic, not a real diff — it can't prove a mixed
file's change was *only* to its test portion, only that the file plausibly
contains tests at all; see the doc comment on `domain::test_scope` for the
exact reasoning.

`txt2img` (`src/application/txt2img.rs`) and `img2img`
(`src/application/img2img.rs`) are stubs: each generates a placeholder
artifact rather than calling a real image model, and neither depends yet on
the standalone `ks-llm-image` crate (`apps/kosmos/libs/ks-llm-image`) — a
single crate meant to back multiple image-generation capabilities, building
and testing independently of this repo. Each only runs when explicitly
invoked via its own subcommand (`ksforge txt2img` / `ksforge img2img`),
never as part of `implement`/`fix`.

## Adding a capability

Implement `domain::Capability` (`id`, `description`, `tool_policy`,
`default_constraints`, `prompt_fragment`, `supports_human_interaction`,
optionally `change_scope` (see `test` above — only needed if ACT's write
scope must be narrower than "the whole workspace"), and `execute` — which
every built-in impl implements as a one-liner delegating to
`application::execute::run`, see the doc comment on the trait for why that
can't be a default method) and register it in
`domain::capability::CapabilityRegistry::with_defaults`. `prompt_fragment`
is just `include_str!("../../prompts/capabilities/<id>.md")` for every
built-in capability — add the new capability's own instructions as a
markdown file there rather than an inline string, see "Prompt structure"
in [03-architecture.md](03-architecture.md).

Declarative, YAML-defined capabilities (loaded at runtime rather than
compiled in) were deliberately not built for this version — the
architecture doesn't block adding a loader later, but a fifth Rust type
implementing one trait was enough for now; a full manifest format and
loader is speculative scope this version didn't need.

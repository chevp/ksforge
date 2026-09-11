# Capabilities

A capability is what kind of operation ksforge is orchestrating. All four
share one execution pipeline (`application::execute::run`); each only
supplies policy.

Not to be confused with an **integration** ([11-integrations.md](11-integrations.md)):
a capability picks *what ksforge is doing* (implement/review/fix/explain);
an integration only adds *tools* (an MCP server) to whichever capability
is running.

Also not a capability: `ksforge coordinate` (04-cli-reference.md). It
doesn't go through `application::execute::run`, `domain::Capability`, or
any of the shared tool-scope/constraint machinery on this page — it's a
separate, read-only analysis command with its own Core prompt
(`prompts/coordinator/system-prompt.md`), see `application::coordinate`.

| Capability | Tools | Writes? | Human-in-the-loop? |
|---|---|---|---|
| `implement` | default (full) | yes | yes |
| `fix` | default (full) | yes | yes |
| `review` | `Read,Grep,Glob` only | never | no |
| `explain` | `Read,Grep,Glob` only | never | no |

`review` and `explain` never offer the human-in-the-loop protocol in their
prompt — there's nothing for them to ask about, they only report. If a
`review`/`explain` run somehow comes back `waiting_for_human` anyway (the
agent not following instructions), ksforge treats that as a failure rather
than trusting it, since the read-only guarantee is a hard one.

## Adding a capability

Implement `domain::Capability` (`id`, `description`, `tool_policy`,
`default_constraints`, `prompt_fragment`, `supports_human_interaction`,
and `execute` — which every built-in impl implements as a one-liner
delegating to `application::execute::run`, see the doc comment on the
trait for why that can't be a default method) and register it in
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

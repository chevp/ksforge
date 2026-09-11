§RCOjEeb Human-in-the-loop

If you cannot proceed without a decision only a human can make (an ambiguous requirement, a
choice between materially different designs), do not guess and do not ask interactively — you
will not be prompted. Instead, set `"status": "waiting_for_human"` in your final JSON
response, along with a concise `"question"` and 2-4 concrete `"options"` (each an
`{id, label}`). Only do this when truly necessary; prefer making a reasonable, documented
default choice and noting it in your summary.

§7XOwIrp Recommendations are mandatory when defensible

Whenever you raise a `waiting_for_human` question, give a concrete recommendation whenever
the repository provides real evidence for one — do not merely enumerate the options and stop
there.

- `"recommended_option"`: the `id` of the option you'd pick — must be one of the `id`s in
  `"options"`.
- `"recommendation"`: the reason, in terms of what you actually found in the repository (an
  existing abstraction, a convention already in use, a dependency already present). Never
  fabricate evidence you did not actually see.

If there is genuinely no repository evidence that favors one option over another, leave both
fields unset and say so plainly in `"recommendation"`:
`"No recommendation: the repository provides no evidence that meaningfully favors one option
over another."` — do not invent a tie-breaker to appear decisive.

Bad: listing options with no opinion ("OAuth2 or JWT — which do you want?").
Good: `"recommended_option": "oauth2"`, `"recommendation": "The repository already has an
OAuth-compatible identity boundary in src/auth/identity.rs; using it avoids a second token
model."`

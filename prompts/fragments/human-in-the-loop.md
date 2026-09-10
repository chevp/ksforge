§23 Human-in-the-loop

If you cannot proceed without a decision only a human can make (an ambiguous requirement, a
choice between materially different designs), do not guess and do not ask interactively — you
will not be prompted. Instead, set `"status": "waiting_for_human"` in your final JSON
response, along with a concise `"question"` and 2-4 concrete `"options"` (each an
`{id, label}`). Only do this when truly necessary; prefer making a reasonable, documented
default choice and noting it in your summary.

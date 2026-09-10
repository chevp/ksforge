§20 Code quality

A change must build and pass the project's own checks, using the repository's existing
language, module, and dependency conventions. No unnecessary dependencies. No new
abstractions unless the story genuinely requires them.

---

§21 Validation workflow

After making a change (`implement`/`fix`) or forming your findings (`review`/`explain`):

1. Run the project's own formatting/lint tooling, if present.
2. Run its build/typecheck, if applicable.
3. Run the relevant tests, and actually execute them — do not infer a result.
4. On a validation failure, determine the cause (a wrong assumption, missing setup, or your
   own change being wrong) and fix what is yours to fix.
5. NEVER hide a real defect behind a weakened check or assertion.

ksforge additionally runs its own validation commands (if configured) after you report
success — a `completed` status here is not the last check that result has to pass.

---

§22 Output format

At the end, output exactly one final-turn result matching the required JSON schema — no
explanatory text outside it. The schema constrains the shape to:

```json
{
  "status": "completed | waiting_for_human | failed",
  "summary": "...",
  "changed_files": ["..."],
  "question": "...",
  "options": [{"id": "...", "label": "..."}],
  "failure_reason": "...",
  "completed": ["..."],
  "open_items": ["..."],
  "recommendation": "...",
  "recommended_option": "..."
}
```

`status: "completed"` without an actually executed, passing validation run is forbidden — no
success claim without a confirmed run (Core §7: never claim a successful validation run you
did not actually execute).

`completed` and `open_items` are short, factual bullet strings — what you actually did, and
what actually remains and why. Populate `completed` on every status, not only `completed`
turns: a `waiting_for_human` report still needs to say what happened before the question came
up (repository analysis, a plan, partial changes), and `open_items` still applies to a
`completed` turn that has known, undone follow-ups. Never write vague filler like "more
information is needed" — state the concrete fact.

`recommendation`/`recommended_option` are covered in full in the human-in-the-loop fragment
below (only relevant on `waiting_for_human`); leave both unset when there is genuinely no
defensible preference, and say why in `recommendation` rather than omitting it silently.

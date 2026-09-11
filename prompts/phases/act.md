§ENxR14E Existing code takes priority over new code

Before writing something new, check whether equivalent behavior already exists — the LOCATE
phase's `existing_abstractions` above already names what it found.

If it does:

* extend or fix it in place when appropriate
* do not duplicate it
* preserve behavior the change request does not ask you to change
* change only what is necessary

If the change request genuinely requires new code: follow the repository's existing naming and
organization conventions for where it goes (the LOCATE phase's `conventions`).

---

§Lfxkhkv Code quality

A change must build and pass the project's own checks, using the repository's existing
language, module, and dependency conventions. No unnecessary dependencies. No new
abstractions unless the change request genuinely requires them.

---

§hQnJPKM Validation workflow

After making a change (`implement`/`fix`) or forming your findings (`review`/`explain`):

1. Run the project's own formatting/lint tooling, if present.
2. Run its build/typecheck, if applicable.
3. Run the relevant tests, and actually execute them — do not infer a result.
4. On a validation failure, determine the cause (a wrong assumption, missing setup, or your
   own change being wrong) and fix what is yours to fix.
5. NEVER hide a real defect behind a weakened check or assertion (Core §aUIxFPP).

ksforge additionally runs its own validation commands (if configured) after this phase, against
the real filesystem result — a `completed` status here is not the last check your result has to
pass.

`title` is a short, standalone headline for the change — the same register as a good commit
subject line: imperative mood ("Add ...", "Fix ...", not "Added"/"Fixes"), **about 5 words**,
no trailing period, no restating "implement"/"fix"/the capability name (ksforge already
prefixes that). It is placed verbatim into the pull request title and commit subject when
`--create-pull-request`/`--push-to-branch` is set. Always set it when you made changes; omit it
for `review`/`explain` (nothing gets a PR) and for `waiting_for_human`/`failed`.

§Ijk08ZT LOCATE phase

You are executing the LOCATE phase. You have read-only tools; nothing you do here can change
the repository. Your understanding of the change request from the UNDERSTAND phase is included
below this prompt.

Inspect what is actually relevant to it:

* the project's build/package manifest and how it's built, tested, linted
* existing code in the area the change request touches
* existing tests covering that area
* naming, module, and error-handling conventions already in use
* related configuration, fixtures, or seed data
* existing documentation that describes the current behavior

Report what you found: paths in `relevant_files`; reusable helpers/patterns already in the
repository in `existing_abstractions`, so the later ACT phase reuses them instead of
reinventing them (e.g. if a `validate_input(...)` helper already exists, name it here);
covering tests in `existing_tests`; and naming/style rules in `conventions`.

§10 Repository analysis

Before changing or reporting on anything, inspect what is actually relevant to the change request:

* the project's build/package manifest and how it's built, tested, linted
* existing code in the area the change request touches
* existing tests covering that area
* naming, module, and error-handling conventions already in use
* related configuration, fixtures, or seed data
* existing documentation that describes the current behavior

Prefer existing abstractions over direct implementation. Example: if a `validate_input(...)`
helper already exists, use it instead of duplicating its logic inline.

---

§11 Existing code takes priority over new code

Before writing something new, check whether equivalent behavior already exists.

If it does:

* extend or fix it in place when appropriate
* do not duplicate it
* preserve behavior the change request does not ask you to change
* change only what is necessary

If the change request genuinely requires new code: follow the repository's existing naming and
organization conventions for where it goes.

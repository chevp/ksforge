Add or update tests for the change request's target area — do not implement or fix the
underlying behavior itself, only test it. Inspect the relevant code and its existing test
suite first, then add the smallest set of test cases that actually exercises the requested
behavior, following the repository's own test framework and conventions.

Only touch test files, or add tests colocated in an existing file where that is the
language's own convention (e.g. Rust's `#[cfg(test)] mod tests` in the same file as the code
it tests). Never change application/library source, configuration, or build files to make a
test pass or to work around a gap in testability — ksforge checks the files you touched
against this rule after this phase and rejects the run if any of them fall outside it,
regardless of what you report.

# Security

For the full model (prompt injection posture, least-privilege GitHub
Actions permissions, secret handling), see
[docs/09-security.md](docs/09-security.md).

## Reporting a vulnerability

Please do not open a public issue for a suspected security vulnerability.
Instead, use GitHub's private vulnerability reporting for this repository
(Security tab → "Report a vulnerability"), or contact the maintainer
directly. Include enough detail to reproduce the issue; you'll get an
acknowledgment and, once fixed, credit in the release notes unless you'd
prefer otherwise.

Note that ksforge shells out to the `claude` and `gh` CLIs and never
handles `ANTHROPIC_API_KEY` or GitHub tokens itself (see
[docs/09-security.md](docs/09-security.md)) — a report about credential
handling in either of those tools should generally go to their own
projects instead, unless you've found ksforge doing something that
exposes them.

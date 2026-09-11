§NNbADAM UNDERSTAND phase

You are executing the UNDERSTAND phase. You have read-only tools; nothing you do here can
change the repository — ksforge itself will not grant write access until the ACT phase, later.

Determine what the change request actually requires:

* what behavior or finding is being asked for, in your own words
* the areas of the repository it plausibly touches (files, modules, directories) — a first
  guess is fine, the LOCATE phase after this one will confirm it

Do not read more of the repository than you need to answer this; deep inspection of existing
code, tests, and conventions is the LOCATE phase's job, not this one.

Report your understanding in `summary`, and the areas from the second bullet in `scope`.

# Repository instructions

- Identify the executable and build profile the user actually launches. Rebuild that executable after relevant changes and provide its exact launch command. Passing tests does not refresh the application executable.
- Match verification effort to the change. Use focused checks for small changes; run broader suites when shared behavior or packaging is affected. Do not repeatedly run unrelated regression checks.
- Extend a meaningful existing test before adding a new one. Consolidate overlapping coverage while preserving unique assertions; avoid tests that merely restate constants or implementation details.
- Keep UI controls compact and avoid duplicate actions. Limit changes to the controls or behavior requested, preserving surrounding workflows. When a screenshot identifies particular controls, do not infer permission to remove the entire toolbar or workflow.
- Keep project documentation in the existing guides: `README.md`, `WINDOWS.md`, `UPDATES.md`, `LICENSES.md`, `DEVELOPMENT.md`, and `RELEASE_NOTES.md`. Update an existing guide when needed; do not modify `README.md` unless the user explicitly requests it.
- Keep guides focused on current behavior. Remove superseded proposals and obsolete milestone history rather than accumulating reports.
- Do not create standalone Markdown plans, research, audits, benchmarks, validation logs, release notes, or task summaries unless the user explicitly requests a file. Report results in the conversation; put generated evidence in ignored `artifacts/`.
- Preserve upstream documentation, licenses, notices, and model attribution under `vendor/` and `third_party/`. Do not use those directories for project reports.
- Run `python3 scripts/check-documentation.py` before finishing changes. If the user explicitly requests another Markdown file, update the check and `.gitignore` allowlists together.

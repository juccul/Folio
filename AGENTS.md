# Repository instructions

- Keep project documentation in the existing guides: `README.md`, `WINDOWS.md`, `UPDATES.md`, `LICENSES.md`, `DEVELOPMENT.md`, `RELEASE_NOTES.md`, `MATH_SOLVER_DESIGN.md`, and `OCR_FIRST_USE.md`. Update an existing guide when needed; do not modify `README.md` unless the user explicitly requests it.
- Do not create standalone Markdown plans, research, audits, benchmarks, validation logs, release notes, or task summaries unless the user explicitly requests a file. Report results in the conversation; put generated evidence in ignored `artifacts/`.
- Preserve upstream documentation, licenses, notices, and model attribution under `vendor/` and `third_party/`. Do not use those directories for project reports.
- Run `python3 scripts/check-documentation.py` before finishing changes. If the user explicitly requests another Markdown file, update the check and `.gitignore` allowlists together.

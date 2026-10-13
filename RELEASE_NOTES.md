# Folio 0.1.5

- More efficient editing: cached viewport geometry and paper backgrounds reduce repeated rendering work; idle maintenance runs less often while active drawing and background operations retain responsive polling.
- Lower retained preview memory: obsolete thumbnails, object previews, PDF pages and GPU images are released as content changes or leaves the working set. Selection actions and drawing-order updates are faster on large documents.
- Handwriting recognition removes false spaces inside compact digit groups, such as `1 2 3 4` → `1234`, using the captured ink layout. Wide gaps and ambiguous or mixed content retain their original spacing; suggestions remain editable before acceptance.
- Math step checking correctly evaluates held arithmetic in fractional powers before comparing expressions.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak, checksums and complete corresponding source are published together. Updates remain signed and user-initiated.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.

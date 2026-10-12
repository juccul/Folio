# Folio 0.1.3

- Reduced GPU memory growth on ink-heavy Linux pages. Completed upload buffers are reused or released instead of accumulating as the scene grows.
- Smoother Linux drawing with bounded background caching for settled ink, matching the Windows rendering path. Selection, erasing, theme and zoom changes retain editable original strokes.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak, checksums and complete corresponding source are published together. Updates remain signed and user-initiated.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.

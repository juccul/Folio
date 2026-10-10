# Folio 0.1.0

This fresh distribution includes all current Folio features and replaces the retired releases.

- Editable vector handwriting, configurable notebooks, PDF annotation, page organization, typed text, offline math and optional OCR.
- Standard color box selector, automatic system appearance, library folders, favorites, tabs, templates and recovery.
- User-initiated, signed updates with download progress, restart and a library backup before installation.
- Fixed Windows Home and Update clicks, and maximize/restore behavior.
- Native Wayland by default on Wayland sessions, including GNOME; X11 remains available with `FOLIO_FORCE_X11=1`.
- Improved Linux window resizing; validation details are included with the published release.
- A new updater cache sequence prevents retired release metadata from blocking future updates; existing recovery files remain intact.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak, checksums and complete corresponding source are published together. The signed Flatpak repository and signing keys are retained.

Existing installations require one manual replacement because 0.1.0 starts a new version sequence. Close Folio, run the Windows installer using the existing installation directory, or reinstall the new Flatpak in the same user/system installation. Preserve your library and app data. Future releases increase the version normally.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.

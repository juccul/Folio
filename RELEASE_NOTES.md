# Folio 0.1.3

- Added an update notification to the right of the title bar, immediately before the window controls, whenever a newer stable release is available.
- Downloads start only after clicking **Update**. Verified packages offer **Restart to update**; failed checks, downloads and installations offer recovery or retry.
- Restart saves all open documents and workspace state, then creates a complete library backup. Unfinished document operations and other Windows Folio instances prevent unsafe replacement.
- Windows installer updates preserve custom installation and library paths. Portable packages support verified payload replacement; the previous payload is retained for recovery.
- Flatpak updates use a signed repository and the update/restart portal without adding host access. New Flatpak bundles connect older installations to the release remote; `.flatpakref` supports new installations.
- Added Settings → Updates for manual checks and diagnostic details.
- Retained the 0.1.2 performance improvements, page-creation settings on the main New document button, and the single Library button.

See [UPDATES.md](UPDATES.md) for installation and recovery. Version 0.1.2 needs one manual installation of 0.1.3 to gain the updater. Future releases must use a higher version; release assets and signed manifests must remain immutable.

Database schema remains 6 and document format remains 4. Keep backups before opening a library with an older version.

Validation results and package hashes are recorded in [RELEASE_VALIDATION.md](RELEASE_VALIDATION.md).

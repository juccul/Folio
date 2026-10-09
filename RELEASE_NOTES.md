# Folio 0.1.4

- Replaced the RGB channel sliders with a standard saturation/brightness color box and rainbow hue slider. Ink, text, paper, new-document paper and theme colors use the same picker. Square swatches, recent colors and exact hex entry remain available.
- Color controls support mouse dragging and keyboard adjustment. Hue stays selected while choosing white, gray or black. Transparent theme borders and input colors retain their opacity and show a checkerboard alpha control.
- Added System appearance, which follows your device at startup and when the theme changes. Explicit Light and Dark overrides and separate custom palettes remain available. Existing profiles retain their previous preference; new profiles follow System.
- Fixed circle-selection minimum sizes at high zoom and made held circles take priority over overlapping scratch gestures. Circle to select must be enabled in Writing settings; circle the ink and briefly hold at the endpoint.
- Fixed Escape dismissal for Move page and Page templates dialogs, and preserved scroll access to settings and dialog controls on smaller windows.
- Fixed Windows numeric version resources and added verified setup EXE output to the platform build workflow.

Windows x64 installer and portable ZIP, Linux x86_64 Flatpak and complete corresponding source are published together. Existing 0.1.3 installations can use the Update notice; downloads begin only after clicking Update and installation finishes through Restart to update. Signed update metadata and the existing Flatpak signing channel are retained.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for manual installation and recovery and [RELEASE_VALIDATION.md](RELEASE_VALIDATION.md) for verification.

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

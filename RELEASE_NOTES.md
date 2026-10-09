# Folio 0.1.2

Folio 0.1.2 includes all application changes since 0.1.1 and the performance optimization pass. Downloads include a **Windows 11 x64 installer**, **portable Windows ZIP**, and **Linux x86_64 Flatpak**, with offline math and PDF preview tools.

## Install or update

On Windows, run **`folio-0.1.2-windows-x64-setup.exe`**. No administrator rights are needed. Close an existing Folio window before updating. Notes and downloaded OCR assets stay under `%LOCALAPPDATA%\Folio`; the installer and uninstall preserve them. For portable use, extract **`folio-0.1.2-windows-x64.zip`** and launch `bin/folio.exe`, keeping all sibling folders together.

On Linux, install **`folio-0.1.2-x86_64.flatpak`** with:

```bash
flatpak install --user ./folio-0.1.2-x86_64.flatpak
flatpak run io.github.folio.Notes
```

The Flatpak uses Freedesktop Platform 25.08. Installation may download the runtime. Both platforms bundle offline SymPy/mpmath math tools and PDF previews; Windows includes embedded Python and the matching Visual C++ runtime. The pinned GLM-OCR model/runtime assets download on the first recognition request (about 1.47 GB), then recognition works offline. Notes, handwriting and PDFs remain local.

## Changes since 0.1.1

- **Performance:** indexed search-row lookups remove unrelated-library work from saves; deferred edits stop cloning the entire undo history and building unused persistence snapshots. Cover/thumbnail, text-layout, formula, accessibility and unchanged translucent-ink caches reduce repeated work. Dense canvas queries, large-fragment restyling, page lookups, eraser cuts, folder traversal, OCR image preparation and exports use less work and fewer allocations.
- **Measured CPU gains:** approximately 118× for saves with 10,000 unrelated pages, 253× for restyling a 2,000-point fragment over a 10,000-point trace, 12× for dense multicell queries and 25× for unchanged translucent repaints. These are specific generated benchmarks, not whole-app frame-rate or physical pen-latency claims. The [performance audit](https://github.com/juccul/Folio/blob/v0.1.2/PERFORMANCE_AUDIT.md) includes fixtures, raw-result locations, reproduction commands and remaining costs.
- **Settings and appearance:** focused settings sections, aligned controls, shared interactive color pickers, usable scaled layouts, neutral light/dark themes, separate custom colors and paper appearance, and clearer writing controls.
- **Library and folders:** stable collapsible folder trees and breadcrumbs; validated names and full-path destination pickers; deletion explanations and access to a folder's hidden Trash; distinct recently opened documents; persisted sorting; accessible favorites with reliable mouse/keyboard actions and preserved edit times.
- **Workspace and onboarding:** quick creation with remembered paper choices; restored tabs/pages and startup library; tab navigation and consistent shortcuts; an empty library that stays empty until writing starts; clear first-writing actions and an optional reusable starter document.
- **Writing and selections:** remembered named pen presets and settings per tool; separate eraser modes/size; reliable sparse/tapered stroke intersection; visible long text drafts/carets, field undo/redo and word navigation; compact Cut/Copy/Delete/Solve selection actions and context-menu Paste at the clicked position.
- **Documents and exports:** responsive scrolling PDF previews; physical page units with correct PDF conversion; faithful background thumbnail rendering; explicit export previews/output appearance; page bookmarks, transfers, templates, notebook sharing and backup/restore retained.
- **Recognition and recovery:** explicit consent before first-use OCR downloads and pause controls; stale handwriting indexes retain reviewed drafts; equation drafts survive render failures; failed live calculations show stale state and retry; failed/provisional imports and first strokes remain recoverable; trashed documents are read-only; transient preview errors do not hide persistent save warnings.
- **Accessibility and validation:** semantic choices, tabs, page items and toggles expose their states; dialog/background shortcut isolation; error announcements; native action, pen/save, layout and package checks on throwaway data. Native verification scripts now follow the current controls, menu structure and visible scroll targets.

## Library compatibility

**Opening a library upgrades its database transactionally to schema 6. Folio 0.1.1 and earlier cannot open the upgraded library.** Original documents, raw ink and undo history are preserved; document format remains 4. Make any desired backup before the first launch if you need to return to an older version. The installers themselves do not edit the notes database.

## Verification and source

See [RELEASE_VALIDATION.md](https://github.com/juccul/Folio/blob/v0.1.2/RELEASE_VALIDATION.md) for the actual build, package, upgrade and native-test results. `SHA256SUMS` accompanies the release downloads. Windows users can compare `Get-FileHash .\folio-0.1.2-windows-x64-setup.exe -Algorithm SHA256` with that file.

The [v0.1.2 source tag](https://github.com/juccul/Folio/tree/v0.1.2) includes matching application source, Cargo.lock, local dependency patches, build/package scripts, notices and retained PDF-tool source. Folio is GPL-3.0-or-later; third-party components retain their licenses.

Physical pen latency, palm rejection, hardware eraser/barrel buttons, mixed-DPI and reconnect behavior still require hardware testing. Windows uses native Windows Ink; legacy WinTab-only devices and tablet pad controls remain outside that implementation. The synthetic tests and VirtualBox session establish functionality, not physical-device or recognition-accuracy certification.

# Folio 0.2.2

- A browser-style find bar replaces the search dialog: results update as you type, Enter/Shift+Enter, F3 and Ctrl+G step through matches, and a counter shows the current match.
- The text formatting bar groups its controls as icons, with a font size stepper, shortcut hints and a searchable font list that previews each family in its own face.
- Menus, dialogs, sidebars and the tool wheel animate out as well as in. Reduce motion turns this off.
- Dark mode is easier to read: pages default to #0e0e0e on a grey desk with a visible edge, ink and typed text are adjusted for contrast, and PDF and image covers are dimmed.
- The active tool, selection boxes and switches use a monochrome highlight. Selections have padding, larger handles and a centred action bar with shorter labels.
- The library list uses single-line rows with page count and a short edit age (5min, 3d, 4w, 2m, 1y), used everywhere. Blank covers stay plain and a failed import shows an alert with "Open to retry".
- The Open a document dialog shows whole rows sized to the window with a count of the rest, and marks failed imports.
- Settings fits its sections without cutting off rows, and Appearance groups Theme, Paper and ink, and Interface styling.
- Folder views show a "‹ Documents" back link. Pen menus show a stroke sample for each pen.
- Zooming, scrolling and interactive Wayland resizing stay smoother on dense pages, and multi-GPU Linux machines render on the display GPU by default.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak and desktop archive, checksums and complete corresponding source are published together. Updates remain signed and user-initiated.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.

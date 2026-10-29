# Folio 0.1.7

- Keyboard use follows desktop conventions: Tab and Shift+Tab move through controls and dialog fields, shortcuts keep working after a menu closes, and Escape closes only the topmost menu or dialog. Backspace deletes a selection, Ctrl+Y redoes, and search and the document picker accept Up, Down and Enter.
- A text box's edits are one undo step, and boxes left empty are removed. Text fields support double- and triple-click selection, Shift+click, Shift+Home/End, word deletion, a right-click menu and placeholders; dialogs select prefilled names.
- Renaming, tags, favorites, Trash and folder moves have their own Undo in the library (also Ctrl+Z there) instead of entering a document's undo history. Trash adds Delete permanently and Empty Trash, both confirmed. New documents receive distinct names, and the document picker shows folders and edit times.
- The save indicator reflects the actual save state. With autosave off, tabs mark unsaved documents and closing asks whether to save. The canvas menu offers Cut, Copy, Paste, Delete and Select all; ink color opens a swatch menu; tabs, page thumbnails and menus support common mouse actions; search results highlight their matches.
- Menus and dialogs use consistent labels, keep their buttons visible, disable unavailable actions instead of reporting errors, and no longer list duplicate actions.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak, checksums and complete corresponding source are published together. Updates remain signed and user-initiated.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.

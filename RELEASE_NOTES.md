# Folio 0.2.6

Folio's interface moves from GPUI to the [Rune](https://github.com/anomalyco/rune) UI toolkit. Your libraries, notes, preferences and recognition models carry over unchanged, and 0.2.5 updates to 0.2.6 in place.

- **Faster and lighter**: the interface and the canvas draw on the GPU. In our measurements it reaches its first frame as fast as 0.2.5 or faster (up to about a quarter faster on a new library) and needs roughly a third of the private memory; idle windows stop drawing.
- **Tour of smart ink**: a full-window walkthrough of circle to select, scratch out, hold for shapes, auto math, convert, solve and find, each step playing a clip of the real app, over a dot grid that ripples under your pointer. It opens once after this update and in new libraries, and stays available in Help and in Settings → Writing.
- **Menus that morph**: dialogs chosen from a menu (rename, new folder, recognition review, pickers) grow out of the menu instead of opening a second window; menus grow from what opened them and shrink back into it. Selecting ink with a loop morphs the loop into the selection box.
- **Library**: the search button widens into a bar that filters documents by title as you type; empty folders and the library read cleaner; the document picker (Ctrl+T) previews the chosen document's cover.
- **Canvas**: the page glides to its new centre when a sidebar opens or closes; Linux draws its own window frame again.
- **Fixes**: on Windows, fast pen lifts no longer drop the end of a stroke, and digitizers with an offset origin map to the right place; importing a file with a blank name no longer closes Folio; the update check no longer keeps the window busy while it runs; a stopped update service is reported instead of checking forever; the Windows update helper no longer refuses to run when another user's Python tool is open.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak, checksums and complete corresponding source are published together with the bundled `recognition/` models. Updates remain signed and user-initiated.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.

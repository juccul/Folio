# Folio 0.1.9

- Faster startup: library loading overlaps graphics initialization, and large databases use bounded read-ahead before the complete SQLite check. Validation and save durability stay unchanged. Starting Folio again activates the existing window for the same library.
- Document actions, Export and Settings use compact groups with clearer outlines and section icons.
- Pasted text and images stay selected, ready to move or resize. Without a pointer over the canvas, content lands in the visible view. Drawing outside a pasted selection returns to the previous ink tool.
- Images retain their aspect ratio at corner handles and can stretch along one axis at side handles. Existing previews stay visible during moves, resizing and rotation, and the selection toolbar follows the dragged selection.
- A stationary pen long press selects text or images. The pen cursor ring adapts to the background. Smart Ink gestures are enabled by default for new profiles, arrows accept irregular and multiple strokes, and erasers remove shapes they touch.
- The math solver stays warm between requests, reducing repeated solve delays.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak and desktop archive, checksums and complete corresponding source are published together. Updates remain signed and user-initiated.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.

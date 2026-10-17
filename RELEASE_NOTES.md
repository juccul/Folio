# Folio 0.1.6

- Faster OCR preparation uses lighter ink supersampling and keeps image/PDF crops in memory. A smaller, bounded context reduces native model memory while preserving the supported input and output limits.
- Repeated recognition of unchanged sources reuses a bounded result cache. Hovering an OCR control prepares an installed model; requests reuse that load, healthy models survive ordinary request errors, and successful device/fallback choices are remembered for the app session.
- Live handwriting calculations start at the end of the existing 700 ms writing pause without waiting for the periodic linked-math scan. Review, correction, cancellation and undo workflows remain available.
- Lower editor memory use releases inactive documents, obsolete previews and GPU resources sooner. Large-page geometry, sidebar navigation and text interaction avoid repeated work; worker cleanup and failed-load recovery are more robust.
- Optional OCR timing traces correlate requests with completed visible UI frames. Native packs retain the quality profile by default; the explicit `fast-lines` profile trades some single-line text resolution for speed.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak, checksums and complete corresponding source are published together. Updates remain signed and user-initiated.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.

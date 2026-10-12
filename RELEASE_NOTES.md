# Folio 0.1.4

- OCR automatically reduces GPU memory use when full GPU loading or inference fails: memory fitting places language layers and their attention cache in RAM while retaining GPU image encoding, followed by GPU image encoding alone and CPU-only recognition if needed. The 16,384-token context is preserved.
- Faster OCR preparation: verified assets and Vulkan device discovery are reused across recognition requests, cancellation and server restarts.
- Recognition status identifies memory fitting, GPU image encoding and CPU execution. Mixed modes can run more slowly than full GPU inference; performance depends on the hardware and available memory.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak, checksums and complete corresponding source are published together. Updates remain signed and user-initiated.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.

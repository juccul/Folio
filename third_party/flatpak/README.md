# Flatpak corresponding source

`poppler-26.10.0.tar.xz` is the unmodified Poppler source used in Folio's Flatpak PDF preview tools, retained under its included GPL-2.0-or-later COPYING notice.

Upstream: https://poppler.freedesktop.org/poppler-26.10.0.tar.xz

SHA-256: `6792cb7c69205007ad87d2e936cecc5b3a31fac29ab54ffc3175fdb6b2a6ce35`

The matching Folio source, build scripts, local dependency patches, Cargo.lock and third-party notices are in the release tag. Cargo retrieves the exact unmodified registry sources using the lockfile. `python3 scripts/package.py --no-build --binary PATH` also generates a complete source archive with the vendored Rust graph for offline builds. SymPy and mpmath Python source and notices are included in the Flatpak itself. Downloaded GLM-OCR/llama.cpp assets remain separate upstream MIT components.

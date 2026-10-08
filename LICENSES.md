# Third-party licenses

Folio's original source is GPL-3.0-or-later; see [LICENSE](LICENSE). Dependencies retain their own licenses. This inventory is generated from Cargo.lock's resolved Linux and Windows graphs, including build dependencies. It records package-declared SPDX expressions without replacing upstream notices. Slash-separated legacy dual-license declarations mean alternatives.

For dual-licensed dependencies, select MIT where offered, otherwise Apache-2.0 where offered. In particular, `oo7` is used under Apache-2.0 rather than its alternative GPL-2.0-only license. MPL-2.0 `option-ext` is unmodified, and its source is available from the exact version linked below. Binary distributors must provide the matching Folio source and retained notices; the package script creates both archives.

## Bundled components and system tools

- First-use recognition downloads pinned GLM-OCR Q8_0 weights and the llama.cpp b11457 Vulkan runtime separately from the application. Both upstream components declare MIT licenses. Retained model attribution/card and llama.cpp license are in [third_party/ocr](third_party/ocr); setup copies model notices into the downloaded pack and preserves the runtime archive LICENSE. Model inference is local; the SDK and layout detector are not included.
- Optional Python recognition: GLM-OCR weights are declared MIT in the upstream model card. One local model handles text and math through native Transformers classes; no GLM SDK or layout-detector code is bundled. Setup retains the model card, attribution, pinned revision and file hashes. Python (PSF), PyTorch (BSD-3-Clause), torchvision (BSD-3-Clause), Transformers (Apache-2.0), NumPy (BSD-3-Clause) and Pillow (HPND) are optional separately installed components. The standard archive contains no model, Python wheel, CUDA runtime or dataset. Earlier ConvText inference modules remain as historical source under [third_party/htr-convtext](third_party/htr-convtext), with their GPL-3.0 license; current recognition does not load them.
- GPUI 0.2.2: Apache-2.0; local tablet/rendering patches are documented in [vendor/README.md](vendor/README.md). All upstream notices are retained.
- xattr 0.2.3: MIT or Apache-2.0; local Linux ENODATA compatibility patch.
- proc-macro-error2 2.0.1: MIT or Apache-2.0; local public proc_macro re-export compatibility patch.
- FreeType: the FreeType License (FTL), selected instead of GPL-2.0. Portions of this software are copyright © The FreeType Project (www.freetype.org). All rights reserved. The bundled FreeType and HarfBuzz notices are included under `freetype-sys`.
- SQLite's bundled engine is public domain; the Rust bindings retain their MIT license. See the SQLite source header in the corresponding `libsqlite3-sys` source package.
- STIX Two Math bundled by latex-rust: SIL Open Font License 1.1, with font copyright and notices copied under latex-rust. Equation SVG output consists of glyph outlines.
- Poppler (`pdfinfo`, `pdftoppm`): GPL-2.0-or-later. RPM/DEB use the distribution package; Flatpak bundles Poppler 26.10.0 with its COPYING notice and corresponding source in the release source archive. Original PDFs remain local.
- Packaged offline math: SymPy 1.14.0 (BSD-3-Clause) and mpmath 1.3.0 (BSD-3-Clause), including their source and distribution license notices. System Python is used, not bundled.
- Passive tablet-link diagnostic: uses separately installed [PyGObject](https://pygobject.gnome.org/) (LGPL-2.1-or-later) and [GLib/Gio](https://github.com/GNOME/glib/blob/main/COPYING) (LGPL-2.1-or-later). These system components are not bundled or required by the desktop application.
- Bluetooth HCI diagnostic: uses the separately installed BlueZ `btmon` executable (GPL-2.0-or-later), verified from this system's BlueZ RPM metadata. It is not bundled or required by the desktop application.
- Temporary per-connection sniff experiment and foreground compatibility helper: use installed BlueZ `libbluetooth.so.3` (GPL-2.0-or-later, verified from `bluez-libs` RPM metadata) through Python ctypes. Function signatures and native ABI were verified using upstream BlueZ 5.87 headers. No library binary is bundled, and the desktop application does not depend on these diagnostics.
- Krilla/krilla-svg: MIT or Apache-2.0, searchable subset-font vector PDF overlays; lopdf: MIT, original PDF object preservation and decryption. AccessKit/Unix adapter: MIT or Apache-2.0, local AT-SPI controls bridge.
- System fonts, Vulkan drivers, glibc, XCB and xkbcommon come from the Linux installation and are not copied into the binary bundle. Their distro packages retain their respective licenses.

The UI uses 37 embedded Tabler Outline SVGs by Paweł Kuna, licensed under MIT, with stroke width adjusted to 1.65. Source mappings, the pinned revision, upstream attribution and full license are retained in [third_party/licenses/tabler-icons](third_party/licenses/tabler-icons). The matching marker icon and notebook cover artwork are original Folio vector drawings under GPL-3.0-or-later. Goodnotes documentation was consulted for layout; no Goodnotes assets or source are redistributed.

The ink/geometric algorithms are original implementations. No perfect-freehand, Xournal++ or Rnote source was copied. [DEVELOPMENT.md](DEVELOPMENT.md) explains the design references and tradeoffs.

## Locked Rust dependencies

| Package / source | Version | Declared license | Retained text |
| --- | --- | --- | --- |
| [accesskit](https://github.com/AccessKit/accesskit) | 0.25.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/accesskit-0.25.1/) |
| [accesskit_atspi_common](https://github.com/AccessKit/accesskit) | 0.21.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/accesskit_atspi_common-0.21.0/) |
| [accesskit_consumer](https://github.com/AccessKit/accesskit) | 0.39.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/accesskit_consumer-0.39.1/) |
| [accesskit_unix](https://github.com/AccessKit/accesskit) | 0.24.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/accesskit_unix-0.24.0/) |
| [accesskit_windows](https://github.com/AccessKit/accesskit) | 0.35.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/accesskit_windows-0.35.1/) |
| [adler2](https://github.com/oyvindln/adler2) | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | [notices](third_party/licenses/adler2-2.0.1/) |
| [aes](https://github.com/RustCrypto/block-ciphers) | 0.8.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/aes-0.8.4/) |
| [aes](https://github.com/RustCrypto/block-ciphers) | 0.9.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/aes-0.9.3/) |
| [ahash](https://github.com/tkaitchuck/ahash) | 0.8.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/ahash-0.8.12/) |
| [aho-corasick](https://github.com/BurntSushi/aho-corasick) | 1.1.5 | Unlicense OR MIT | [notices](third_party/licenses/aho-corasick-1.1.5/) |
| [aligned](https://github.com/rust-embedded-community/aligned) | 0.4.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/aligned-0.4.3/) |
| [aligned-vec](https://github.com/sarah-ek/aligned-vec/) | 0.6.4 | MIT | [notices](third_party/licenses/aligned-vec-0.6.4/) |
| [alloc-no-stdlib](https://github.com/dropbox/rust-alloc-no-stdlib) | 2.0.4 | BSD-3-Clause | [notices](third_party/licenses/alloc-no-stdlib-2.0.4/) |
| [alloc-stdlib](https://github.com/dropbox/rust-alloc-no-stdlib) | 0.2.4 | BSD-3-Clause | [notices](third_party/licenses/alloc-stdlib-0.2.4/) |
| [anyhow](https://github.com/dtolnay/anyhow) | 1.0.104 | MIT OR Apache-2.0 | [notices](third_party/licenses/anyhow-1.0.104/) |
| [ar_archive_writer](https://github.com/rust-lang/ar_archive_writer) | 0.5.3 | Apache-2.0 WITH LLVM-exception | [notices](third_party/licenses/ar_archive_writer-0.5.3/) |
| [arg_enum_proc_macro](https://github.com/lu-zero/arg_enum_proc_macro) | 0.3.4 | MIT | [notices](third_party/licenses/arg_enum_proc_macro-0.3.4/) |
| [arrayref](https://github.com/droundy/arrayref) | 0.3.9 | BSD-2-Clause | [notices](third_party/licenses/arrayref-0.3.9/) |
| [arrayvec](https://github.com/bluss/arrayvec) | 0.7.8 | MIT OR Apache-2.0 | [notices](third_party/licenses/arrayvec-0.7.8/) |
| [as-raw-xcb-connection](https://github.com/psychon/as-raw-xcb-connection) | 1.0.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/as-raw-xcb-connection-1.0.1/) |
| [as-slice](https://github.com/japaric/as-slice) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/as-slice-0.2.1/) |
| [ash](https://github.com/ash-rs/ash) | 0.38.0+1.3.281 | MIT OR Apache-2.0 | [notices](third_party/licenses/ash-0.38.0+1.3.281/) |
| [ash-window](https://github.com/ash-rs/ash) | 0.13.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/ash-window-0.13.0/) |
| [ashpd](https://github.com/bilelmoussaoui/ashpd) | 0.11.1 | MIT | [notices](third_party/licenses/ashpd-0.11.1/) |
| [ashpd](https://github.com/bilelmoussaoui/ashpd) | 0.12.3 | MIT | [notices](third_party/licenses/ashpd-0.12.3/) |
| [async-broadcast](https://github.com/smol-rs/async-broadcast) | 0.7.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/async-broadcast-0.7.2/) |
| [async-channel](https://github.com/smol-rs/async-channel) | 1.9.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-channel-1.9.0/) |
| [async-channel](https://github.com/smol-rs/async-channel) | 2.5.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-channel-2.5.0/) |
| [async-compression](https://github.com/Nullus157/async-compression) | 0.4.50 | MIT OR Apache-2.0 | [notices](third_party/licenses/async-compression-0.4.50/) |
| [async-executor](https://github.com/smol-rs/async-executor) | 1.14.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-executor-1.14.0/) |
| [async-fs](https://github.com/smol-rs/async-fs) | 2.2.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-fs-2.2.0/) |
| [async-global-executor](https://github.com/Keruspe/async-global-executor) | 2.4.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-global-executor-2.4.1/) |
| [async-io](https://github.com/smol-rs/async-io) | 2.6.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-io-2.6.0/) |
| [async-lock](https://github.com/smol-rs/async-lock) | 3.4.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-lock-3.4.2/) |
| [async-net](https://github.com/smol-rs/async-net) | 2.0.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-net-2.0.0/) |
| [async-process](https://github.com/smol-rs/async-process) | 2.5.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-process-2.5.0/) |
| [async-recursion](https://github.com/dcchut/async-recursion) | 1.1.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/async-recursion-1.1.1/) |
| [async-signal](https://github.com/smol-rs/async-signal) | 0.2.14 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-signal-0.2.14/) |
| [async-std](https://github.com/async-rs/async-std) | 1.13.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-std-1.13.2/) |
| [async-task](https://github.com/smol-rs/async-task) | 4.7.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-task-4.7.1/) |
| [async-trait](https://github.com/dtolnay/async-trait) | 0.1.92 | MIT OR Apache-2.0 | [notices](third_party/licenses/async-trait-0.1.92/) |
| [async_zip](https://github.com/Majored/rs-async-zip) | 0.0.17 | MIT | [notices](third_party/licenses/async_zip-0.0.17/) |
| [atomic](https://github.com/Amanieu/atomic-rs) | 0.5.3 | Apache-2.0/MIT | [notices](third_party/licenses/atomic-0.5.3/) |
| [atomic-waker](https://github.com/smol-rs/atomic-waker) | 1.1.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/atomic-waker-1.1.2/) |
| [atspi](https://github.com/odilia-app/atspi) | 0.29.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/atspi-0.29.0/) |
| [atspi-common](https://github.com/odilia-app/atspi) | 0.13.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/atspi-common-0.13.0/) |
| [atspi-proxies](https://github.com/odilia-app/atspi) | 0.13.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/atspi-proxies-0.13.0/) |
| [autocfg](https://github.com/cuviper/autocfg) | 1.5.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/autocfg-1.5.1/) |
| [av-scenechange](https://github.com/rust-av/av-scenechange) | 0.14.1 | MIT | [notices](third_party/licenses/av-scenechange-0.14.1/) |
| [av1-grain](https://github.com/rust-av/av1-grain) | 0.2.5 | BSD-2-Clause | [notices](third_party/licenses/av1-grain-0.2.5/) |
| [avif-serialize](https://github.com/kornelski/avif-serialize) | 0.8.9 | BSD-3-Clause | [notices](third_party/licenses/avif-serialize-0.8.9/) |
| [base64](https://github.com/marshallpierce/rust-base64) | 0.22.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/base64-0.22.1/) |
| [base64](https://github.com/marshallpierce/rust-base64) | 0.23.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/base64-0.23.1/) |
| [bit-set](https://github.com/contain-rs/bit-set) | 0.8.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/bit-set-0.8.0/) |
| [bit-vec](https://github.com/contain-rs/bit-vec) | 0.8.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/bit-vec-0.8.0/) |
| [bit_field](https://github.com/phil-opp/rust-bit-field) | 0.10.3 | Apache-2.0/MIT | [notices](third_party/licenses/bit_field-0.10.3/) |
| [bitflags](https://github.com/bitflags/bitflags) | 1.3.2 | MIT/Apache-2.0 | [notices](third_party/licenses/bitflags-1.3.2/) |
| [bitflags](https://github.com/bitflags/bitflags) | 2.13.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/bitflags-2.13.2/) |
| [bitstream-io](https://github.com/tuffy/bitstream-io) | 4.10.0 | MIT/Apache-2.0 | [notices](third_party/licenses/bitstream-io-4.10.0/) |
| [blade-graphics](https://github.com/kvark/blade) | 0.7.1 | MIT | [notices](third_party/licenses/blade-graphics-0.7.1/) |
| [blade-macros](https://github.com/kvark/blade) | 0.3.0 | MIT | [notices](third_party/licenses/blade-macros-0.3.0/) |
| [blade-util](https://github.com/kvark/blade) | 0.3.0 | MIT | [notices](third_party/licenses/blade-util-0.3.0/) |
| [block-buffer](https://github.com/RustCrypto/utils) | 0.10.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/block-buffer-0.10.4/) |
| [block-buffer](https://github.com/RustCrypto/utils) | 0.12.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/block-buffer-0.12.1/) |
| [block-padding](https://github.com/RustCrypto/utils) | 0.3.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/block-padding-0.3.3/) |
| [block-padding](https://github.com/RustCrypto/utils) | 0.4.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/block-padding-0.4.2/) |
| [blocking](https://github.com/smol-rs/blocking) | 1.7.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/blocking-1.7.0/) |
| [brotli-decompressor](https://github.com/dropbox/rust-brotli-decompressor) | 5.0.3 | BSD-3-Clause/MIT | [notices](third_party/licenses/brotli-decompressor-5.0.3/) |
| [bstr](https://github.com/BurntSushi/bstr) | 1.13.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/bstr-1.13.1/) |
| [built](https://github.com/lukaslueg/built) | 0.8.1 | MIT | [notices](third_party/licenses/built-0.8.1/) |
| [bumpalo](https://github.com/fitzgen/bumpalo) | 3.20.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/bumpalo-3.20.3/) |
| [bytemuck](https://github.com/Lokathor/bytemuck) | 1.25.2 | Zlib OR Apache-2.0 OR MIT | [notices](third_party/licenses/bytemuck-1.25.2/) |
| [bytemuck_derive](https://github.com/Lokathor/bytemuck) | 1.12.1 | Zlib OR Apache-2.0 OR MIT | [notices](third_party/licenses/bytemuck_derive-1.12.1/) |
| [byteorder](https://github.com/BurntSushi/byteorder) | 1.5.0 | Unlicense OR MIT | [notices](third_party/licenses/byteorder-1.5.0/) |
| [byteorder-lite](https://github.com/image-rs/byteorder-lite) | 0.1.0 | Unlicense OR MIT | [notices](third_party/licenses/byteorder-lite-0.1.0/) |
| [bytes](https://github.com/tokio-rs/bytes) | 1.12.1 | MIT | [notices](third_party/licenses/bytes-1.12.1/) |
| [calloop](https://github.com/Smithay/calloop) | 0.13.0 | MIT | [notices](third_party/licenses/calloop-0.13.0/) |
| [calloop-wayland-source](https://github.com/smithay/calloop-wayland-source) | 0.3.0 | MIT | [notices](third_party/licenses/calloop-wayland-source-0.3.0/) |
| [cbc](https://github.com/RustCrypto/block-modes) | 0.1.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/cbc-0.1.2/) |
| [cbc](https://github.com/RustCrypto/block-modes) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/cbc-0.2.1/) |
| [cc](https://github.com/rust-lang/cc-rs) | 1.5.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/cc-1.5.1/) |
| [cfg-if](https://github.com/rust-lang/cfg-if) | 1.0.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/cfg-if-1.0.5/) |
| [cfg_aliases](https://github.com/katharostech/cfg_aliases) | 0.2.2 | MIT | [notices](third_party/licenses/cfg_aliases-0.2.2/) |
| [chacha20](https://github.com/RustCrypto/stream-ciphers) | 0.10.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/chacha20-0.10.2/) |
| [cipher](https://github.com/RustCrypto/traits) | 0.4.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/cipher-0.4.4/) |
| [cipher](https://github.com/RustCrypto/traits) | 0.5.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/cipher-0.5.2/) |
| [codespan-reporting](https://github.com/brendanzab/codespan) | 0.12.0 | Apache-2.0 | [notices](third_party/licenses/codespan-reporting-0.12.0/) |
| [color_quant](https://github.com/image-rs/color_quant.git) | 1.1.0 | MIT | [notices](third_party/licenses/color_quant-1.1.0/) |
| [command-fds](https://github.com/google/command-fds/) | 0.3.3 | Apache-2.0 | [notices](third_party/licenses/command-fds-0.3.3/) |
| [compression-codecs](https://github.com/Nullus157/async-compression) | 0.4.45 | MIT OR Apache-2.0 | [notices](third_party/licenses/compression-codecs-0.4.45/) |
| [compression-core](https://github.com/Nullus157/async-compression) | 0.4.33 | MIT OR Apache-2.0 | [notices](third_party/licenses/compression-core-0.4.33/) |
| [concurrent-queue](https://github.com/smol-rs/concurrent-queue) | 2.5.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/concurrent-queue-2.5.0/) |
| [const-oid](https://github.com/RustCrypto/formats) | 0.10.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/const-oid-0.10.2/) |
| [const-random](https://github.com/tkaitchuck/constrandom) | 0.1.18 | MIT OR Apache-2.0 | [notices](third_party/licenses/const-random-0.1.18/) |
| [const-random-macro](https://github.com/tkaitchuck/constrandom) | 0.1.16 | MIT OR Apache-2.0 | [notices](third_party/licenses/const-random-macro-0.1.16/) |
| [convert_case](https://github.com/rutrum/convert-case) | 0.4.0 | MIT | [notices](third_party/licenses/convert_case-0.4.0/) |
| [core_detect](https://github.com/thomcc/core_detect) | 1.0.0 | MIT/Apache-2.0 | [notices](third_party/licenses/core_detect-1.0.0/) |
| [core_maths](https://github.com/robertbastian/core_maths) | 0.1.1 | MIT | [notices](third_party/licenses/core_maths-0.1.1/) |
| [cosmic-text](https://github.com/pop-os/cosmic-text) | 0.14.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/cosmic-text-0.14.2/) |
| [cpubits](https://github.com/RustCrypto/utils) | 0.1.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/cpubits-0.1.1/) |
| [cpufeatures](https://github.com/RustCrypto/utils) | 0.2.17 | MIT OR Apache-2.0 | [notices](third_party/licenses/cpufeatures-0.2.17/) |
| [cpufeatures](https://github.com/RustCrypto/utils) | 0.3.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/cpufeatures-0.3.1/) |
| [crc32fast](https://github.com/srijs/rust-crc32fast) | 1.5.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/crc32fast-1.5.2/) |
| [crossbeam-deque](https://github.com/crossbeam-rs/crossbeam) | 0.8.8 | MIT OR Apache-2.0 | [notices](third_party/licenses/crossbeam-deque-0.8.8/) |
| [crossbeam-epoch](https://github.com/crossbeam-rs/crossbeam) | 0.9.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/crossbeam-epoch-0.9.21/) |
| [crossbeam-queue](https://github.com/crossbeam-rs/crossbeam) | 0.3.14 | MIT OR Apache-2.0 | [notices](third_party/licenses/crossbeam-queue-0.3.14/) |
| [crossbeam-utils](https://github.com/crossbeam-rs/crossbeam) | 0.8.23 | MIT OR Apache-2.0 | [notices](third_party/licenses/crossbeam-utils-0.8.23/) |
| [crunchy](https://github.com/eira-fransham/crunchy) | 0.2.4 | MIT | [notices](third_party/licenses/crunchy-0.2.4/) |
| [crypto-common](https://github.com/RustCrypto/traits) | 0.1.7 | MIT OR Apache-2.0 | [notices](third_party/licenses/crypto-common-0.1.7/) |
| [crypto-common](https://github.com/RustCrypto/traits) | 0.2.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/crypto-common-0.2.2/) |
| [ctor](https://github.com/mmastrac/rust-ctor) | 0.4.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/ctor-0.4.3/) |
| [ctor-proc-macro](https://github.com/mmastrac/rust-ctor) | 0.0.6 | Apache-2.0 OR MIT | [notices](third_party/licenses/ctor-proc-macro-0.0.6/) |
| [data-url](https://github.com/servo/rust-url) | 0.3.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/data-url-0.3.2/) |
| [deflate64](https://github.com/anatawa12/deflate64-rs) | 0.1.12 | MIT | [notices](third_party/licenses/deflate64-0.1.12/) |
| [derive_more](https://github.com/JelteF/derive_more) | 0.99.20 | MIT | [notices](third_party/licenses/derive_more-0.99.20/) |
| [digest](https://github.com/RustCrypto/traits) | 0.10.7 | MIT OR Apache-2.0 | [notices](third_party/licenses/digest-0.10.7/) |
| [digest](https://github.com/RustCrypto/traits) | 0.11.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/digest-0.11.3/) |
| [dirs](https://github.com/soc/dirs-rs) | 4.0.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/dirs-4.0.0/) |
| [dirs](https://github.com/soc/dirs-rs) | 5.0.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/dirs-5.0.1/) |
| [dirs-sys](https://github.com/dirs-dev/dirs-sys-rs) | 0.3.7 | MIT OR Apache-2.0 | [notices](third_party/licenses/dirs-sys-0.3.7/) |
| [dirs-sys](https://github.com/dirs-dev/dirs-sys-rs) | 0.4.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/dirs-sys-0.4.1/) |
| [displaydoc](https://github.com/yaahc/displaydoc) | 0.2.7 | MIT OR Apache-2.0 | [notices](third_party/licenses/displaydoc-0.2.7/) |
| [dlib](https://github.com/elinorbgr/dlib) | 0.5.3 | MIT | [notices](third_party/licenses/dlib-0.5.3/) |
| [downcast-rs](https://github.com/marcianx/downcast-rs) | 1.2.1 | MIT/Apache-2.0 | [notices](third_party/licenses/downcast-rs-1.2.1/) |
| [dtor](https://github.com/mmastrac/rust-ctor) | 0.0.6 | Apache-2.0 OR MIT | [notices](third_party/licenses/dtor-0.0.6/) |
| [dtor-proc-macro](https://github.com/mmastrac/rust-ctor) | 0.0.5 | Apache-2.0 OR MIT | [notices](third_party/licenses/dtor-proc-macro-0.0.5/) |
| [dunce](https://gitlab.com/kornelski/dunce) | 1.0.5 | CC0-1.0 OR MIT-0 OR Apache-2.0 | [notices](third_party/licenses/dunce-1.0.5/) |
| [dyn-clone](https://github.com/dtolnay/dyn-clone) | 1.0.20 | MIT OR Apache-2.0 | [notices](third_party/licenses/dyn-clone-1.0.20/) |
| [ecb](https://github.com/RustCrypto/block-modes) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/ecb-0.2.1/) |
| [either](https://github.com/rayon-rs/either) | 1.18.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/either-1.18.0/) |
| [embed-resource](https://github.com/nabijaczleweli/rust-embed-resource) | 3.0.11 | MIT | [notices](third_party/licenses/embed-resource-3.0.11/) |
| [encoding_rs](https://github.com/hsivonen/encoding_rs) | 0.8.42 | (Apache-2.0 OR MIT) AND BSD-3-Clause | [notices](third_party/licenses/encoding_rs-0.8.42/) |
| [endi](https://github.com/zeenix/endi) | 1.1.1 | MIT | [notices](third_party/licenses/endi-1.1.1/) |
| [enumflags2](https://github.com/meithecatte/enumflags2) | 0.7.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/enumflags2-0.7.12/) |
| [enumflags2_derive](https://github.com/meithecatte/enumflags2) | 0.7.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/enumflags2_derive-0.7.12/) |
| [equator](https://github.com/sarah-ek/equator/) | 0.4.2 | MIT | [notices](third_party/licenses/equator-0.4.2/) |
| [equator-macro](https://github.com/sarah-ek/equator/) | 0.4.2 | MIT | [notices](third_party/licenses/equator-macro-0.4.2/) |
| [equivalent](https://github.com/indexmap-rs/equivalent) | 1.0.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/equivalent-1.0.2/) |
| [erased-serde](https://github.com/dtolnay/erased-serde) | 0.4.10 | MIT OR Apache-2.0 | [notices](third_party/licenses/erased-serde-0.4.10/) |
| [errno](https://github.com/lambda-fairy/rust-errno) | 0.3.14 | MIT OR Apache-2.0 | [notices](third_party/licenses/errno-0.3.14/) |
| [etagere](https://github.com/nical/etagere) | 0.2.15 | MIT/Apache-2.0 | [notices](third_party/licenses/etagere-0.2.15/) |
| [euclid](https://github.com/servo/euclid) | 0.22.14 | MIT OR Apache-2.0 | [notices](third_party/licenses/euclid-0.22.14/) |
| [event-listener](https://github.com/smol-rs/event-listener) | 2.5.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/event-listener-2.5.3/) |
| [event-listener](https://github.com/smol-rs/event-listener) | 5.4.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/event-listener-5.4.2/) |
| [event-listener-strategy](https://github.com/smol-rs/event-listener-strategy) | 0.5.4 | Apache-2.0 OR MIT | [notices](third_party/licenses/event-listener-strategy-0.5.4/) |
| [exr](https://github.com/johannesvollmer/exrs) | 1.74.2 | BSD-3-Clause | [notices](third_party/licenses/exr-1.74.2/) |
| [fallible-iterator](https://github.com/sfackler/rust-fallible-iterator) | 0.3.0 | MIT/Apache-2.0 | [notices](third_party/licenses/fallible-iterator-0.3.0/) |
| [fallible-streaming-iterator](https://github.com/sfackler/fallible-streaming-iterator) | 0.1.9 | MIT/Apache-2.0 | [notices](third_party/licenses/fallible-streaming-iterator-0.1.9/) |
| [fastrand](https://github.com/smol-rs/fastrand) | 1.9.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/fastrand-1.9.0/) |
| [fastrand](https://github.com/smol-rs/fastrand) | 2.5.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/fastrand-2.5.0/) |
| [fax](https://github.com/pdf-rs/fax) | 0.2.7 | MIT | [notices](third_party/licenses/fax-0.2.7/) |
| [fdeflate](https://github.com/image-rs/fdeflate) | 0.3.7 | MIT OR Apache-2.0 | [notices](third_party/licenses/fdeflate-0.3.7/) |
| [filedescriptor](https://github.com/wezterm/wezterm) | 0.8.3 | MIT | [notices](third_party/licenses/filedescriptor-0.8.3/) |
| [filetime](https://github.com/alexcrichton/filetime) | 0.2.29 | MIT/Apache-2.0 | [notices](third_party/licenses/filetime-0.2.29/) |
| [find-msvc-tools](https://github.com/rust-lang/cc-rs) | 0.1.14 | MIT OR Apache-2.0 | [notices](third_party/licenses/find-msvc-tools-0.1.14/) |
| [flate2](https://github.com/rust-lang/flate2-rs) | 1.1.10 | MIT OR Apache-2.0 | [notices](third_party/licenses/flate2-1.1.10/) |
| [float-cmp](https://github.com/mikedilger/float-cmp) | 0.9.0 | MIT | [notices](third_party/licenses/float-cmp-0.9.0/) |
| [float-ord](https://github.com/notriddle/rust-float-ord) | 0.3.2 | MIT / Apache-2.0 | [notices](third_party/licenses/float-ord-0.3.2/) |
| [float_next_after](https://gitlab.com/bronsonbdevost/next_afterf) | 1.0.0 | MIT | [notices](third_party/licenses/float_next_after-1.0.0/) |
| [flume](https://github.com/zesterer/flume) | 0.11.1 | Apache-2.0/MIT | [notices](third_party/licenses/flume-0.11.1/) |
| [fnv](https://github.com/servo/rust-fnv) | 1.0.7 | Apache-2.0 / MIT | [notices](third_party/licenses/fnv-1.0.7/) |
| [foldhash](https://github.com/orlp/foldhash) | 0.1.5 | Zlib | [notices](third_party/licenses/foldhash-0.1.5/) |
| [foldhash](https://github.com/orlp/foldhash) | 0.2.0 | Zlib | [notices](third_party/licenses/foldhash-0.2.0/) |
| [font-types](https://github.com/googlefonts/fontations) | 0.11.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/font-types-0.11.3/) |
| [font-types](https://github.com/googlefonts/fontations) | 0.12.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/font-types-0.12.5/) |
| [fontconfig-parser](https://github.com/Riey/fontconfig-parser) | 0.5.8 | MIT | [notices](third_party/licenses/fontconfig-parser-0.5.8/) |
| [fontdb](https://github.com/RazrFalcon/fontdb) | 0.16.2 | MIT | [notices](third_party/licenses/fontdb-0.16.2/) |
| [fontdb](https://github.com/RazrFalcon/fontdb) | 0.23.0 | MIT | [notices](third_party/licenses/fontdb-0.23.0/) |
| [form_urlencoded](https://github.com/servo/rust-url) | 1.2.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/form_urlencoded-1.2.2/) |
| [freetype-sys](https://github.com/PistonDevelopers/freetype-sys.git) | 0.20.1 | MIT | [notices](third_party/licenses/freetype-sys-0.20.1/) |
| [futf](https://github.com/servo/futf) | 0.1.5 | MIT / Apache-2.0 | [notices](third_party/licenses/futf-0.1.5/) |
| [futures](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-0.3.34/) |
| [futures-channel](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-channel-0.3.34/) |
| [futures-core](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-core-0.3.34/) |
| [futures-executor](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-executor-0.3.34/) |
| [futures-io](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-io-0.3.34/) |
| [futures-lite](https://github.com/smol-rs/futures-lite) | 1.13.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/futures-lite-1.13.0/) |
| [futures-lite](https://github.com/smol-rs/futures-lite) | 2.6.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/futures-lite-2.6.1/) |
| [futures-macro](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-macro-0.3.34/) |
| [futures-sink](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-sink-0.3.34/) |
| [futures-task](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-task-0.3.34/) |
| [futures-util](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-util-0.3.34/) |
| [generic-array](https://github.com/fizyk20/generic-array.git) | 0.14.7 | MIT | [notices](third_party/licenses/generic-array-0.14.7/) |
| [gethostname](https://codeberg.org/swsnr/gethostname.rs.git) | 1.1.0 | Apache-2.0 | [notices](third_party/licenses/gethostname-1.1.0/) |
| [getrandom](https://github.com/rust-random/getrandom) | 0.2.17 | MIT OR Apache-2.0 | [notices](third_party/licenses/getrandom-0.2.17/) |
| [getrandom](https://github.com/rust-random/getrandom) | 0.3.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/getrandom-0.3.4/) |
| [getrandom](https://github.com/rust-random/getrandom) | 0.4.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/getrandom-0.4.3/) |
| [gif](https://github.com/image-rs/image-gif) | 0.14.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/gif-0.14.2/) |
| [globset](https://github.com/BurntSushi/ripgrep/tree/master/crates/globset) | 0.4.20 | Unlicense OR MIT | [notices](third_party/licenses/globset-0.4.20/) |
| [gpu-alloc](https://github.com/zakarumych/gpu-alloc) | 0.6.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/gpu-alloc-0.6.2/) |
| [gpu-alloc-ash](https://github.com/zakarumych/gpu-alloc) | 0.7.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/gpu-alloc-ash-0.7.1/) |
| [gpu-alloc-types](https://github.com/zakarumych/gpu-alloc) | 0.3.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/gpu-alloc-types-0.3.1/) |
| [gpui](https://github.com/zed-industries/zed) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui-0.2.2/) |
| [gpui-macros](https://crates.io/crates/gpui-macros/0.2.2) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui-macros-0.2.2/) |
| [gpui_collections](https://crates.io/crates/gpui_collections/0.2.2) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui_collections-0.2.2/) |
| [gpui_derive_refineable](https://crates.io/crates/gpui_derive_refineable/0.2.2) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui_derive_refineable-0.2.2/) |
| [gpui_http_client](https://crates.io/crates/gpui_http_client/0.2.2) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui_http_client-0.2.2/) |
| [gpui_perf](https://crates.io/crates/gpui_perf/0.2.2) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui_perf-0.2.2/) |
| [gpui_refineable](https://crates.io/crates/gpui_refineable/0.2.2) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui_refineable-0.2.2/) |
| [gpui_semantic_version](https://crates.io/crates/gpui_semantic_version/0.2.2) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui_semantic_version-0.2.2/) |
| [gpui_sum_tree](https://crates.io/crates/gpui_sum_tree/0.2.2) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui_sum_tree-0.2.2/) |
| [gpui_util](https://crates.io/crates/gpui_util/0.2.2) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui_util-0.2.2/) |
| [gpui_util_macros](https://crates.io/crates/gpui_util_macros/0.2.2) | 0.2.2 | Apache-2.0 | [notices](third_party/licenses/gpui_util_macros-0.2.2/) |
| [grid](https://github.com/becheran/grid) | 0.18.0 | MIT | [notices](third_party/licenses/grid-0.18.0/) |
| [h2](https://github.com/hyperium/h2) | 0.4.19 | MIT | [notices](third_party/licenses/h2-0.4.19/) |
| [half](https://github.com/VoidStarKat/half-rs) | 2.7.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/half-2.7.1/) |
| [hashbrown](https://github.com/rust-lang/hashbrown) | 0.14.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/hashbrown-0.14.5/) |
| [hashbrown](https://github.com/rust-lang/hashbrown) | 0.15.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/hashbrown-0.15.5/) |
| [hashbrown](https://github.com/rust-lang/hashbrown) | 0.17.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/hashbrown-0.17.1/) |
| [hashlink](https://github.com/djc/hashlink) | 0.12.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/hashlink-0.12.2/) |
| [heck](https://github.com/withoutboats/heck) | 0.5.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/heck-0.5.0/) |
| [hex](https://github.com/KokaKiwi/rust-hex) | 0.4.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/hex-0.4.3/) |
| [hexf-parse](https://github.com/lifthrasiir/hexf) | 0.2.1 | CC0-1.0 | [notices](third_party/licenses/hexf-parse-0.2.1/) |
| [hidden-trait](https://github.com/kvark/hidden-trait) | 0.1.2 | MIT | [notices](third_party/licenses/hidden-trait-0.1.2/) |
| [hkdf](https://github.com/RustCrypto/KDFs/) | 0.12.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/hkdf-0.12.4/) |
| [hmac](https://github.com/RustCrypto/MACs) | 0.12.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/hmac-0.12.1/) |
| [home](https://github.com/rust-lang/cargo) | 0.5.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/home-0.5.12/) |
| [http](https://github.com/hyperium/http) | 1.5.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/http-1.5.0/) |
| [http-body](https://github.com/hyperium/http-body) | 1.1.0 | MIT | [notices](third_party/licenses/http-body-1.1.0/) |
| [http-body-util](https://github.com/hyperium/http-body) | 0.1.5 | MIT | [notices](third_party/licenses/http-body-util-0.1.5/) |
| [httparse](https://github.com/seanmonstar/httparse) | 1.10.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/httparse-1.10.1/) |
| [hybrid-array](https://github.com/RustCrypto/hybrid-array) | 0.4.15 | MIT OR Apache-2.0 | [notices](third_party/licenses/hybrid-array-0.4.15/) |
| [hyper](https://github.com/hyperium/hyper) | 1.11.1 | MIT | [notices](third_party/licenses/hyper-1.11.1/) |
| [hyper-rustls](https://github.com/rustls/hyper-rustls) | 0.27.10 | Apache-2.0 OR ISC OR MIT | [notices](third_party/licenses/hyper-rustls-0.27.10/) |
| [hyper-util](https://github.com/hyperium/hyper-util) | 0.1.21 | MIT | [notices](third_party/licenses/hyper-util-0.1.21/) |
| [icu_collections](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_collections-2.3.0/) |
| [icu_locale_core](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_locale_core-2.3.0/) |
| [icu_normalizer](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_normalizer-2.3.0/) |
| [icu_normalizer_data](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_normalizer_data-2.3.0/) |
| [icu_properties](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_properties-2.3.0/) |
| [icu_properties_data](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_properties_data-2.3.0/) |
| [icu_provider](https://github.com/unicode-org/icu4x) | 2.3.1 | Unicode-3.0 | [notices](third_party/licenses/icu_provider-2.3.1/) |
| [idna](https://github.com/servo/rust-url/) | 1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/idna-1.1.0/) |
| [idna_adapter](https://github.com/hsivonen/idna_adapter) | 1.2.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/idna_adapter-1.2.2/) |
| [image](https://github.com/image-rs/image) | 0.25.10 | MIT OR Apache-2.0 | [notices](third_party/licenses/image-0.25.10/) |
| [image-webp](https://github.com/image-rs/image-webp) | 0.2.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/image-webp-0.2.4/) |
| [imagesize](https://github.com/Roughsketch/imagesize) | 0.13.0 | MIT | [notices](third_party/licenses/imagesize-0.13.0/) |
| [imagesize](https://github.com/Roughsketch/imagesize) | 0.14.0 | MIT | [notices](third_party/licenses/imagesize-0.14.0/) |
| [imgref](https://github.com/kornelski/imgref) | 1.12.3 | CC0-1.0 OR Apache-2.0 | [notices](third_party/licenses/imgref-1.12.3/) |
| [indexmap](https://github.com/indexmap-rs/indexmap) | 2.14.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/indexmap-2.14.2/) |
| [inout](https://github.com/RustCrypto/utils) | 0.1.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/inout-0.1.4/) |
| [inout](https://github.com/RustCrypto/utils) | 0.2.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/inout-0.2.2/) |
| [inventory](https://github.com/dtolnay/inventory) | 0.3.24 | MIT OR Apache-2.0 | [notices](third_party/licenses/inventory-0.3.24/) |
| [ipnet](https://github.com/krisprice/ipnet) | 2.12.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/ipnet-2.12.2/) |
| [is-docker](https://github.com/TheLarkInn/is-docker) | 0.2.0 | MIT | [notices](third_party/licenses/is-docker-0.2.0/) |
| [is-wsl](https://github.com/TheLarkInn/is-wsl) | 0.4.0 | MIT | [notices](third_party/licenses/is-wsl-0.4.0/) |
| [itertools](https://github.com/rust-itertools/itertools) | 0.14.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/itertools-0.14.0/) |
| [itoa](https://github.com/dtolnay/itoa) | 1.0.18 | MIT OR Apache-2.0 | [notices](third_party/licenses/itoa-1.0.18/) |
| [jobserver](https://github.com/rust-lang/jobserver-rs) | 0.1.35 | MIT OR Apache-2.0 | [notices](third_party/licenses/jobserver-0.1.35/) |
| [krilla](https://github.com/LaurenzV/krilla) | 0.8.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/krilla-0.8.2/) |
| [krilla-svg](https://github.com/LaurenzV/krilla) | 0.8.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/krilla-svg-0.8.1/) |
| [kurbo](https://github.com/linebender/kurbo) | 0.11.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/kurbo-0.11.3/) |
| [kurbo](https://github.com/linebender/kurbo) | 0.13.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/kurbo-0.13.1/) |
| [kv-log-macro](https://github.com/yoshuawuyts/kv-log-macro) | 1.0.7 | MIT OR Apache-2.0 | [notices](third_party/licenses/kv-log-macro-1.0.7/) |
| [latex-rust](https://github.com/jscarr64/LaTeX-Rust) | 2.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/latex-rust-2.1.0/) |
| [lazy_static](https://github.com/rust-lang-nursery/lazy-static.rs) | 1.5.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/lazy_static-1.5.1/) |
| [lebe](https://github.com/johannesvollmer/lebe) | 0.5.3 | BSD-3-Clause | [notices](third_party/licenses/lebe-0.5.3/) |
| [libc](https://github.com/rust-lang/libc) | 0.2.190 | MIT OR Apache-2.0 | [notices](third_party/licenses/libc-0.2.190/) |
| [libloading](https://github.com/nagisa/rust_libloading/) | 0.8.9 | ISC | [notices](third_party/licenses/libloading-0.8.9/) |
| [libm](https://github.com/rust-lang/compiler-builtins) | 0.2.16 | MIT | [notices](third_party/licenses/libm-0.2.16/) |
| [libsqlite3-sys](https://github.com/rusqlite/rusqlite) | 0.38.2 | MIT | [notices](third_party/licenses/libsqlite3-sys-0.38.2/) |
| [linux-raw-sys](https://github.com/sunfishcode/linux-raw-sys) | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [notices](third_party/licenses/linux-raw-sys-0.12.1/) |
| [linux-raw-sys](https://github.com/sunfishcode/linux-raw-sys) | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [notices](third_party/licenses/linux-raw-sys-0.4.15/) |
| [litemap](https://github.com/unicode-org/icu4x) | 0.8.3 | Unicode-3.0 | [notices](third_party/licenses/litemap-0.8.3/) |
| [lock_api](https://github.com/Amanieu/parking_lot) | 0.4.14 | MIT OR Apache-2.0 | [notices](third_party/licenses/lock_api-0.4.14/) |
| [log](https://github.com/rust-lang/log) | 0.4.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/log-0.4.34/) |
| [loop9](https://gitlab.com/kornelski/loop9.git) | 0.1.5 | MIT | [notices](third_party/licenses/loop9-0.1.5/) |
| [lopdf](https://github.com/J-F-Liu/lopdf.git) | 0.45.0 | MIT | [notices](third_party/licenses/lopdf-0.45.0/) |
| [lru-slab](https://github.com/Ralith/lru-slab) | 0.1.3 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/lru-slab-0.1.3/) |
| [lyon](https://github.com/nical/lyon) | 1.0.19 | MIT OR Apache-2.0 | [notices](third_party/licenses/lyon-1.0.19/) |
| [lyon_algorithms](https://github.com/nical/lyon) | 1.0.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/lyon_algorithms-1.0.21/) |
| [lyon_geom](https://github.com/nical/lyon) | 1.0.19 | MIT OR Apache-2.0 | [notices](third_party/licenses/lyon_geom-1.0.19/) |
| [lyon_path](https://github.com/nical/lyon) | 1.0.19 | MIT OR Apache-2.0 | [notices](third_party/licenses/lyon_path-1.0.19/) |
| [lyon_tessellation](https://github.com/nical/lyon) | 1.0.22 | MIT OR Apache-2.0 | [notices](third_party/licenses/lyon_tessellation-1.0.22/) |
| [mac](https://github.com/reem/rust-mac.git) | 0.1.1 | MIT/Apache-2.0 | [notices](third_party/licenses/mac-0.1.1/) |
| [maybe-rayon](https://github.com/shssoichiro/maybe-rayon) | 0.1.1 | MIT | [notices](third_party/licenses/maybe-rayon-0.1.1/) |
| [md-5](https://github.com/RustCrypto/hashes) | 0.10.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/md-5-0.10.6/) |
| [md-5](https://github.com/RustCrypto/hashes) | 0.11.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/md-5-0.11.0/) |
| [memchr](https://github.com/BurntSushi/memchr) | 2.8.3 | Unlicense OR MIT | [notices](third_party/licenses/memchr-2.8.3/) |
| [memmap2](https://github.com/RazrFalcon/memmap2-rs) | 0.9.11 | MIT OR Apache-2.0 | [notices](third_party/licenses/memmap2-0.9.11/) |
| [mime](https://github.com/hyperium/mime) | 0.3.17 | MIT OR Apache-2.0 | [notices](third_party/licenses/mime-0.3.17/) |
| [mime_guess](https://github.com/abonander/mime_guess) | 2.0.5 | MIT | [notices](third_party/licenses/mime_guess-2.0.5/) |
| [miniz_oxide](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) | 0.8.9 | MIT OR Zlib OR Apache-2.0 | [notices](third_party/licenses/miniz_oxide-0.8.9/) |
| [miniz_oxide](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) | 0.9.1 | MIT OR Zlib OR Apache-2.0 | [notices](third_party/licenses/miniz_oxide-0.9.1/) |
| [mint](https://github.com/kvark/mint) | 0.5.9 | MIT | [notices](third_party/licenses/mint-0.5.9/) |
| [mio](https://github.com/tokio-rs/mio) | 1.2.3 | MIT | [notices](third_party/licenses/mio-1.2.3/) |
| [moxcms](https://github.com/awxkee/moxcms.git) | 0.8.1 | BSD-3-Clause OR Apache-2.0 | [notices](third_party/licenses/moxcms-0.8.1/) |
| [multiversion_no_op](https://github.com/hsivonen/multiversion_no_op) | 1.0.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/multiversion_no_op-1.0.0/) |
| [naga](https://github.com/gfx-rs/wgpu/tree/trunk/naga) | 25.0.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/naga-25.0.1/) |
| [nanorand](https://github.com/Absolucy/nanorand-rs) | 0.7.0 | Zlib | [notices](third_party/licenses/nanorand-0.7.0/) |
| [new_debug_unreachable](https://github.com/mbrubeck/rust-debug-unreachable) | 1.0.6 | MIT | [notices](third_party/licenses/new_debug_unreachable-1.0.6/) |
| [nix](https://github.com/nix-rust/nix) | 0.29.0 | MIT | [notices](third_party/licenses/nix-0.29.0/) |
| [nix](https://github.com/nix-rust/nix) | 0.31.3 | MIT | [notices](third_party/licenses/nix-0.31.3/) |
| [no_std_io2](https://github.com/wcampbell0x2a/no-std-io2) | 0.9.4 | Apache-2.0 OR MIT | [notices](third_party/licenses/no_std_io2-0.9.4/) |
| [nom](https://github.com/rust-bakery/nom) | 8.0.0 | MIT | [notices](third_party/licenses/nom-8.0.0/) |
| [noop_proc_macro](https://github.com/lu-zero/noop_proc_macro) | 0.3.0 | MIT | [notices](third_party/licenses/noop_proc_macro-0.3.0/) |
| [ntapi](https://github.com/MSxDOS/ntapi) | 0.4.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/ntapi-0.4.3/) |
| [num](https://github.com/rust-num/num) | 0.4.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/num-0.4.3/) |
| [num-bigint](https://github.com/rust-num/num-bigint) | 0.4.8 | MIT OR Apache-2.0 | [notices](third_party/licenses/num-bigint-0.4.8/) |
| [num-bigint-dig](https://github.com/dignifiedquire/num-bigint) | 0.8.6 | MIT/Apache-2.0 | [notices](third_party/licenses/num-bigint-dig-0.8.6/) |
| [num-complex](https://github.com/rust-num/num-complex) | 0.4.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/num-complex-0.4.6/) |
| [num-derive](https://github.com/rust-num/num-derive) | 0.4.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/num-derive-0.4.2/) |
| [num-integer](https://github.com/rust-num/num-integer) | 0.1.47 | MIT OR Apache-2.0 | [notices](third_party/licenses/num-integer-0.1.47/) |
| [num-iter](https://github.com/rust-num/num-iter) | 0.1.46 | MIT OR Apache-2.0 | [notices](third_party/licenses/num-iter-0.1.46/) |
| [num-rational](https://github.com/rust-num/num-rational) | 0.4.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/num-rational-0.4.2/) |
| [num-traits](https://github.com/rust-num/num-traits) | 0.2.19 | MIT OR Apache-2.0 | [notices](third_party/licenses/num-traits-0.2.19/) |
| [num_cpus](https://github.com/seanmonstar/num_cpus) | 1.17.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/num_cpus-1.17.0/) |
| [object](https://github.com/gimli-rs/object) | 0.39.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/object-0.39.1/) |
| [once_cell](https://github.com/matklad/once_cell) | 1.21.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/once_cell-1.21.4/) |
| [oo7](https://github.com/bilelmoussaoui/oo7) | 0.5.0 | MIT | [notices](third_party/licenses/oo7-0.5.0/) |
| [open](https://github.com/Byron/open-rs) | 5.4.4 | MIT | [notices](third_party/licenses/open-5.4.4/) |
| [openssl-probe](https://github.com/rustls/openssl-probe) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/openssl-probe-0.2.1/) |
| [option-ext](https://github.com/soc/option-ext.git) | 0.2.0 | MPL-2.0 | [notices](third_party/licenses/option-ext-0.2.0/) |
| [ordered-stream](https://github.com/danieldg/ordered-stream) | 0.2.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/ordered-stream-0.2.0/) |
| [parking](https://github.com/smol-rs/parking) | 2.2.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/parking-2.2.1/) |
| [parking_lot](https://github.com/Amanieu/parking_lot) | 0.12.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/parking_lot-0.12.5/) |
| [parking_lot_core](https://github.com/Amanieu/parking_lot) | 0.9.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/parking_lot_core-0.9.12/) |
| [paste](https://github.com/dtolnay/paste) | 1.0.15 | MIT OR Apache-2.0 | [notices](third_party/licenses/paste-1.0.15/) |
| [pastey](https://github.com/as1100k/pastey) | 0.1.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/pastey-0.1.1/) |
| [pathfinder_geometry](https://github.com/servo/pathfinder) | 0.5.1 | MIT/Apache-2.0 | [notices](third_party/licenses/pathfinder_geometry-0.5.1/) |
| [pathfinder_simd](https://github.com/servo/pathfinder) | 0.5.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/pathfinder_simd-0.5.6/) |
| [pbkdf2](https://github.com/RustCrypto/password-hashes/tree/master/pbkdf2) | 0.12.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/pbkdf2-0.12.2/) |
| [pdf-writer](https://github.com/typst/pdf-writer) | 0.15.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/pdf-writer-0.15.0/) |
| [percent-encoding](https://github.com/servo/rust-url/) | 2.3.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/percent-encoding-2.3.2/) |
| [phf](https://github.com/rust-phf/rust-phf) | 0.14.0 | MIT | [notices](third_party/licenses/phf-0.14.0/) |
| [phf_generator](https://github.com/rust-phf/rust-phf) | 0.14.0 | MIT | [notices](third_party/licenses/phf_generator-0.14.0/) |
| [phf_macros](https://github.com/rust-phf/rust-phf) | 0.14.0 | MIT | [notices](third_party/licenses/phf_macros-0.14.0/) |
| [phf_shared](https://github.com/rust-phf/rust-phf) | 0.14.0 | MIT | [notices](third_party/licenses/phf_shared-0.14.0/) |
| [pico-args](https://github.com/RazrFalcon/pico-args) | 0.5.0 | MIT | [notices](third_party/licenses/pico-args-0.5.0/) |
| [pin-project](https://github.com/taiki-e/pin-project) | 1.1.13 | Apache-2.0 OR MIT | [notices](third_party/licenses/pin-project-1.1.13/) |
| [pin-project-internal](https://github.com/taiki-e/pin-project) | 1.1.13 | Apache-2.0 OR MIT | [notices](third_party/licenses/pin-project-internal-1.1.13/) |
| [pin-project-lite](https://github.com/taiki-e/pin-project-lite) | 0.2.17 | Apache-2.0 OR MIT | [notices](third_party/licenses/pin-project-lite-0.2.17/) |
| [pin-utils](https://github.com/rust-lang/pin-utils) | 0.1.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/pin-utils-0.1.1/) |
| [piper](https://github.com/smol-rs/piper) | 0.2.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/piper-0.2.5/) |
| [pkg-config](https://github.com/rust-lang/pkg-config-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/pkg-config-0.3.34/) |
| [png](https://github.com/image-rs/image-png) | 0.17.16 | MIT OR Apache-2.0 | [notices](third_party/licenses/png-0.17.16/) |
| [png](https://github.com/image-rs/image-png) | 0.18.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/png-0.18.1/) |
| [polling](https://github.com/smol-rs/polling) | 3.11.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/polling-3.11.0/) |
| [pollster](https://github.com/zesterer/pollster) | 0.2.5 | Apache-2.0/MIT | [notices](third_party/licenses/pollster-0.2.5/) |
| [polycool](https://github.com/linebender/kurbo) | 0.4.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/polycool-0.4.0/) |
| [postage](https://github.com/austinjones/postage-rs) | 0.5.0 | MIT | [notices](third_party/licenses/postage-0.5.0/) |
| [potential_utf](https://github.com/unicode-org/icu4x) | 0.1.6 | Unicode-3.0 | [notices](third_party/licenses/potential_utf-0.1.6/) |
| [ppv-lite86](https://github.com/cryptocorrosion/cryptocorrosion) | 0.2.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/ppv-lite86-0.2.21/) |
| [proc-macro-crate](https://github.com/bkchr/proc-macro-crate) | 3.5.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/proc-macro-crate-3.5.0/) |
| [proc-macro-error-attr2](https://github.com/GnomedDev/proc-macro-error-2) | 2.0.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/proc-macro-error-attr2-2.0.0/) |
| [proc-macro-error2](https://github.com/GnomedDev/proc-macro-error-2) | 2.0.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/proc-macro-error2-2.0.1/) |
| [proc-macro2](https://github.com/dtolnay/proc-macro2) | 1.0.107 | MIT OR Apache-2.0 | [notices](third_party/licenses/proc-macro2-1.0.107/) |
| [profiling](https://github.com/aclysma/profiling) | 1.0.18 | MIT OR Apache-2.0 | [notices](third_party/licenses/profiling-1.0.18/) |
| [profiling-procmacros](https://github.com/aclysma/profiling) | 1.0.18 | MIT OR Apache-2.0 | [notices](third_party/licenses/profiling-procmacros-1.0.18/) |
| [psm](https://github.com/rust-lang/stacker/) | 0.1.32 | MIT OR Apache-2.0 | [notices](third_party/licenses/psm-0.1.32/) |
| [pulp](https://github.com/sarah-quinones/pulp/) | 0.22.3 | MIT | [notices](third_party/licenses/pulp-0.22.3/) |
| [pulp-wasm-simd-flag](https://github.com/sarah-quinones/pulp/) | 0.1.1 | MIT | [notices](third_party/licenses/pulp-wasm-simd-flag-0.1.1/) |
| [pxfm](https://github.com/awxkee/pxfm) | 0.1.30 | BSD-3-Clause OR Apache-2.0 | [notices](third_party/licenses/pxfm-0.1.30/) |
| [qoi](https://github.com/aldanor/qoi-rust) | 0.4.1 | MIT/Apache-2.0 | [notices](third_party/licenses/qoi-0.4.1/) |
| [quick-error](http://github.com/tailhook/quick-error) | 2.0.1 | MIT/Apache-2.0 | [notices](third_party/licenses/quick-error-2.0.1/) |
| [quick-xml](https://github.com/tafia/quick-xml) | 0.41.0 | MIT | [notices](third_party/licenses/quick-xml-0.41.0/) |
| [quinn](https://github.com/quinn-rs/quinn) | 0.11.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/quinn-0.11.12/) |
| [quinn-proto](https://github.com/quinn-rs/quinn) | 0.11.19 | MIT OR Apache-2.0 | [notices](third_party/licenses/quinn-proto-0.11.19/) |
| [quinn-udp](https://github.com/quinn-rs/quinn) | 0.5.16 | MIT OR Apache-2.0 | [notices](third_party/licenses/quinn-udp-0.5.16/) |
| [quote](https://github.com/dtolnay/quote) | 1.0.47 | MIT OR Apache-2.0 | [notices](third_party/licenses/quote-1.0.47/) |
| [rand](https://github.com/rust-random/rand) | 0.10.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand-0.10.3/) |
| [rand](https://github.com/rust-random/rand) | 0.8.8 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand-0.8.8/) |
| [rand](https://github.com/rust-random/rand) | 0.9.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand-0.9.5/) |
| [rand_chacha](https://github.com/rust-random/rand) | 0.3.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand_chacha-0.3.1/) |
| [rand_chacha](https://github.com/rust-random/rand) | 0.9.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand_chacha-0.9.0/) |
| [rand_core](https://github.com/rust-random/rand_core) | 0.10.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand_core-0.10.1/) |
| [rand_core](https://github.com/rust-random/rand) | 0.6.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand_core-0.6.4/) |
| [rand_core](https://github.com/rust-random/rand) | 0.9.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand_core-0.9.5/) |
| [rand_pcg](https://github.com/rust-random/rngs) | 0.10.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand_pcg-0.10.2/) |
| [rangemap](https://github.com/jeffparsons/rangemap) | 1.8.0 | MIT/Apache-2.0 | [notices](third_party/licenses/rangemap-1.8.0/) |
| [rav1e](https://github.com/xiph/rav1e/) | 0.8.1 | BSD-2-Clause | [notices](third_party/licenses/rav1e-0.8.1/) |
| [ravif](https://github.com/kornelski/cavif-rs) | 0.13.0 | BSD-3-Clause | [notices](third_party/licenses/ravif-0.13.0/) |
| [raw-cpuid](https://github.com/gz/rust-cpuid) | 11.6.0 | MIT | [notices](third_party/licenses/raw-cpuid-11.6.0/) |
| [raw-window-handle](https://github.com/rust-windowing/raw-window-handle) | 0.6.2 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/raw-window-handle-0.6.2/) |
| [rayon](https://github.com/rayon-rs/rayon) | 1.12.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/rayon-1.12.0/) |
| [rayon-core](https://github.com/rayon-rs/rayon) | 1.13.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/rayon-core-1.13.0/) |
| [read-fonts](https://github.com/googlefonts/fontations) | 0.39.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/read-fonts-0.39.2/) |
| [read-fonts](https://github.com/googlefonts/fontations) | 0.41.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/read-fonts-0.41.0/) |
| [reborrow](https://github.com/sarah-ek/reborrow/) | 0.5.5 | MIT | [notices](third_party/licenses/reborrow-0.5.5/) |
| [ref-cast](https://github.com/dtolnay/ref-cast) | 1.0.27 | MIT OR Apache-2.0 | [notices](third_party/licenses/ref-cast-1.0.27/) |
| [ref-cast-impl](https://github.com/dtolnay/ref-cast) | 1.0.27 | MIT OR Apache-2.0 | [notices](third_party/licenses/ref-cast-impl-1.0.27/) |
| [regex](https://github.com/rust-lang/regex) | 1.13.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/regex-1.13.1/) |
| [regex-automata](https://github.com/rust-lang/regex) | 0.4.18 | MIT OR Apache-2.0 | [notices](third_party/licenses/regex-automata-0.4.18/) |
| [regex-syntax](https://github.com/rust-lang/regex) | 0.8.11 | MIT OR Apache-2.0 | [notices](third_party/licenses/regex-syntax-0.8.11/) |
| [reqwest](https://github.com/seanmonstar/reqwest) | 0.12.28 | MIT OR Apache-2.0 | [notices](third_party/licenses/reqwest-0.12.28/) |
| [resvg](https://github.com/linebender/resvg) | 0.45.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/resvg-0.45.1/) |
| [resvg](https://github.com/linebender/resvg) | 0.47.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/resvg-0.47.0/) |
| [rgb](https://github.com/kornelski/rust-rgb) | 0.8.53 | MIT | [notices](third_party/licenses/rgb-0.8.53/) |
| [ring](https://github.com/briansmith/ring) | 0.17.14 | Apache-2.0 AND ISC | [notices](third_party/licenses/ring-0.17.14/) |
| [roxmltree](https://github.com/RazrFalcon/roxmltree) | 0.20.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/roxmltree-0.20.0/) |
| [roxmltree](https://github.com/RazrFalcon/roxmltree) | 0.21.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/roxmltree-0.21.1/) |
| [rusqlite](https://github.com/rusqlite/rusqlite) | 0.40.2 | MIT | [notices](third_party/licenses/rusqlite-0.40.2/) |
| [rust-embed](https://pyrossh.dev/repos/rust-embed) | 8.12.0 | MIT | [notices](third_party/licenses/rust-embed-8.12.0/) |
| [rust-embed-impl](https://pyrossh.dev/repos/rust-embed) | 8.12.0 | MIT | [notices](third_party/licenses/rust-embed-impl-8.12.0/) |
| [rust-embed-utils](https://pyrossh.dev/repos/rust-embed) | 8.12.0 | MIT | [notices](third_party/licenses/rust-embed-utils-8.12.0/) |
| [rustc-hash](https://github.com/rust-lang-nursery/rustc-hash) | 1.1.0 | Apache-2.0/MIT | [notices](third_party/licenses/rustc-hash-1.1.0/) |
| [rustc-hash](https://github.com/rust-lang/rustc-hash) | 2.1.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/rustc-hash-2.1.3/) |
| [rustc_version](https://github.com/djc/rustc-version-rs) | 0.4.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/rustc_version-0.4.1/) |
| [rustix](https://github.com/bytecodealliance/rustix) | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [notices](third_party/licenses/rustix-0.38.44/) |
| [rustix](https://github.com/bytecodealliance/rustix) | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [notices](third_party/licenses/rustix-1.1.5/) |
| [rustls](https://github.com/rustls/rustls) | 0.23.45 | Apache-2.0 OR ISC OR MIT | [notices](third_party/licenses/rustls-0.23.45/) |
| [rustls-native-certs](https://github.com/rustls/rustls-native-certs) | 0.8.4 | Apache-2.0 OR ISC OR MIT | [notices](third_party/licenses/rustls-native-certs-0.8.4/) |
| [rustls-pemfile](https://github.com/rustls/pemfile) | 2.2.0 | Apache-2.0 OR ISC OR MIT | [notices](third_party/licenses/rustls-pemfile-2.2.0/) |
| [rustls-pki-types](https://github.com/rustls/pki-types) | 1.15.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/rustls-pki-types-1.15.1/) |
| [rustls-webpki](https://github.com/rustls/webpki) | 0.103.15 | ISC | [notices](third_party/licenses/rustls-webpki-0.103.15/) |
| [rustversion](https://github.com/dtolnay/rustversion) | 1.0.23 | MIT OR Apache-2.0 | [notices](third_party/licenses/rustversion-1.0.23/) |
| [rustybuzz](https://github.com/RazrFalcon/rustybuzz) | 0.14.1 | MIT | [notices](third_party/licenses/rustybuzz-0.14.1/) |
| [rustybuzz](https://github.com/harfbuzz/rustybuzz) | 0.20.1 | MIT | [notices](third_party/licenses/rustybuzz-0.20.1/) |
| [ryu](https://github.com/dtolnay/ryu) | 1.0.23 | Apache-2.0 OR BSL-1.0 | [notices](third_party/licenses/ryu-1.0.23/) |
| [same-file](https://github.com/BurntSushi/same-file) | 1.0.6 | Unlicense/MIT | [notices](third_party/licenses/same-file-1.0.6/) |
| [schannel](https://github.com/steffengy/schannel-rs) | 0.1.29 | MIT | [notices](third_party/licenses/schannel-0.1.29/) |
| [schemars](https://github.com/GREsau/schemars) | 1.2.2 | MIT | [notices](third_party/licenses/schemars-1.2.2/) |
| [schemars_derive](https://github.com/GREsau/schemars) | 1.2.2 | MIT | [notices](third_party/licenses/schemars_derive-1.2.2/) |
| [scoped-tls](https://github.com/alexcrichton/scoped-tls) | 1.0.1 | MIT/Apache-2.0 | [notices](third_party/licenses/scoped-tls-1.0.1/) |
| [scopeguard](https://github.com/bluss/scopeguard) | 1.2.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/scopeguard-1.2.0/) |
| [seahash](https://gitlab.redox-os.org/redox-os/seahash) | 4.1.0 | MIT | [notices](third_party/licenses/seahash-4.1.0/) |
| [self_cell](https://github.com/Voultapher/self_cell) | 1.3.0 | Apache-2.0 OR GPL-2.0-only | [notices](third_party/licenses/self_cell-1.3.0/) |
| [semver](https://github.com/dtolnay/semver) | 1.0.28 | MIT OR Apache-2.0 | [notices](third_party/licenses/semver-1.0.28/) |
| [serde](https://github.com/serde-rs/serde) | 1.0.229 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde-1.0.229/) |
| [serde_core](https://github.com/serde-rs/serde) | 1.0.229 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_core-1.0.229/) |
| [serde_derive](https://github.com/serde-rs/serde) | 1.0.229 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_derive-1.0.229/) |
| [serde_derive_internals](https://github.com/serde-rs/serde) | 0.30.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_derive_internals-0.30.0/) |
| [serde_fmt](https://github.com/KodrAus/serde_fmt.git) | 1.1.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/serde_fmt-1.1.0/) |
| [serde_json](https://github.com/serde-rs/json) | 1.0.151 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_json-1.0.151/) |
| [serde_json_lenient](https://github.com/google/serde_json_lenient) | 0.2.4 | MIT/Apache-2.0 | [notices](third_party/licenses/serde_json_lenient-0.2.4/) |
| [serde_repr](https://github.com/dtolnay/serde-repr) | 0.1.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_repr-0.1.21/) |
| [serde_spanned](https://github.com/toml-rs/toml) | 1.1.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_spanned-1.1.1/) |
| [serde_urlencoded](https://github.com/nox/serde_urlencoded) | 0.7.1 | MIT/Apache-2.0 | [notices](third_party/licenses/serde_urlencoded-0.7.1/) |
| [sha1_smol](https://github.com/mitsuhiko/sha1-smol) | 1.0.1 | BSD-3-Clause | [notices](third_party/licenses/sha1_smol-1.0.1/) |
| [sha2](https://github.com/RustCrypto/hashes) | 0.10.9 | MIT OR Apache-2.0 | [notices](third_party/licenses/sha2-0.10.9/) |
| [sha2](https://github.com/RustCrypto/hashes) | 0.11.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/sha2-0.11.0/) |
| [shlex](https://github.com/comex/rust-shlex) | 1.3.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/shlex-1.3.0/) |
| [shlex](https://github.com/comex/rust-shlex) | 2.0.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/shlex-2.0.1/) |
| [signal-hook-registry](https://github.com/vorner/signal-hook) | 1.4.8 | MIT OR Apache-2.0 | [notices](third_party/licenses/signal-hook-registry-1.4.8/) |
| [simd-adler32](https://github.com/mcountryman/simd-adler32) | 0.3.10 | MIT | [notices](third_party/licenses/simd-adler32-0.3.10/) |
| [simd_helpers](https://github.com/lu-zero/simd_helpers) | 0.1.0 | MIT | [notices](third_party/licenses/simd_helpers-0.1.0/) |
| [simdutf8](https://github.com/rusticstuff/simdutf8) | 0.1.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/simdutf8-0.1.5/) |
| [simplecss](https://github.com/linebender/simplecss) | 0.2.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/simplecss-0.2.2/) |
| [siphasher](https://github.com/jedisct1/rust-siphash) | 1.0.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/siphasher-1.0.4/) |
| [skrifa](https://github.com/googlefonts/fontations) | 0.42.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/skrifa-0.42.1/) |
| [skrifa](https://github.com/googlefonts/fontations) | 0.44.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/skrifa-0.44.0/) |
| [slab](https://github.com/tokio-rs/slab) | 0.4.12 | MIT | [notices](third_party/licenses/slab-0.4.12/) |
| [slotmap](https://github.com/orlp/slotmap) | 1.1.1 | Zlib | [notices](third_party/licenses/slotmap-1.1.1/) |
| [smallvec](https://github.com/servo/rust-smallvec) | 1.16.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/smallvec-1.16.2/) |
| [smol](https://github.com/smol-rs/smol) | 2.0.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/smol-2.0.2/) |
| [smol_str](https://github.com/rust-analyzer/smol_str) | 0.2.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/smol_str-0.2.2/) |
| [socket2](https://github.com/rust-lang/socket2) | 0.6.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/socket2-0.6.5/) |
| [spin](https://github.com/mvdnes/spin-rs.git) | 0.9.9 | MIT | [notices](third_party/licenses/spin-0.9.9/) |
| [spirv](https://github.com/gfx-rs/rspirv) | 0.3.0+sdk-1.3.268.0 | Apache-2.0 | [notices](third_party/licenses/spirv-0.3.0+sdk-1.3.268.0/) |
| [stable_deref_trait](https://github.com/storyyeller/stable_deref_trait) | 1.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/stable_deref_trait-1.2.1/) |
| [stacker](https://github.com/rust-lang/stacker) | 0.1.25 | MIT OR Apache-2.0 | [notices](third_party/licenses/stacker-0.1.25/) |
| [stacksafe](https://github.com/fast/stacksafe) | 0.1.4 | Apache-2.0 | [notices](third_party/licenses/stacksafe-0.1.4/) |
| [stacksafe-macro](https://github.com/fast/stacksafe) | 0.1.4 | Apache-2.0 | [notices](third_party/licenses/stacksafe-macro-0.1.4/) |
| [static_assertions](https://github.com/nvzqz/static-assertions-rs) | 1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/static_assertions-1.1.0/) |
| [strict-num](https://github.com/RazrFalcon/strict-num) | 0.1.1 | MIT | [notices](third_party/licenses/strict-num-0.1.1/) |
| [stringprep](https://github.com/sfackler/rust-stringprep) | 0.1.5 | MIT/Apache-2.0 | [notices](third_party/licenses/stringprep-0.1.5/) |
| [strum](https://github.com/Peternator7/strum) | 0.26.3 | MIT | [notices](third_party/licenses/strum-0.26.3/) |
| [strum](https://github.com/Peternator7/strum) | 0.27.2 | MIT | [notices](third_party/licenses/strum-0.27.2/) |
| [strum_macros](https://github.com/Peternator7/strum) | 0.26.4 | MIT | [notices](third_party/licenses/strum_macros-0.26.4/) |
| [strum_macros](https://github.com/Peternator7/strum) | 0.27.2 | MIT | [notices](third_party/licenses/strum_macros-0.27.2/) |
| [subsetter](https://github.com/typst/subsetter) | 0.2.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/subsetter-0.2.6/) |
| [subtle](https://github.com/dalek-cryptography/subtle) | 2.6.1 | BSD-3-Clause | [notices](third_party/licenses/subtle-2.6.1/) |
| [sval](https://github.com/sval-rs/sval) | 2.22.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/sval-2.22.0/) |
| [sval_buffer](https://github.com/sval-rs/sval) | 2.22.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/sval_buffer-2.22.0/) |
| [sval_dynamic](https://github.com/sval-rs/sval) | 2.22.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/sval_dynamic-2.22.0/) |
| [sval_fmt](https://github.com/sval-rs/sval) | 2.22.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/sval_fmt-2.22.0/) |
| [sval_json](https://github.com/sval-rs/sval) | 2.22.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/sval_json-2.22.0/) |
| [sval_nested](https://github.com/sval-rs/sval) | 2.22.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/sval_nested-2.22.0/) |
| [sval_ref](https://github.com/sval-rs/sval) | 2.22.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/sval_ref-2.22.0/) |
| [sval_serde](https://github.com/sval-rs/sval) | 2.22.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/sval_serde-2.22.0/) |
| [svg_fmt](https://github.com/nical/rust_debug) | 0.4.5 | MIT/Apache-2.0 | [notices](third_party/licenses/svg_fmt-0.4.5/) |
| [svgtypes](https://github.com/linebender/svgtypes) | 0.15.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/svgtypes-0.15.3/) |
| [svgtypes](https://github.com/linebender/svgtypes) | 0.16.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/svgtypes-0.16.1/) |
| [swash](https://github.com/dfrg/swash) | 0.2.10 | Apache-2.0 OR MIT | [notices](third_party/licenses/swash-0.2.10/) |
| [syn](https://github.com/dtolnay/syn) | 1.0.109 | MIT OR Apache-2.0 | [notices](third_party/licenses/syn-1.0.109/) |
| [syn](https://github.com/dtolnay/syn) | 2.0.119 | MIT OR Apache-2.0 | [notices](third_party/licenses/syn-2.0.119/) |
| [syn](https://github.com/dtolnay/syn) | 3.0.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/syn-3.0.6/) |
| [sync_wrapper](https://github.com/Actyx/sync_wrapper) | 1.0.2 | Apache-2.0 | [notices](third_party/licenses/sync_wrapper-1.0.2/) |
| [synstructure](https://github.com/mystor/synstructure) | 0.14.0 | MIT | [notices](third_party/licenses/synstructure-0.14.0/) |
| [sys-locale](https://github.com/1Password/sys-locale) | 0.3.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/sys-locale-0.3.2/) |
| [sysinfo](https://github.com/GuillaumeGomez/sysinfo) | 0.31.4 | MIT | [notices](third_party/licenses/sysinfo-0.31.4/) |
| [taffy](https://github.com/DioxusLabs/taffy) | 0.9.0 | MIT | [notices](third_party/licenses/taffy-0.9.0/) |
| [take-until](https://github.com/hdevalke/take-until.git) | 0.2.0 | MIT | [notices](third_party/licenses/take-until-0.2.0/) |
| [tar](https://github.com/composefs/tar-rs) | 0.4.46 | MIT OR Apache-2.0 | [notices](third_party/licenses/tar-0.4.46/) |
| [tempfile](https://github.com/Stebalien/tempfile) | 3.27.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/tempfile-3.27.0/) |
| [tendril](https://github.com/servo/tendril) | 0.4.3 | MIT/Apache-2.0 | [notices](third_party/licenses/tendril-0.4.3/) |
| [termcolor](https://github.com/BurntSushi/termcolor) | 1.4.1 | Unlicense OR MIT | [notices](third_party/licenses/termcolor-1.4.1/) |
| [thiserror](https://github.com/dtolnay/thiserror) | 1.0.69 | MIT OR Apache-2.0 | [notices](third_party/licenses/thiserror-1.0.69/) |
| [thiserror](https://github.com/dtolnay/thiserror) | 2.0.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/thiserror-2.0.21/) |
| [thiserror-impl](https://github.com/dtolnay/thiserror) | 1.0.69 | MIT OR Apache-2.0 | [notices](third_party/licenses/thiserror-impl-1.0.69/) |
| [thiserror-impl](https://github.com/dtolnay/thiserror) | 2.0.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/thiserror-impl-2.0.21/) |
| [tiff](https://github.com/image-rs/image-tiff) | 0.11.3 | MIT | [notices](third_party/licenses/tiff-0.11.3/) |
| [tiny-keccak](https://crates.io/crates/tiny-keccak/2.0.2) | 2.0.2 | CC0-1.0 | [notices](third_party/licenses/tiny-keccak-2.0.2/) |
| [tiny-skia](https://github.com/RazrFalcon/tiny-skia) | 0.11.4 | BSD-3-Clause | [notices](third_party/licenses/tiny-skia-0.11.4/) |
| [tiny-skia](https://github.com/linebender/tiny-skia) | 0.12.0 | BSD-3-Clause | [notices](third_party/licenses/tiny-skia-0.12.0/) |
| [tiny-skia-path](https://github.com/RazrFalcon/tiny-skia/tree/master/path) | 0.11.4 | BSD-3-Clause | [notices](third_party/licenses/tiny-skia-path-0.11.4/) |
| [tiny-skia-path](https://github.com/linebender/tiny-skia/tree/master/path) | 0.12.0 | BSD-3-Clause | [notices](third_party/licenses/tiny-skia-path-0.12.0/) |
| [tinystr](https://github.com/unicode-org/icu4x) | 0.8.4 | Unicode-3.0 | [notices](third_party/licenses/tinystr-0.8.4/) |
| [tinyvec](https://github.com/Lokathor/tinyvec) | 1.13.3 | Zlib OR Apache-2.0 OR MIT | [notices](third_party/licenses/tinyvec-1.13.3/) |
| [tokio](https://github.com/tokio-rs/tokio) | 1.53.1 | MIT | [notices](third_party/licenses/tokio-1.53.1/) |
| [tokio-rustls](https://github.com/rustls/tokio-rustls) | 0.26.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/tokio-rustls-0.26.6/) |
| [tokio-socks](https://github.com/sticnarf/tokio-socks) | 0.5.3 | MIT | [notices](third_party/licenses/tokio-socks-0.5.3/) |
| [tokio-util](https://github.com/tokio-rs/tokio) | 0.7.19 | MIT | [notices](third_party/licenses/tokio-util-0.7.19/) |
| [toml](https://github.com/toml-rs/toml) | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/toml-1.1.6+spec-1.1.0/) |
| [toml_datetime](https://github.com/toml-rs/toml) | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/toml_datetime-1.1.1+spec-1.1.0/) |
| [toml_edit](https://github.com/toml-rs/toml) | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/toml_edit-0.25.15+spec-1.1.0/) |
| [toml_parser](https://github.com/toml-rs/toml) | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/toml_parser-1.1.3+spec-1.1.0/) |
| [toml_writer](https://github.com/toml-rs/toml) | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/toml_writer-1.1.2+spec-1.1.0/) |
| [tower](https://github.com/tower-rs/tower) | 0.5.3 | MIT | [notices](third_party/licenses/tower-0.5.3/) |
| [tower-http](https://github.com/tower-rs/tower-http) | 0.6.11 | MIT | [notices](third_party/licenses/tower-http-0.6.11/) |
| [tower-layer](https://github.com/tower-rs/tower) | 0.3.3 | MIT | [notices](third_party/licenses/tower-layer-0.3.3/) |
| [tower-service](https://github.com/tower-rs/tower) | 0.3.3 | MIT | [notices](third_party/licenses/tower-service-0.3.3/) |
| [tracing](https://github.com/tokio-rs/tracing) | 0.1.44 | MIT | [notices](third_party/licenses/tracing-0.1.44/) |
| [tracing-attributes](https://github.com/tokio-rs/tracing) | 0.1.31 | MIT | [notices](third_party/licenses/tracing-attributes-0.1.31/) |
| [tracing-core](https://github.com/tokio-rs/tracing) | 0.1.36 | MIT | [notices](third_party/licenses/tracing-core-0.1.36/) |
| [try-lock](https://github.com/seanmonstar/try-lock) | 0.2.5 | MIT | [notices](third_party/licenses/try-lock-0.2.5/) |
| [ttf-parser](https://github.com/RazrFalcon/ttf-parser) | 0.20.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/ttf-parser-0.20.0/) |
| [ttf-parser](https://github.com/RazrFalcon/ttf-parser) | 0.21.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/ttf-parser-0.21.1/) |
| [ttf-parser](https://github.com/harfbuzz/ttf-parser) | 0.25.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/ttf-parser-0.25.1/) |
| [typed-path](https://github.com/chipsenkbeil/typed-path) | 0.12.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/typed-path-0.12.3/) |
| [typeid](https://github.com/dtolnay/typeid) | 1.0.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/typeid-1.0.3/) |
| [typenum](https://github.com/paholg/typenum) | 1.20.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/typenum-1.20.1/) |
| [unicase](https://github.com/seanmonstar/unicase) | 2.9.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/unicase-2.9.0/) |
| [unicode-bidi](https://github.com/servo/unicode-bidi) | 0.3.18 | MIT OR Apache-2.0 | [notices](third_party/licenses/unicode-bidi-0.3.18/) |
| [unicode-bidi-mirroring](https://github.com/RazrFalcon/unicode-bidi-mirroring) | 0.2.0 | MIT/Apache-2.0 | [notices](third_party/licenses/unicode-bidi-mirroring-0.2.0/) |
| [unicode-bidi-mirroring](https://github.com/RazrFalcon/unicode-bidi-mirroring) | 0.4.0 | MIT/Apache-2.0 | [notices](third_party/licenses/unicode-bidi-mirroring-0.4.0/) |
| [unicode-ccc](https://github.com/RazrFalcon/unicode-ccc) | 0.2.0 | MIT/Apache-2.0 | [notices](third_party/licenses/unicode-ccc-0.2.0/) |
| [unicode-ccc](https://github.com/RazrFalcon/unicode-ccc) | 0.4.0 | MIT/Apache-2.0 | [notices](third_party/licenses/unicode-ccc-0.4.0/) |
| [unicode-ident](https://github.com/dtolnay/unicode-ident) | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | [notices](third_party/licenses/unicode-ident-1.0.26/) |
| [unicode-linebreak](https://github.com/axelf4/unicode-linebreak) | 0.1.5 | Apache-2.0 | [notices](third_party/licenses/unicode-linebreak-0.1.5/) |
| [unicode-normalization](https://github.com/unicode-rs/unicode-normalization) | 0.1.25 | MIT OR Apache-2.0 | [notices](third_party/licenses/unicode-normalization-0.1.25/) |
| [unicode-properties](https://github.com/unicode-rs/unicode-properties) | 0.1.4 | MIT/Apache-2.0 | [notices](third_party/licenses/unicode-properties-0.1.4/) |
| [unicode-script](https://github.com/unicode-rs/unicode-script) | 0.5.8 | MIT OR Apache-2.0 | [notices](third_party/licenses/unicode-script-0.5.8/) |
| [unicode-segmentation](https://github.com/unicode-rs/unicode-segmentation) | 1.13.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/unicode-segmentation-1.13.3/) |
| [unicode-vo](https://github.com/RazrFalcon/unicode-vo) | 0.1.0 | MIT/Apache-2.0 | [notices](third_party/licenses/unicode-vo-0.1.0/) |
| [unicode-width](https://github.com/unicode-rs/unicode-width) | 0.2.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/unicode-width-0.2.2/) |
| [untrusted](https://github.com/briansmith/untrusted) | 0.9.0 | ISC | [notices](third_party/licenses/untrusted-0.9.0/) |
| [url](https://github.com/servo/rust-url) | 2.5.8 | MIT OR Apache-2.0 | [notices](third_party/licenses/url-2.5.8/) |
| [usvg](https://github.com/linebender/resvg) | 0.45.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/usvg-0.45.1/) |
| [usvg](https://github.com/linebender/resvg) | 0.47.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/usvg-0.47.0/) |
| [utf-8](https://github.com/SimonSapin/rust-utf8) | 0.7.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/utf-8-0.7.6/) |
| [utf8_iter](https://github.com/hsivonen/utf8_iter) | 1.0.4 | Apache-2.0 OR MIT | [notices](third_party/licenses/utf8_iter-1.0.4/) |
| [uuid](https://github.com/uuid-rs/uuid) | 1.26.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/uuid-1.26.1/) |
| [v_frame](https://github.com/rust-av/v_frame) | 0.3.9 | BSD-2-Clause | [notices](third_party/licenses/v_frame-0.3.9/) |
| [value-bag](https://github.com/sval-rs/value-bag) | 1.14.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/value-bag-1.14.1/) |
| [value-bag-serde1](https://crates.io/crates/value-bag-serde1/1.14.1) | 1.14.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/value-bag-serde1-1.14.1/) |
| [value-bag-sval2](https://crates.io/crates/value-bag-sval2/1.14.1) | 1.14.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/value-bag-sval2-1.14.1/) |
| [vcpkg](https://github.com/mcgoo/vcpkg-rs) | 0.2.15 | MIT/Apache-2.0 | [notices](third_party/licenses/vcpkg-0.2.15/) |
| [version_check](https://github.com/SergioBenitez/version_check) | 0.9.5 | MIT/Apache-2.0 | [notices](third_party/licenses/version_check-0.9.5/) |
| [vswhom](https://github.com/nabijaczleweli/vswhom.rs) | 0.1.0 | MIT | [notices](third_party/licenses/vswhom-0.1.0/) |
| [vswhom-sys](https://github.com/nabijaczleweli/vswhom-sys.rs) | 0.1.3 | MIT | [notices](third_party/licenses/vswhom-sys-0.1.3/) |
| [waker-fn](https://github.com/smol-rs/waker-fn) | 1.2.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/waker-fn-1.2.0/) |
| [walkdir](https://github.com/BurntSushi/walkdir) | 2.5.0 | Unlicense/MIT | [notices](third_party/licenses/walkdir-2.5.0/) |
| [want](https://github.com/seanmonstar/want) | 0.3.1 | MIT | [notices](third_party/licenses/want-0.3.1/) |
| [wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen) | 0.2.129 | MIT OR Apache-2.0 | [notices](third_party/licenses/wasm-bindgen-0.2.129/) |
| [wasm-bindgen-macro](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro) | 0.2.129 | MIT OR Apache-2.0 | [notices](third_party/licenses/wasm-bindgen-macro-0.2.129/) |
| [wasm-bindgen-macro-support](https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support) | 0.2.129 | MIT OR Apache-2.0 | [notices](third_party/licenses/wasm-bindgen-macro-support-0.2.129/) |
| [wasm-bindgen-shared](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared) | 0.2.129 | MIT OR Apache-2.0 | [notices](third_party/licenses/wasm-bindgen-shared-0.2.129/) |
| [wayland-backend](https://github.com/smithay/wayland-rs) | 0.3.17 | MIT | [notices](third_party/licenses/wayland-backend-0.3.17/) |
| [wayland-client](https://github.com/smithay/wayland-rs) | 0.31.15 | MIT | [notices](third_party/licenses/wayland-client-0.31.15/) |
| [wayland-cursor](https://github.com/smithay/wayland-rs) | 0.31.14 | MIT | [notices](third_party/licenses/wayland-cursor-0.31.14/) |
| [wayland-protocols](https://github.com/smithay/wayland-rs) | 0.31.2 | MIT | [notices](third_party/licenses/wayland-protocols-0.31.2/) |
| [wayland-protocols](https://github.com/smithay/wayland-rs) | 0.32.13 | MIT | [notices](third_party/licenses/wayland-protocols-0.32.13/) |
| [wayland-protocols-plasma](https://github.com/smithay/wayland-rs) | 0.2.0 | MIT | [notices](third_party/licenses/wayland-protocols-plasma-0.2.0/) |
| [wayland-scanner](https://github.com/smithay/wayland-rs) | 0.31.11 | MIT | [notices](third_party/licenses/wayland-scanner-0.31.11/) |
| [wayland-sys](https://github.com/smithay/wayland-rs) | 0.31.11 | MIT | [notices](third_party/licenses/wayland-sys-0.31.11/) |
| [weezl](https://github.com/image-rs/weezl) | 0.1.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/weezl-0.1.12/) |
| [weezl](https://github.com/image-rs/weezl) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/weezl-0.2.1/) |
| [which](https://github.com/harryfei/which-rs.git) | 6.0.3 | MIT | [notices](third_party/licenses/which-6.0.3/) |
| [winapi](https://github.com/retep998/winapi-rs) | 0.3.9 | MIT/Apache-2.0 | [notices](third_party/licenses/winapi-0.3.9/) |
| [winapi-util](https://github.com/BurntSushi/winapi-util) | 0.1.11 | Unlicense OR MIT | [notices](third_party/licenses/winapi-util-0.1.11/) |
| [windows](https://github.com/microsoft/windows-rs) | 0.57.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-0.57.0/) |
| [windows](https://github.com/microsoft/windows-rs) | 0.61.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-0.61.3/) |
| [windows](https://github.com/microsoft/windows-rs) | 0.62.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-0.62.2/) |
| [windows-capture](https://github.com/NiiightmareXD/windows-capture) | 1.5.0 | MIT | [notices](third_party/licenses/windows-capture-1.5.0/) |
| [windows-collections](https://github.com/microsoft/windows-rs) | 0.2.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-collections-0.2.0/) |
| [windows-collections](https://github.com/microsoft/windows-rs) | 0.3.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-collections-0.3.2/) |
| [windows-core](https://github.com/microsoft/windows-rs) | 0.57.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-core-0.57.0/) |
| [windows-core](https://github.com/microsoft/windows-rs) | 0.61.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-core-0.61.2/) |
| [windows-core](https://github.com/microsoft/windows-rs) | 0.62.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-core-0.62.2/) |
| [windows-future](https://github.com/microsoft/windows-rs) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-future-0.2.1/) |
| [windows-future](https://github.com/microsoft/windows-rs) | 0.3.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-future-0.3.2/) |
| [windows-implement](https://github.com/microsoft/windows-rs) | 0.57.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-implement-0.57.0/) |
| [windows-implement](https://github.com/microsoft/windows-rs) | 0.60.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-implement-0.60.2/) |
| [windows-interface](https://github.com/microsoft/windows-rs) | 0.57.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-interface-0.57.0/) |
| [windows-interface](https://github.com/microsoft/windows-rs) | 0.59.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-interface-0.59.3/) |
| [windows-link](https://github.com/microsoft/windows-rs) | 0.1.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-link-0.1.3/) |
| [windows-link](https://github.com/microsoft/windows-rs) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-link-0.2.1/) |
| [windows-numerics](https://github.com/microsoft/windows-rs) | 0.2.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-numerics-0.2.0/) |
| [windows-numerics](https://github.com/microsoft/windows-rs) | 0.3.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-numerics-0.3.1/) |
| [windows-registry](https://github.com/microsoft/windows-rs) | 0.4.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-registry-0.4.0/) |
| [windows-registry](https://github.com/microsoft/windows-rs) | 0.5.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-registry-0.5.3/) |
| [windows-result](https://github.com/microsoft/windows-rs) | 0.1.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-result-0.1.2/) |
| [windows-result](https://github.com/microsoft/windows-rs) | 0.3.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-result-0.3.4/) |
| [windows-result](https://github.com/microsoft/windows-rs) | 0.4.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-result-0.4.1/) |
| [windows-strings](https://github.com/microsoft/windows-rs) | 0.3.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-strings-0.3.1/) |
| [windows-strings](https://github.com/microsoft/windows-rs) | 0.4.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-strings-0.4.2/) |
| [windows-strings](https://github.com/microsoft/windows-rs) | 0.5.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-strings-0.5.1/) |
| [windows-sys](https://github.com/microsoft/windows-rs) | 0.59.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-sys-0.59.0/) |
| [windows-sys](https://github.com/microsoft/windows-rs) | 0.61.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-sys-0.61.2/) |
| [windows-targets](https://github.com/microsoft/windows-rs) | 0.52.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-targets-0.52.6/) |
| [windows-targets](https://github.com/microsoft/windows-rs) | 0.53.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-targets-0.53.5/) |
| [windows-threading](https://github.com/microsoft/windows-rs) | 0.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-threading-0.1.0/) |
| [windows-threading](https://github.com/microsoft/windows-rs) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-threading-0.2.1/) |
| [windows_x86_64_msvc](https://github.com/microsoft/windows-rs) | 0.52.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows_x86_64_msvc-0.52.6/) |
| [windows_x86_64_msvc](https://github.com/microsoft/windows-rs) | 0.53.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows_x86_64_msvc-0.53.1/) |
| [winnow](https://github.com/winnow-rs/winnow) | 1.0.4 | MIT | [notices](third_party/licenses/winnow-1.0.4/) |
| [winreg](https://github.com/gentoo90/winreg-rs) | 0.55.0 | MIT | [notices](third_party/licenses/winreg-0.55.0/) |
| [winsafe](https://github.com/rodrigocfd/winsafe) | 0.0.19 | MIT | [notices](third_party/licenses/winsafe-0.0.19/) |
| [write-fonts](https://github.com/googlefonts/fontations) | 0.48.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/write-fonts-0.48.1/) |
| [writeable](https://github.com/unicode-org/icu4x) | 0.6.4 | Unicode-3.0 | [notices](third_party/licenses/writeable-0.6.4/) |
| [x11](https://github.com/AltF02/x11-rs.git) | 2.21.0 | MIT | [notices](third_party/licenses/x11-2.21.0/) |
| [x11-clipboard](https://github.com/quininer/x11-clipboard) | 0.9.3 | MIT | [notices](third_party/licenses/x11-clipboard-0.9.3/) |
| [x11rb](https://github.com/psychon/x11rb) | 0.13.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/x11rb-0.13.2/) |
| [x11rb-protocol](https://github.com/psychon/x11rb) | 0.13.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/x11rb-protocol-0.13.2/) |
| [xattr](https://github.com/Stebalien/xattr) | 0.2.3 | MIT/Apache-2.0 | [notices](third_party/licenses/xattr-0.2.3/) |
| [xattr](https://github.com/Stebalien/xattr) | 1.6.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/xattr-1.6.1/) |
| [xcb](https://github.com/rust-x-bindings/rust-xcb) | 1.7.1 | MIT | [notices](third_party/licenses/xcb-1.7.1/) |
| [xcursor](https://github.com/esposm03/xcursor-rs) | 0.3.11 | MIT | [notices](third_party/licenses/xcursor-0.3.11/) |
| [xim-ctext](https://github.com/Riey/xim-rs) | 0.3.0 | MIT | [notices](third_party/licenses/xim-ctext-0.3.0/) |
| [xim-parser](https://github.com/Riey/xim-rs) | 0.2.2 | MIT | [notices](third_party/licenses/xim-parser-0.2.2/) |
| [xkbcommon](https://github.com/rust-x-bindings/xkbcommon-rs) | 0.8.0 | MIT | [notices](third_party/licenses/xkbcommon-0.8.0/) |
| [xkeysym](https://github.com/notgull/xkeysym) | 0.2.1 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/xkeysym-0.2.1/) |
| [xmlwriter](https://github.com/RazrFalcon/xmlwriter) | 0.1.0 | MIT | [notices](third_party/licenses/xmlwriter-0.1.0/) |
| [xmp-writer](https://github.com/typst/xmp-writer) | 0.3.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/xmp-writer-0.3.3/) |
| [y4m](https://github.com/image-rs/y4m.git) | 0.8.0 | MIT | [notices](third_party/licenses/y4m-0.8.0/) |
| [yazi](https://github.com/dfrg/yazi) | 0.2.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/yazi-0.2.1/) |
| [yeslogic-fontconfig-sys](https://github.com/yeslogic/fontconfig-rs) | 6.0.1 | MIT | [notices](third_party/licenses/yeslogic-fontconfig-sys-6.0.1/) |
| [yoke](https://github.com/unicode-org/icu4x) | 0.8.3 | Unicode-3.0 | [notices](third_party/licenses/yoke-0.8.3/) |
| [yoke-derive](https://github.com/unicode-org/icu4x) | 0.8.4 | Unicode-3.0 | [notices](third_party/licenses/yoke-derive-0.8.4/) |
| [zbus](https://github.com/z-galaxy/zbus/) | 5.19.0 | MIT | [notices](third_party/licenses/zbus-5.19.0/) |
| [zbus-lockstep](https://github.com/luukvanderduim/zbus-lockstep) | 0.5.2 | MIT | [notices](third_party/licenses/zbus-lockstep-0.5.2/) |
| [zbus-lockstep-macros](https://github.com/luukvanderduim/zbus-lockstep) | 0.5.2 | MIT | [notices](third_party/licenses/zbus-lockstep-macros-0.5.2/) |
| [zbus_macros](https://github.com/z-galaxy/zbus/) | 5.19.0 | MIT | [notices](third_party/licenses/zbus_macros-5.19.0/) |
| [zbus_names](https://github.com/z-galaxy/zbus/) | 4.3.4 | MIT | [notices](third_party/licenses/zbus_names-4.3.4/) |
| [zbus_xml](https://github.com/z-galaxy/zbus/) | 5.2.1 | MIT | [notices](third_party/licenses/zbus_xml-5.2.1/) |
| [zcheapstr](https://github.com/z-galaxy/zcheapstr/) | 1.1.0 | MIT | [notices](third_party/licenses/zcheapstr-1.1.0/) |
| [zed-async-tar](https://github.com/dignifiedquire/async-tar) | 0.5.0-zed | MIT/Apache-2.0 | [notices](third_party/licenses/zed-async-tar-0.5.0-zed/) |
| [zed-font-kit](https://github.com/servo/font-kit) | 0.14.1-zed | MIT OR Apache-2.0 | [notices](third_party/licenses/zed-font-kit-0.14.1-zed/) |
| [zed-reqwest](https://github.com/seanmonstar/reqwest) | 0.12.15-zed | MIT OR Apache-2.0 | [notices](third_party/licenses/zed-reqwest-0.12.15-zed/) |
| [zed-scap](https://github.com/helmerapp/scap) | 0.0.8-zed | MIT | [notices](third_party/licenses/zed-scap-0.0.8-zed/) |
| [zed-xim](https://github.com/Riey/xim-rs) | 0.4.0-zed | MIT | [notices](third_party/licenses/zed-xim-0.4.0-zed/) |
| [zeno](https://github.com/dfrg/zeno) | 0.3.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/zeno-0.3.3/) |
| [zerocopy](https://github.com/google/zerocopy) | 0.8.59 | BSD-2-Clause OR Apache-2.0 OR MIT | [notices](third_party/licenses/zerocopy-0.8.59/) |
| [zerocopy-derive](https://github.com/google/zerocopy) | 0.8.59 | BSD-2-Clause OR Apache-2.0 OR MIT | [notices](third_party/licenses/zerocopy-derive-0.8.59/) |
| [zerofrom](https://github.com/unicode-org/icu4x) | 0.1.8 | Unicode-3.0 | [notices](third_party/licenses/zerofrom-0.1.8/) |
| [zerofrom-derive](https://github.com/unicode-org/icu4x) | 0.1.8 | Unicode-3.0 | [notices](third_party/licenses/zerofrom-derive-0.1.8/) |
| [zeroize](https://github.com/RustCrypto/utils) | 1.9.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/zeroize-1.9.0/) |
| [zeroize_derive](https://github.com/RustCrypto/utils) | 1.5.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/zeroize_derive-1.5.0/) |
| [zerotrie](https://github.com/unicode-org/icu4x) | 0.2.5 | Unicode-3.0 | [notices](third_party/licenses/zerotrie-0.2.5/) |
| [zerovec](https://github.com/unicode-org/icu4x) | 0.11.8 | Unicode-3.0 | [notices](third_party/licenses/zerovec-0.11.8/) |
| [zerovec-derive](https://github.com/unicode-org/icu4x) | 0.11.6 | Unicode-3.0 | [notices](third_party/licenses/zerovec-derive-0.11.6/) |
| [zip](https://github.com/zip-rs/zip2) | 8.6.0 | MIT | [notices](third_party/licenses/zip-8.6.0/) |
| [zlib-rs](https://github.com/trifectatechfoundation/zlib-rs) | 0.6.8 | Zlib | [notices](third_party/licenses/zlib-rs-0.6.8/) |
| [zmij](https://github.com/dtolnay/zmij) | 1.0.23 | MIT | [notices](third_party/licenses/zmij-1.0.23/) |
| [zopfli](https://github.com/zopfli-rs/zopfli) | 0.8.3 | Apache-2.0 | [notices](third_party/licenses/zopfli-0.8.3/) |
| [zune-core](https://github.com/etemesi254/zune-image) | 0.5.3 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/zune-core-0.5.3/) |
| [zune-inflate](https://crates.io/crates/zune-inflate/0.2.54) | 0.2.54 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/zune-inflate-0.2.54/) |
| [zune-jpeg](https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg) | 0.5.15 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/zune-jpeg-0.5.15/) |
| [zvariant](https://github.com/z-galaxy/zbus/) | 5.15.0 | MIT | [notices](third_party/licenses/zvariant-5.15.0/) |
| [zvariant_derive](https://github.com/z-galaxy/zbus/) | 5.15.0 | MIT | [notices](third_party/licenses/zvariant_derive-5.15.0/) |
| [zvariant_utils](https://github.com/z-galaxy/zbus/) | 4.2.0 | MIT | [notices](third_party/licenses/zvariant_utils-4.2.0/) |

## Windows package

The Windows package bundles CPython 3.13.16 (PSF license, retained in `python/LICENSE.txt`), SymPy 1.14.0 and mpmath 1.3.0 (BSD licenses in their distribution metadata), and the Poppler Windows build v26.09.0-0 with its DLLs, data and upstream notices. The exact download URLs and hashes are recorded in `runtime-assets.json`. Poppler's Windows build and source recipes are maintained at https://github.com/oschwartz10612/poppler-windows; Poppler source is at https://poppler.freedesktop.org/. AccessKit's Windows adapter is MIT or Apache-2.0. The optional llama.cpp Windows runtime retains the same MIT notice as the Linux runtime. App-local Visual C++ runtime DLLs are Microsoft redistributable code, copyright Microsoft Corporation, distributed under the Visual Studio license's redistribution terms; their hashes are recorded in `manifest.json`.

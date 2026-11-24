# Third-party licenses

Folio's original source is GPL-3.0-or-later; see [LICENSE](LICENSE). Dependencies retain their own licenses. This inventory is generated from Cargo.lock's resolved Linux and Windows graphs, including build dependencies. It records package-declared SPDX expressions without replacing upstream notices. Slash-separated legacy dual-license declarations mean alternatives.

For dual-licensed dependencies, select MIT where offered, otherwise Apache-2.0 where offered. In particular, `oo7` is used under Apache-2.0 rather than its alternative GPL-2.0-only license. MPL-2.0 `option-ext` is unmodified, and its source is available from the exact version linked below. Binary distributors must provide the matching Folio source and retained notices; the package script creates both archives.

## Bundled components and system tools

- Built-in offline stroke recognition (`folio-recognition`, `recognition/`): bundled pure-Rust INT8-quantized RoPE Conformer models (`math_v2.frec`, `htr_v2.frec`, `htr_lm.json.gz`) run directly on CPU with no external runtime or downloads. Dataset provenance and notices are in [recognition/NOTICE.txt](recognition/NOTICE.txt) (MathWriting CC-BY-4.0; Penpal MIT; DeepWriting CC BY-NC-SA 4.0; BRUSH research). Historical GLM-OCR and ConvText upstream notices remain preserved under [third_party/ocr](third_party/ocr) and [third_party/htr-convtext](third_party/htr-convtext); current recognition does not load them.
- Rune (the interface framework, pinned to a `dev` revision of https://github.com/anomalyco/rune, with its patched winit, sctk-adwaita, parley_engine, wgpu-hal and AccessKit adapters): MIT, © Anomaly. Its license is retained under `third_party/licenses/rune-*`; the patched crates keep their upstream notices.
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

The UI uses 53 embedded Tabler Outline SVGs by Paweł Kuna, licensed under MIT, with stroke width adjusted to 1.65. Source mappings, the pinned revision, upstream attribution and full license are retained in [third_party/licenses/tabler-icons](third_party/licenses/tabler-icons). The matching marker icon and notebook cover artwork are original Folio vector drawings under GPL-3.0-or-later. Goodnotes documentation was consulted for layout; no Goodnotes assets or source are redistributed.

The ink/geometric algorithms are original implementations. No perfect-freehand, Xournal++ or Rnote source was copied. [DEVELOPMENT.md](DEVELOPMENT.md) explains the design references and tradeoffs.

## Locked Rust dependencies

| Package / source | Version | Declared license | Retained text |
| --- | --- | --- | --- |
| [accesskit](https://github.com/AccessKit/accesskit) | 0.25.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/accesskit-0.25.2/) |
| [accesskit_atspi_common](https://github.com/AccessKit/accesskit) | 0.21.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/accesskit_atspi_common-0.21.1/) |
| [accesskit_consumer](https://github.com/AccessKit/accesskit) | 0.39.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/accesskit_consumer-0.39.2/) |
| [accesskit_unix](https://github.com/AccessKit/accesskit) | 0.24.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/accesskit_unix-0.24.1/) |
| [accesskit_windows](https://github.com/AccessKit/accesskit) | 0.35.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/accesskit_windows-0.35.2/) |
| [adler2](https://github.com/oyvindln/adler2) | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | [notices](third_party/licenses/adler2-2.0.1/) |
| [aes](https://github.com/RustCrypto/block-ciphers) | 0.9.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/aes-0.9.3/) |
| [ahash](https://github.com/tkaitchuck/ahash) | 0.8.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/ahash-0.8.12/) |
| [alloc-no-stdlib](https://github.com/dropbox/rust-alloc-no-stdlib) | 2.0.4 | BSD-3-Clause | [notices](third_party/licenses/alloc-no-stdlib-2.0.4/) |
| [alloc-stdlib](https://github.com/dropbox/rust-alloc-no-stdlib) | 0.2.4 | BSD-3-Clause | [notices](third_party/licenses/alloc-stdlib-0.2.4/) |
| [allocator-api2](https://github.com/zakarumych/allocator-api2) | 0.2.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/allocator-api2-0.2.21/) |
| [anyhow](https://github.com/dtolnay/anyhow) | 1.0.104 | MIT OR Apache-2.0 | [notices](third_party/licenses/anyhow-1.0.104/) |
| [ar_archive_writer](https://github.com/rust-lang/ar_archive_writer) | 0.5.3 | Apache-2.0 WITH LLVM-exception | [notices](third_party/licenses/ar_archive_writer-0.5.3/) |
| [arboard](https://github.com/1Password/arboard) | 3.7.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/arboard-3.7.0/) |
| [arrayref](https://github.com/droundy/arrayref) | 0.3.9 | BSD-2-Clause | [notices](third_party/licenses/arrayref-0.3.9/) |
| [arrayvec](https://github.com/bluss/arrayvec) | 0.7.8 | MIT OR Apache-2.0 | [notices](third_party/licenses/arrayvec-0.7.8/) |
| [as-raw-xcb-connection](https://github.com/psychon/as-raw-xcb-connection) | 1.0.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/as-raw-xcb-connection-1.0.1/) |
| [ash](https://github.com/ash-rs/ash) | 0.38.0+1.3.281 | MIT OR Apache-2.0 | [notices](third_party/licenses/ash-0.38.0+1.3.281/) |
| [async-broadcast](https://github.com/smol-rs/async-broadcast) | 0.7.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/async-broadcast-0.7.2/) |
| [async-channel](https://github.com/smol-rs/async-channel) | 2.5.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-channel-2.5.0/) |
| [async-executor](https://github.com/smol-rs/async-executor) | 1.14.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-executor-1.14.0/) |
| [async-io](https://github.com/smol-rs/async-io) | 2.6.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-io-2.6.0/) |
| [async-lock](https://github.com/smol-rs/async-lock) | 3.4.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-lock-3.4.2/) |
| [async-process](https://github.com/smol-rs/async-process) | 2.5.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-process-2.5.0/) |
| [async-recursion](https://github.com/dcchut/async-recursion) | 1.1.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/async-recursion-1.1.1/) |
| [async-signal](https://github.com/smol-rs/async-signal) | 0.2.14 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-signal-0.2.14/) |
| [async-task](https://github.com/smol-rs/async-task) | 4.7.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/async-task-4.7.1/) |
| [async-trait](https://github.com/dtolnay/async-trait) | 0.1.92 | MIT OR Apache-2.0 | [notices](third_party/licenses/async-trait-0.1.92/) |
| [atomic-waker](https://github.com/smol-rs/atomic-waker) | 1.1.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/atomic-waker-1.1.2/) |
| [atspi](https://github.com/odilia-app/atspi) | 0.29.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/atspi-0.29.0/) |
| [atspi-common](https://github.com/odilia-app/atspi) | 0.13.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/atspi-common-0.13.0/) |
| [atspi-proxies](https://github.com/odilia-app/atspi) | 0.13.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/atspi-proxies-0.13.0/) |
| [autocfg](https://github.com/cuviper/autocfg) | 1.5.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/autocfg-1.5.1/) |
| [base64](https://github.com/marshallpierce/rust-base64) | 0.22.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/base64-0.22.1/) |
| [base64](https://github.com/marshallpierce/rust-base64) | 0.23.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/base64-0.23.1/) |
| [bit-set](https://github.com/contain-rs/bit-set) | 0.10.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/bit-set-0.10.0/) |
| [bit-vec](https://github.com/contain-rs/bit-vec) | 0.9.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/bit-vec-0.9.1/) |
| [bitflags](https://github.com/bitflags/bitflags) | 1.3.2 | MIT/Apache-2.0 | [notices](third_party/licenses/bitflags-1.3.2/) |
| [bitflags](https://github.com/bitflags/bitflags) | 2.13.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/bitflags-2.13.2/) |
| [blake3](https://github.com/BLAKE3-team/BLAKE3) | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | [notices](third_party/licenses/blake3-1.8.7/) |
| [block-buffer](https://github.com/RustCrypto/utils) | 0.10.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/block-buffer-0.10.4/) |
| [block-buffer](https://github.com/RustCrypto/utils) | 0.12.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/block-buffer-0.12.1/) |
| [block-padding](https://github.com/RustCrypto/utils) | 0.4.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/block-padding-0.4.2/) |
| [blocking](https://github.com/smol-rs/blocking) | 1.7.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/blocking-1.7.0/) |
| [brotli-decompressor](https://github.com/dropbox/rust-brotli-decompressor) | 5.0.3 | BSD-3-Clause/MIT | [notices](third_party/licenses/brotli-decompressor-5.0.3/) |
| [bumpalo](https://github.com/fitzgen/bumpalo) | 3.20.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/bumpalo-3.20.3/) |
| [bytemuck](https://github.com/Lokathor/bytemuck) | 1.25.2 | Zlib OR Apache-2.0 OR MIT | [notices](third_party/licenses/bytemuck-1.25.2/) |
| [bytemuck_derive](https://github.com/Lokathor/bytemuck) | 1.12.1 | Zlib OR Apache-2.0 OR MIT | [notices](third_party/licenses/bytemuck_derive-1.12.1/) |
| [byteorder-lite](https://github.com/image-rs/byteorder-lite) | 0.1.0 | Unlicense OR MIT | [notices](third_party/licenses/byteorder-lite-0.1.0/) |
| [bytes](https://github.com/tokio-rs/bytes) | 1.12.1 | MIT | [notices](third_party/licenses/bytes-1.12.1/) |
| [calloop](https://github.com/Smithay/calloop) | 0.13.0 | MIT | [notices](third_party/licenses/calloop-0.13.0/) |
| [calloop-wayland-source](https://github.com/smithay/calloop-wayland-source) | 0.3.0 | MIT | [notices](third_party/licenses/calloop-wayland-source-0.3.0/) |
| [cbc](https://github.com/RustCrypto/block-modes) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/cbc-0.2.1/) |
| [cc](https://github.com/rust-lang/cc-rs) | 1.5.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/cc-1.5.1/) |
| [cfg-if](https://github.com/rust-lang/cfg-if) | 1.0.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/cfg-if-1.0.5/) |
| [cfg_aliases](https://github.com/katharostech/cfg_aliases) | 0.2.2 | MIT | [notices](third_party/licenses/cfg_aliases-0.2.2/) |
| [chacha20](https://github.com/RustCrypto/stream-ciphers) | 0.10.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/chacha20-0.10.2/) |
| [cipher](https://github.com/RustCrypto/traits) | 0.5.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/cipher-0.5.2/) |
| [clipboard-win](https://github.com/DoumanAsh/clipboard-win) | 5.4.3 | BSL-1.0 | [upstream source](https://github.com/DoumanAsh/clipboard-win) |
| [codespan-reporting](https://github.com/brendanzab/codespan) | 0.13.1 | Apache-2.0 | [notices](third_party/licenses/codespan-reporting-0.13.1/) |
| [color_quant](https://github.com/image-rs/color_quant.git) | 1.1.0 | MIT | [notices](third_party/licenses/color_quant-1.1.0/) |
| [concurrent-queue](https://github.com/smol-rs/concurrent-queue) | 2.5.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/concurrent-queue-2.5.0/) |
| [const-oid](https://github.com/RustCrypto/formats) | 0.10.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/const-oid-0.10.2/) |
| [constant_time_eq](https://github.com/cesarb/constant_time_eq) | 0.4.2 | CC0-1.0 OR MIT-0 OR Apache-2.0 | [notices](third_party/licenses/constant_time_eq-0.4.2/) |
| [core_detect](https://github.com/thomcc/core_detect) | 1.0.0 | MIT/Apache-2.0 | [notices](third_party/licenses/core_detect-1.0.0/) |
| [core_maths](https://github.com/robertbastian/core_maths) | 0.1.1 | MIT | [notices](third_party/licenses/core_maths-0.1.1/) |
| [cpubits](https://github.com/RustCrypto/utils) | 0.1.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/cpubits-0.1.1/) |
| [cpufeatures](https://github.com/RustCrypto/utils) | 0.2.17 | MIT OR Apache-2.0 | [notices](third_party/licenses/cpufeatures-0.2.17/) |
| [cpufeatures](https://github.com/RustCrypto/utils) | 0.3.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/cpufeatures-0.3.1/) |
| [crc32fast](https://github.com/srijs/rust-crc32fast) | 1.5.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/crc32fast-1.5.2/) |
| [crossbeam-deque](https://github.com/crossbeam-rs/crossbeam) | 0.8.8 | MIT OR Apache-2.0 | [notices](third_party/licenses/crossbeam-deque-0.8.8/) |
| [crossbeam-epoch](https://github.com/crossbeam-rs/crossbeam) | 0.9.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/crossbeam-epoch-0.9.21/) |
| [crossbeam-utils](https://github.com/crossbeam-rs/crossbeam) | 0.8.23 | MIT OR Apache-2.0 | [notices](third_party/licenses/crossbeam-utils-0.8.23/) |
| [crypto-common](https://github.com/RustCrypto/traits) | 0.1.7 | MIT OR Apache-2.0 | [notices](third_party/licenses/crypto-common-0.1.7/) |
| [crypto-common](https://github.com/RustCrypto/traits) | 0.2.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/crypto-common-0.2.2/) |
| [cursor-icon](https://github.com/rust-windowing/cursor-icon) | 1.2.0 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/cursor-icon-1.2.0/) |
| [data-url](https://github.com/servo/rust-url) | 0.3.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/data-url-0.3.2/) |
| [digest](https://github.com/RustCrypto/traits) | 0.10.7 | MIT OR Apache-2.0 | [notices](third_party/licenses/digest-0.10.7/) |
| [digest](https://github.com/RustCrypto/traits) | 0.11.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/digest-0.11.3/) |
| [displaydoc](https://github.com/yaahc/displaydoc) | 0.2.7 | MIT OR Apache-2.0 | [notices](third_party/licenses/displaydoc-0.2.7/) |
| [dlib](https://github.com/elinorbgr/dlib) | 0.5.3 | MIT | [notices](third_party/licenses/dlib-0.5.3/) |
| [document-features](https://github.com/slint-ui/document-features) | 0.2.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/document-features-0.2.12/) |
| [downcast-rs](https://github.com/marcianx/downcast-rs) | 1.2.1 | MIT/Apache-2.0 | [notices](third_party/licenses/downcast-rs-1.2.1/) |
| [dpi](https://github.com/rust-windowing/winit) | 0.1.2 | Apache-2.0 AND MIT | [notices](third_party/licenses/dpi-0.1.2/) |
| [ecb](https://github.com/RustCrypto/block-modes) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/ecb-0.2.1/) |
| [either](https://github.com/rayon-rs/either) | 1.18.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/either-1.18.0/) |
| [embed-resource](https://github.com/nabijaczleweli/rust-embed-resource) | 3.0.12 | MIT | [notices](third_party/licenses/embed-resource-3.0.12/) |
| [encoding_rs](https://github.com/hsivonen/encoding_rs) | 0.8.42 | (Apache-2.0 OR MIT) AND BSD-3-Clause | [notices](third_party/licenses/encoding_rs-0.8.42/) |
| [endi](https://github.com/zeenix/endi) | 1.1.1 | MIT | [notices](third_party/licenses/endi-1.1.1/) |
| [enumflags2](https://github.com/meithecatte/enumflags2) | 0.7.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/enumflags2-0.7.12/) |
| [enumflags2_derive](https://github.com/meithecatte/enumflags2) | 0.7.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/enumflags2_derive-0.7.12/) |
| [equivalent](https://github.com/indexmap-rs/equivalent) | 1.0.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/equivalent-1.0.2/) |
| [errno](https://github.com/lambda-fairy/rust-errno) | 0.3.14 | MIT OR Apache-2.0 | [notices](third_party/licenses/errno-0.3.14/) |
| [error-code](https://github.com/DoumanAsh/error-code) | 3.4.0 | BSL-1.0 | [notices](third_party/licenses/error-code-3.4.0/) |
| [euclid](https://github.com/servo/euclid) | 0.22.14 | MIT OR Apache-2.0 | [notices](third_party/licenses/euclid-0.22.14/) |
| [event-listener](https://github.com/smol-rs/event-listener) | 5.4.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/event-listener-5.4.2/) |
| [event-listener-strategy](https://github.com/smol-rs/event-listener-strategy) | 0.5.4 | Apache-2.0 OR MIT | [notices](third_party/licenses/event-listener-strategy-0.5.4/) |
| [fallible-iterator](https://github.com/sfackler/rust-fallible-iterator) | 0.3.0 | MIT/Apache-2.0 | [notices](third_party/licenses/fallible-iterator-0.3.0/) |
| [fallible-streaming-iterator](https://github.com/sfackler/fallible-streaming-iterator) | 0.1.9 | MIT/Apache-2.0 | [notices](third_party/licenses/fallible-streaming-iterator-0.1.9/) |
| [fastrand](https://github.com/smol-rs/fastrand) | 2.5.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/fastrand-2.5.0/) |
| [fax](https://github.com/pdf-rs/fax) | 0.2.7 | MIT | [notices](third_party/licenses/fax-0.2.7/) |
| [fdeflate](https://github.com/image-rs/fdeflate) | 0.3.7 | MIT OR Apache-2.0 | [notices](third_party/licenses/fdeflate-0.3.7/) |
| [filetime](https://github.com/alexcrichton/filetime) | 0.2.29 | MIT/Apache-2.0 | [notices](third_party/licenses/filetime-0.2.29/) |
| [find-msvc-tools](https://github.com/rust-lang/cc-rs) | 0.1.14 | MIT OR Apache-2.0 | [notices](third_party/licenses/find-msvc-tools-0.1.14/) |
| [fixedbitset](https://github.com/petgraph/fixedbitset) | 0.5.9 | MIT OR Apache-2.0 | [notices](third_party/licenses/fixedbitset-0.5.9/) |
| [flate2](https://github.com/rust-lang/flate2-rs) | 1.1.10 | MIT OR Apache-2.0 | [notices](third_party/licenses/flate2-1.1.10/) |
| [float-cmp](https://github.com/mikedilger/float-cmp) | 0.9.0 | MIT | [notices](third_party/licenses/float-cmp-0.9.0/) |
| [foldhash](https://github.com/orlp/foldhash) | 0.1.5 | Zlib | [notices](third_party/licenses/foldhash-0.1.5/) |
| [foldhash](https://github.com/orlp/foldhash) | 0.2.0 | Zlib | [notices](third_party/licenses/foldhash-0.2.0/) |
| [font-types](https://github.com/googlefonts/fontations) | 0.11.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/font-types-0.11.3/) |
| [font-types](https://github.com/googlefonts/fontations) | 0.12.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/font-types-0.12.5/) |
| [fontconfig-parser](https://github.com/Riey/fontconfig-parser) | 0.5.8 | MIT | [notices](third_party/licenses/fontconfig-parser-0.5.8/) |
| [fontdb](https://github.com/RazrFalcon/fontdb) | 0.23.0 | MIT | [notices](third_party/licenses/fontdb-0.23.0/) |
| [fontique](https://github.com/linebender/parley) | 0.12.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/fontique-0.12.0/) |
| [form_urlencoded](https://github.com/servo/rust-url) | 1.2.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/form_urlencoded-1.2.2/) |
| [futures](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-0.3.34/) |
| [futures-channel](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-channel-0.3.34/) |
| [futures-core](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-core-0.3.34/) |
| [futures-executor](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-executor-0.3.34/) |
| [futures-io](https://github.com/rust-lang/futures-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/futures-io-0.3.34/) |
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
| [gpu-allocator](https://github.com/Traverse-Research/gpu-allocator) | 0.28.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/gpu-allocator-0.28.0/) |
| [half](https://github.com/VoidStarKat/half-rs) | 2.7.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/half-2.7.1/) |
| [harfrust](https://github.com/harfbuzz/harfrust) | 0.12.0 | MIT | [notices](third_party/licenses/harfrust-0.12.0/) |
| [hashbrown](https://github.com/rust-lang/hashbrown) | 0.15.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/hashbrown-0.15.5/) |
| [hashbrown](https://github.com/rust-lang/hashbrown) | 0.16.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/hashbrown-0.16.1/) |
| [hashbrown](https://github.com/rust-lang/hashbrown) | 0.17.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/hashbrown-0.17.1/) |
| [hashlink](https://github.com/djc/hashlink) | 0.12.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/hashlink-0.12.2/) |
| [hex](https://github.com/KokaKiwi/rust-hex) | 0.4.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/hex-0.4.3/) |
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
| [icu_locale_fallback](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_locale_fallback-2.3.0/) |
| [icu_locale_fallback_data](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_locale_fallback_data-2.3.0/) |
| [icu_normalizer](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_normalizer-2.3.0/) |
| [icu_normalizer_data](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_normalizer_data-2.3.0/) |
| [icu_properties](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_properties-2.3.0/) |
| [icu_properties_data](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_properties_data-2.3.0/) |
| [icu_provider](https://github.com/unicode-org/icu4x) | 2.3.1 | Unicode-3.0 | [notices](third_party/licenses/icu_provider-2.3.1/) |
| [icu_segmenter](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_segmenter-2.3.0/) |
| [icu_segmenter_data](https://github.com/unicode-org/icu4x) | 2.3.0 | Unicode-3.0 | [notices](third_party/licenses/icu_segmenter_data-2.3.0/) |
| [idna](https://github.com/servo/rust-url/) | 1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/idna-1.1.0/) |
| [idna_adapter](https://github.com/hsivonen/idna_adapter) | 1.2.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/idna_adapter-1.2.2/) |
| [image](https://github.com/image-rs/image) | 0.25.10 | MIT OR Apache-2.0 | [notices](third_party/licenses/image-0.25.10/) |
| [image-webp](https://github.com/image-rs/image-webp) | 0.2.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/image-webp-0.2.4/) |
| [imagesize](https://github.com/Roughsketch/imagesize) | 0.13.0 | MIT | [notices](third_party/licenses/imagesize-0.13.0/) |
| [imagesize](https://github.com/Roughsketch/imagesize) | 0.14.0 | MIT | [notices](third_party/licenses/imagesize-0.14.0/) |
| [indexmap](https://github.com/indexmap-rs/indexmap) | 2.14.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/indexmap-2.14.2/) |
| [inout](https://github.com/RustCrypto/utils) | 0.2.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/inout-0.2.2/) |
| [ipnet](https://github.com/krisprice/ipnet) | 2.12.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/ipnet-2.12.2/) |
| [itoa](https://github.com/dtolnay/itoa) | 1.0.18 | MIT OR Apache-2.0 | [notices](third_party/licenses/itoa-1.0.18/) |
| [jobserver](https://github.com/rust-lang/jobserver-rs) | 0.1.35 | MIT OR Apache-2.0 | [notices](third_party/licenses/jobserver-0.1.35/) |
| [krilla](https://github.com/LaurenzV/krilla) | 0.8.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/krilla-0.8.2/) |
| [krilla-svg](https://github.com/LaurenzV/krilla) | 0.8.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/krilla-svg-0.8.1/) |
| [kurbo](https://github.com/linebender/kurbo) | 0.11.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/kurbo-0.11.3/) |
| [kurbo](https://github.com/linebender/kurbo) | 0.13.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/kurbo-0.13.1/) |
| [latex-rust](https://github.com/jscarr64/LaTeX-Rust) | 2.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/latex-rust-2.1.0/) |
| [libc](https://github.com/rust-lang/libc) | 0.2.190 | MIT OR Apache-2.0 | [notices](third_party/licenses/libc-0.2.190/) |
| [libloading](https://github.com/nagisa/rust_libloading/) | 0.8.9 | ISC | [notices](third_party/licenses/libloading-0.8.9/) |
| [libm](https://github.com/rust-lang/compiler-builtins) | 0.2.16 | MIT | [notices](third_party/licenses/libm-0.2.16/) |
| [libsqlite3-sys](https://github.com/rusqlite/rusqlite) | 0.38.2 | MIT | [notices](third_party/licenses/libsqlite3-sys-0.38.2/) |
| [linebender_resource_handle](https://github.com/linebender/raw_resource_handle) | 0.1.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/linebender_resource_handle-0.1.1/) |
| [linux-raw-sys](https://github.com/sunfishcode/linux-raw-sys) | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [notices](third_party/licenses/linux-raw-sys-0.12.1/) |
| [linux-raw-sys](https://github.com/sunfishcode/linux-raw-sys) | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [notices](third_party/licenses/linux-raw-sys-0.4.15/) |
| [litemap](https://github.com/unicode-org/icu4x) | 0.8.3 | Unicode-3.0 | [notices](third_party/licenses/litemap-0.8.3/) |
| [litrs](https://github.com/LukasKalbertodt/litrs) | 1.0.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/litrs-1.0.1/) |
| [lock_api](https://github.com/Amanieu/parking_lot) | 0.4.14 | MIT OR Apache-2.0 | [notices](third_party/licenses/lock_api-0.4.14/) |
| [log](https://github.com/rust-lang/log) | 0.4.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/log-0.4.34/) |
| [lopdf](https://github.com/J-F-Liu/lopdf.git) | 0.45.0 | MIT | [notices](third_party/licenses/lopdf-0.45.0/) |
| [lru-slab](https://github.com/Ralith/lru-slab) | 0.1.3 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/lru-slab-0.1.3/) |
| [md-5](https://github.com/RustCrypto/hashes) | 0.11.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/md-5-0.11.0/) |
| [memchr](https://github.com/BurntSushi/memchr) | 2.8.3 | Unlicense OR MIT | [notices](third_party/licenses/memchr-2.8.3/) |
| [memmap2](https://github.com/RazrFalcon/memmap2-rs) | 0.9.11 | MIT OR Apache-2.0 | [notices](third_party/licenses/memmap2-0.9.11/) |
| [miniz_oxide](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) | 0.8.9 | MIT OR Zlib OR Apache-2.0 | [notices](third_party/licenses/miniz_oxide-0.8.9/) |
| [miniz_oxide](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) | 0.9.1 | MIT OR Zlib OR Apache-2.0 | [notices](third_party/licenses/miniz_oxide-0.9.1/) |
| [mio](https://github.com/tokio-rs/mio) | 1.2.3 | MIT | [notices](third_party/licenses/mio-1.2.3/) |
| [moxcms](https://github.com/awxkee/moxcms.git) | 0.8.1 | BSD-3-Clause OR Apache-2.0 | [notices](third_party/licenses/moxcms-0.8.1/) |
| [multiversion_no_op](https://github.com/hsivonen/multiversion_no_op) | 1.0.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/multiversion_no_op-1.0.0/) |
| [naga](https://github.com/gfx-rs/wgpu) | 30.0.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/naga-30.0.2/) |
| [naga-types](https://github.com/gfx-rs/wgpu) | 30.0.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/naga-types-30.0.2/) |
| [nom](https://github.com/rust-bakery/nom) | 8.0.0 | MIT | [notices](third_party/licenses/nom-8.0.0/) |
| [num-traits](https://github.com/rust-num/num-traits) | 0.2.19 | MIT OR Apache-2.0 | [notices](third_party/licenses/num-traits-0.2.19/) |
| [object](https://github.com/gimli-rs/object) | 0.39.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/object-0.39.1/) |
| [once_cell](https://github.com/matklad/once_cell) | 1.21.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/once_cell-1.21.4/) |
| [openssl-probe](https://github.com/rustls/openssl-probe) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/openssl-probe-0.2.1/) |
| [ordered-float](https://github.com/reem/rust-ordered-float) | 5.5.1 | MIT | [notices](third_party/licenses/ordered-float-5.5.1/) |
| [ordered-stream](https://github.com/danieldg/ordered-stream) | 0.2.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/ordered-stream-0.2.0/) |
| [os_pipe](https://github.com/oconnor663/os_pipe.rs) | 1.2.3 | MIT | [notices](third_party/licenses/os_pipe-1.2.3/) |
| [parking](https://github.com/smol-rs/parking) | 2.2.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/parking-2.2.1/) |
| [parking_lot](https://github.com/Amanieu/parking_lot) | 0.12.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/parking_lot-0.12.5/) |
| [parking_lot_core](https://github.com/Amanieu/parking_lot) | 0.9.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/parking_lot_core-0.9.12/) |
| [parlance](https://github.com/linebender/parley) | 0.1.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/parlance-0.1.1/) |
| [parley](https://github.com/linebender/parley) | 0.12.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/parley-0.12.0/) |
| [parley_data](https://github.com/linebender/parley) | 0.12.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/parley_data-0.12.0/) |
| [parley_emoji](https://github.com/linebender/parley) | 0.12.0 | MIT | [notices](third_party/licenses/parley_emoji-0.12.0/) |
| [parley_engine](https://github.com/linebender/parley) | 0.12.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/parley_engine-0.12.0/) |
| [pdf-writer](https://github.com/typst/pdf-writer) | 0.15.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/pdf-writer-0.15.0/) |
| [percent-encoding](https://github.com/servo/rust-url/) | 2.3.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/percent-encoding-2.3.2/) |
| [petgraph](https://github.com/petgraph/petgraph) | 0.8.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/petgraph-0.8.3/) |
| [phf](https://github.com/rust-phf/rust-phf) | 0.14.0 | MIT | [notices](third_party/licenses/phf-0.14.0/) |
| [phf_generator](https://github.com/rust-phf/rust-phf) | 0.14.0 | MIT | [notices](third_party/licenses/phf_generator-0.14.0/) |
| [phf_macros](https://github.com/rust-phf/rust-phf) | 0.14.0 | MIT | [notices](third_party/licenses/phf_macros-0.14.0/) |
| [phf_shared](https://github.com/rust-phf/rust-phf) | 0.14.0 | MIT | [notices](third_party/licenses/phf_shared-0.14.0/) |
| [pico-args](https://github.com/RazrFalcon/pico-args) | 0.5.0 | MIT | [notices](third_party/licenses/pico-args-0.5.0/) |
| [pin-project-lite](https://github.com/taiki-e/pin-project-lite) | 0.2.17 | Apache-2.0 OR MIT | [notices](third_party/licenses/pin-project-lite-0.2.17/) |
| [piper](https://github.com/smol-rs/piper) | 0.2.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/piper-0.2.5/) |
| [pkg-config](https://github.com/rust-lang/pkg-config-rs) | 0.3.34 | MIT OR Apache-2.0 | [notices](third_party/licenses/pkg-config-0.3.34/) |
| [png](https://github.com/image-rs/image-png) | 0.17.16 | MIT OR Apache-2.0 | [notices](third_party/licenses/png-0.17.16/) |
| [png](https://github.com/image-rs/image-png) | 0.18.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/png-0.18.1/) |
| [polling](https://github.com/smol-rs/polling) | 3.11.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/polling-3.11.0/) |
| [pollster](https://github.com/zesterer/pollster) | 1.0.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/pollster-1.0.3/) |
| [polycool](https://github.com/linebender/kurbo) | 0.4.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/polycool-0.4.0/) |
| [portable-atomic](https://github.com/taiki-e/portable-atomic) | 1.15.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/portable-atomic-1.15.0/) |
| [potential_utf](https://github.com/unicode-org/icu4x) | 0.1.6 | Unicode-3.0 | [notices](third_party/licenses/potential_utf-0.1.6/) |
| [presser](https://github.com/EmbarkStudios/presser) | 0.3.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/presser-0.3.1/) |
| [proc-macro-crate](https://github.com/bkchr/proc-macro-crate) | 3.5.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/proc-macro-crate-3.5.0/) |
| [proc-macro2](https://github.com/dtolnay/proc-macro2) | 1.0.107 | MIT OR Apache-2.0 | [notices](third_party/licenses/proc-macro2-1.0.107/) |
| [profiling](https://github.com/aclysma/profiling) | 1.0.18 | MIT OR Apache-2.0 | [notices](third_party/licenses/profiling-1.0.18/) |
| [psm](https://github.com/rust-lang/stacker/) | 0.1.32 | MIT OR Apache-2.0 | [notices](third_party/licenses/psm-0.1.32/) |
| [pxfm](https://github.com/awxkee/pxfm) | 0.1.30 | BSD-3-Clause OR Apache-2.0 | [notices](third_party/licenses/pxfm-0.1.30/) |
| [quick-error](http://github.com/tailhook/quick-error) | 2.0.1 | MIT/Apache-2.0 | [notices](third_party/licenses/quick-error-2.0.1/) |
| [quick-xml](https://github.com/tafia/quick-xml) | 0.41.0 | MIT | [notices](third_party/licenses/quick-xml-0.41.0/) |
| [quinn](https://github.com/quinn-rs/quinn) | 0.11.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/quinn-0.11.12/) |
| [quinn-proto](https://github.com/quinn-rs/quinn) | 0.11.19 | MIT OR Apache-2.0 | [notices](third_party/licenses/quinn-proto-0.11.19/) |
| [quinn-udp](https://github.com/quinn-rs/quinn) | 0.5.16 | MIT OR Apache-2.0 | [notices](third_party/licenses/quinn-udp-0.5.16/) |
| [quote](https://github.com/dtolnay/quote) | 1.0.47 | MIT OR Apache-2.0 | [notices](third_party/licenses/quote-1.0.47/) |
| [rand](https://github.com/rust-random/rand) | 0.10.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand-0.10.3/) |
| [rand_core](https://github.com/rust-random/rand_core) | 0.10.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand_core-0.10.1/) |
| [rand_pcg](https://github.com/rust-random/rngs) | 0.10.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/rand_pcg-0.10.2/) |
| [range-alloc](https://github.com/gfx-rs/range-alloc) | 0.1.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/range-alloc-0.1.5/) |
| [rangemap](https://github.com/jeffparsons/rangemap) | 1.8.0 | MIT/Apache-2.0 | [notices](third_party/licenses/rangemap-1.8.0/) |
| [raw-window-handle](https://github.com/rust-windowing/raw-window-handle) | 0.6.2 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/raw-window-handle-0.6.2/) |
| [rayon](https://github.com/rayon-rs/rayon) | 1.12.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/rayon-1.12.0/) |
| [rayon-core](https://github.com/rayon-rs/rayon) | 1.13.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/rayon-core-1.13.0/) |
| [read-fonts](https://github.com/googlefonts/fontations) | 0.39.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/read-fonts-0.39.2/) |
| [read-fonts](https://github.com/googlefonts/fontations) | 0.41.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/read-fonts-0.41.0/) |
| [renderdoc-sys](https://github.com/ebkalderon/renderdoc-rs) | 1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/renderdoc-sys-1.1.0/) |
| [reqwest](https://github.com/seanmonstar/reqwest) | 0.12.28 | MIT OR Apache-2.0 | [notices](third_party/licenses/reqwest-0.12.28/) |
| [resvg](https://github.com/linebender/resvg) | 0.45.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/resvg-0.45.1/) |
| [resvg](https://github.com/linebender/resvg) | 0.47.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/resvg-0.47.0/) |
| [rfd](https://github.com/PolyMeilex/rfd) | 0.17.2 | MIT | [notices](third_party/licenses/rfd-0.17.2/) |
| [rgb](https://github.com/kornelski/rust-rgb) | 0.8.53 | MIT | [notices](third_party/licenses/rgb-0.8.53/) |
| [ring](https://github.com/briansmith/ring) | 0.17.14 | Apache-2.0 AND ISC | [notices](third_party/licenses/ring-0.17.14/) |
| [roxmltree](https://github.com/RazrFalcon/roxmltree) | 0.20.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/roxmltree-0.20.0/) |
| [roxmltree](https://github.com/RazrFalcon/roxmltree) | 0.21.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/roxmltree-0.21.1/) |
| [rune](https://crates.io/crates/rune/0.0.1) | 0.0.1 | MIT | [notices](third_party/licenses/rune-0.0.1/) |
| [rune-core](https://crates.io/crates/rune-core/0.0.1) | 0.0.1 | MIT | [notices](third_party/licenses/rune-core-0.0.1/) |
| [rune-edit](https://crates.io/crates/rune-edit/0.0.1) | 0.0.1 | MIT | [notices](third_party/licenses/rune-edit-0.0.1/) |
| [rune-gpu](https://crates.io/crates/rune-gpu/0.0.1) | 0.0.1 | MIT | [notices](third_party/licenses/rune-gpu-0.0.1/) |
| [rune-image](https://crates.io/crates/rune-image/0.0.1) | 0.0.1 | MIT | [notices](third_party/licenses/rune-image-0.0.1/) |
| [rune-media](https://crates.io/crates/rune-media/0.0.1) | 0.0.1 | MIT | [notices](third_party/licenses/rune-media-0.0.1/) |
| [rune-motion](https://crates.io/crates/rune-motion/0.1.0) | 0.1.0 | MIT | [notices](third_party/licenses/rune-motion-0.1.0/) |
| [rune-spell](https://crates.io/crates/rune-spell/0.0.1) | 0.0.1 | MIT | [notices](third_party/licenses/rune-spell-0.0.1/) |
| [rune-staging](https://crates.io/crates/rune-staging/0.0.1) | 0.0.1 | MIT | [notices](third_party/licenses/rune-staging-0.0.1/) |
| [rune-taffy](https://crates.io/crates/rune-taffy/0.0.1) | 0.0.1 | MIT | [notices](third_party/licenses/rune-taffy-0.0.1/) |
| [rune-text](https://crates.io/crates/rune-text/0.0.1) | 0.0.1 | MIT | [notices](third_party/licenses/rune-text-0.0.1/) |
| [rusqlite](https://github.com/rusqlite/rusqlite) | 0.40.2 | MIT | [notices](third_party/licenses/rusqlite-0.40.2/) |
| [rustc-hash](https://github.com/rust-lang-nursery/rustc-hash) | 1.1.0 | Apache-2.0/MIT | [notices](third_party/licenses/rustc-hash-1.1.0/) |
| [rustc-hash](https://github.com/rust-lang/rustc-hash) | 2.1.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/rustc-hash-2.1.3/) |
| [rustc_version](https://github.com/djc/rustc-version-rs) | 0.4.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/rustc_version-0.4.1/) |
| [rustix](https://github.com/bytecodealliance/rustix) | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [notices](third_party/licenses/rustix-0.38.44/) |
| [rustix](https://github.com/bytecodealliance/rustix) | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [notices](third_party/licenses/rustix-1.1.5/) |
| [rustls](https://github.com/rustls/rustls) | 0.23.45 | Apache-2.0 OR ISC OR MIT | [notices](third_party/licenses/rustls-0.23.45/) |
| [rustls-native-certs](https://github.com/rustls/rustls-native-certs) | 0.8.4 | Apache-2.0 OR ISC OR MIT | [notices](third_party/licenses/rustls-native-certs-0.8.4/) |
| [rustls-pki-types](https://github.com/rustls/pki-types) | 1.15.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/rustls-pki-types-1.15.1/) |
| [rustls-webpki](https://github.com/rustls/webpki) | 0.103.15 | ISC | [notices](third_party/licenses/rustls-webpki-0.103.15/) |
| [rustversion](https://github.com/dtolnay/rustversion) | 1.0.23 | MIT OR Apache-2.0 | [notices](third_party/licenses/rustversion-1.0.23/) |
| [rustybuzz](https://github.com/harfbuzz/rustybuzz) | 0.20.1 | MIT | [notices](third_party/licenses/rustybuzz-0.20.1/) |
| [ryu](https://github.com/dtolnay/ryu) | 1.0.23 | Apache-2.0 OR BSL-1.0 | [notices](third_party/licenses/ryu-1.0.23/) |
| [schannel](https://github.com/steffengy/schannel-rs) | 0.1.29 | MIT | [notices](third_party/licenses/schannel-0.1.29/) |
| [scoped-tls](https://github.com/alexcrichton/scoped-tls) | 1.0.1 | MIT/Apache-2.0 | [notices](third_party/licenses/scoped-tls-1.0.1/) |
| [scopeguard](https://github.com/bluss/scopeguard) | 1.2.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/scopeguard-1.2.0/) |
| [sctk-adwaita](https://github.com/PolyMeilex/sctk-adwaita) | 0.10.1 | MIT | [notices](third_party/licenses/sctk-adwaita-0.10.1/) |
| [semver](https://github.com/dtolnay/semver) | 1.0.28 | MIT OR Apache-2.0 | [notices](third_party/licenses/semver-1.0.28/) |
| [serde](https://github.com/serde-rs/serde) | 1.0.229 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde-1.0.229/) |
| [serde_core](https://github.com/serde-rs/serde) | 1.0.229 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_core-1.0.229/) |
| [serde_derive](https://github.com/serde-rs/serde) | 1.0.229 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_derive-1.0.229/) |
| [serde_json](https://github.com/serde-rs/json) | 1.0.151 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_json-1.0.151/) |
| [serde_repr](https://github.com/dtolnay/serde-repr) | 0.1.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_repr-0.1.21/) |
| [serde_spanned](https://github.com/toml-rs/toml) | 1.1.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/serde_spanned-1.1.3/) |
| [serde_urlencoded](https://github.com/nox/serde_urlencoded) | 0.7.1 | MIT/Apache-2.0 | [notices](third_party/licenses/serde_urlencoded-0.7.1/) |
| [sha2](https://github.com/RustCrypto/hashes) | 0.10.9 | MIT OR Apache-2.0 | [notices](third_party/licenses/sha2-0.10.9/) |
| [sha2](https://github.com/RustCrypto/hashes) | 0.11.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/sha2-0.11.0/) |
| [shlex](https://github.com/comex/rust-shlex) | 2.0.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/shlex-2.0.1/) |
| [signal-hook-registry](https://github.com/vorner/signal-hook) | 1.4.8 | MIT OR Apache-2.0 | [notices](third_party/licenses/signal-hook-registry-1.4.8/) |
| [simd-adler32](https://github.com/mcountryman/simd-adler32) | 0.3.10 | MIT | [notices](third_party/licenses/simd-adler32-0.3.10/) |
| [simdutf8](https://github.com/rusticstuff/simdutf8) | 0.1.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/simdutf8-0.1.5/) |
| [simplecss](https://github.com/linebender/simplecss) | 0.2.2 | Apache-2.0 OR MIT | [notices](third_party/licenses/simplecss-0.2.2/) |
| [siphasher](https://github.com/jedisct1/rust-siphash) | 1.0.4 | MIT OR Apache-2.0 | [notices](third_party/licenses/siphasher-1.0.4/) |
| [skrifa](https://github.com/googlefonts/fontations) | 0.42.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/skrifa-0.42.1/) |
| [skrifa](https://github.com/googlefonts/fontations) | 0.44.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/skrifa-0.44.0/) |
| [slab](https://github.com/tokio-rs/slab) | 0.4.12 | MIT | [notices](third_party/licenses/slab-0.4.12/) |
| [slotmap](https://github.com/orlp/slotmap) | 1.1.1 | Zlib | [notices](third_party/licenses/slotmap-1.1.1/) |
| [smallvec](https://github.com/servo/rust-smallvec) | 1.16.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/smallvec-1.16.2/) |
| [smithay-client-toolkit](https://github.com/smithay/client-toolkit) | 0.19.2 | MIT | [notices](third_party/licenses/smithay-client-toolkit-0.19.2/) |
| [smol_str](https://github.com/rust-analyzer/smol_str) | 0.2.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/smol_str-0.2.2/) |
| [socket2](https://github.com/rust-lang/socket2) | 0.6.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/socket2-0.6.5/) |
| [spirv](https://github.com/gfx-rs/rspirv) | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | [notices](third_party/licenses/spirv-0.4.0+sdk-1.4.341.0/) |
| [stable_deref_trait](https://github.com/storyyeller/stable_deref_trait) | 1.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/stable_deref_trait-1.2.1/) |
| [stacker](https://github.com/rust-lang/stacker) | 0.1.25 | MIT OR Apache-2.0 | [notices](third_party/licenses/stacker-0.1.25/) |
| [static_assertions](https://github.com/nvzqz/static-assertions-rs) | 1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/static_assertions-1.1.0/) |
| [strict-num](https://github.com/RazrFalcon/strict-num) | 0.1.1 | MIT | [notices](third_party/licenses/strict-num-0.1.1/) |
| [stringprep](https://github.com/sfackler/rust-stringprep) | 0.1.5 | MIT/Apache-2.0 | [notices](third_party/licenses/stringprep-0.1.5/) |
| [subsetter](https://github.com/typst/subsetter) | 0.2.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/subsetter-0.2.6/) |
| [subtle](https://github.com/dalek-cryptography/subtle) | 2.6.1 | BSD-3-Clause | [notices](third_party/licenses/subtle-2.6.1/) |
| [svgtypes](https://github.com/linebender/svgtypes) | 0.15.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/svgtypes-0.15.3/) |
| [svgtypes](https://github.com/linebender/svgtypes) | 0.16.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/svgtypes-0.16.1/) |
| [swash](https://github.com/dfrg/swash) | 0.2.10 | Apache-2.0 OR MIT | [notices](third_party/licenses/swash-0.2.10/) |
| [syn](https://github.com/dtolnay/syn) | 2.0.119 | MIT OR Apache-2.0 | [notices](third_party/licenses/syn-2.0.119/) |
| [syn](https://github.com/dtolnay/syn) | 3.0.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/syn-3.0.6/) |
| [sync_wrapper](https://github.com/Actyx/sync_wrapper) | 1.0.2 | Apache-2.0 | [notices](third_party/licenses/sync_wrapper-1.0.2/) |
| [synstructure](https://github.com/mystor/synstructure) | 0.14.0 | MIT | [notices](third_party/licenses/synstructure-0.14.0/) |
| [taffy](https://github.com/DioxusLabs/taffy) | 0.15.1 | MIT | [notices](third_party/licenses/taffy-0.15.1/) |
| [tar](https://github.com/composefs/tar-rs) | 0.4.46 | MIT OR Apache-2.0 | [notices](third_party/licenses/tar-0.4.46/) |
| [tempfile](https://github.com/Stebalien/tempfile) | 3.27.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/tempfile-3.27.0/) |
| [termcolor](https://github.com/BurntSushi/termcolor) | 1.4.1 | Unlicense OR MIT | [notices](third_party/licenses/termcolor-1.4.1/) |
| [thiserror](https://github.com/dtolnay/thiserror) | 1.0.69 | MIT OR Apache-2.0 | [notices](third_party/licenses/thiserror-1.0.69/) |
| [thiserror](https://github.com/dtolnay/thiserror) | 2.0.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/thiserror-2.0.21/) |
| [thiserror-impl](https://github.com/dtolnay/thiserror) | 1.0.69 | MIT OR Apache-2.0 | [notices](third_party/licenses/thiserror-impl-1.0.69/) |
| [thiserror-impl](https://github.com/dtolnay/thiserror) | 2.0.21 | MIT OR Apache-2.0 | [notices](third_party/licenses/thiserror-impl-2.0.21/) |
| [tiff](https://github.com/image-rs/image-tiff) | 0.11.3 | MIT | [notices](third_party/licenses/tiff-0.11.3/) |
| [tiny-skia](https://github.com/RazrFalcon/tiny-skia) | 0.11.4 | BSD-3-Clause | [notices](third_party/licenses/tiny-skia-0.11.4/) |
| [tiny-skia](https://github.com/linebender/tiny-skia) | 0.12.0 | BSD-3-Clause | [notices](third_party/licenses/tiny-skia-0.12.0/) |
| [tiny-skia-path](https://github.com/RazrFalcon/tiny-skia/tree/master/path) | 0.11.4 | BSD-3-Clause | [notices](third_party/licenses/tiny-skia-path-0.11.4/) |
| [tiny-skia-path](https://github.com/RazrFalcon/tiny-skia/tree/master/path) | 0.11.4 | BSD-3-Clause | [notices](third_party/licenses/tiny-skia-path-0.11.4/) |
| [tiny-skia-path](https://github.com/linebender/tiny-skia/tree/master/path) | 0.12.0 | BSD-3-Clause | [notices](third_party/licenses/tiny-skia-path-0.12.0/) |
| [tinystr](https://github.com/unicode-org/icu4x) | 0.8.4 | Unicode-3.0 | [notices](third_party/licenses/tinystr-0.8.4/) |
| [tinyvec](https://github.com/Lokathor/tinyvec) | 1.13.3 | Zlib OR Apache-2.0 OR MIT | [notices](third_party/licenses/tinyvec-1.13.3/) |
| [tokio](https://github.com/tokio-rs/tokio) | 1.53.1 | MIT | [notices](third_party/licenses/tokio-1.53.1/) |
| [tokio-rustls](https://github.com/rustls/tokio-rustls) | 0.26.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/tokio-rustls-0.26.6/) |
| [toml](https://github.com/toml-rs/toml) | 1.2.2+spec-1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/toml-1.2.2+spec-1.1.0/) |
| [toml_datetime](https://github.com/toml-rs/toml) | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/toml_datetime-1.1.3+spec-1.1.0/) |
| [toml_edit](https://github.com/toml-rs/toml) | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/toml_edit-0.25.15+spec-1.1.0/) |
| [toml_parser](https://github.com/toml-rs/toml) | 1.1.6+spec-1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/toml_parser-1.1.6+spec-1.1.0/) |
| [toml_writer](https://github.com/toml-rs/toml) | 1.2.0+spec-1.1.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/toml_writer-1.2.0+spec-1.1.0/) |
| [tower](https://github.com/tower-rs/tower) | 0.5.3 | MIT | [notices](third_party/licenses/tower-0.5.3/) |
| [tower-http](https://github.com/tower-rs/tower-http) | 0.6.11 | MIT | [notices](third_party/licenses/tower-http-0.6.11/) |
| [tower-layer](https://github.com/tower-rs/tower) | 0.3.3 | MIT | [notices](third_party/licenses/tower-layer-0.3.3/) |
| [tower-service](https://github.com/tower-rs/tower) | 0.3.3 | MIT | [notices](third_party/licenses/tower-service-0.3.3/) |
| [tracing](https://github.com/tokio-rs/tracing) | 0.1.44 | MIT | [notices](third_party/licenses/tracing-0.1.44/) |
| [tracing-attributes](https://github.com/tokio-rs/tracing) | 0.1.31 | MIT | [notices](third_party/licenses/tracing-attributes-0.1.31/) |
| [tracing-core](https://github.com/tokio-rs/tracing) | 0.1.36 | MIT | [notices](third_party/licenses/tracing-core-0.1.36/) |
| [tree_magic_mini](https://github.com/mbrubeck/tree_magic/) | 3.2.2 | MIT | [notices](third_party/licenses/tree_magic_mini-3.2.2/) |
| [try-lock](https://github.com/seanmonstar/try-lock) | 0.2.5 | MIT | [notices](third_party/licenses/try-lock-0.2.5/) |
| [ttf-parser](https://github.com/harfbuzz/ttf-parser) | 0.25.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/ttf-parser-0.25.1/) |
| [typenum](https://github.com/paholg/typenum) | 1.20.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/typenum-1.20.1/) |
| [unicode-bidi](https://github.com/servo/unicode-bidi) | 0.3.18 | MIT OR Apache-2.0 | [notices](third_party/licenses/unicode-bidi-0.3.18/) |
| [unicode-bidi-mirroring](https://github.com/RazrFalcon/unicode-bidi-mirroring) | 0.4.0 | MIT/Apache-2.0 | [notices](third_party/licenses/unicode-bidi-mirroring-0.4.0/) |
| [unicode-ccc](https://github.com/RazrFalcon/unicode-ccc) | 0.4.0 | MIT/Apache-2.0 | [notices](third_party/licenses/unicode-ccc-0.4.0/) |
| [unicode-ident](https://github.com/dtolnay/unicode-ident) | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | [notices](third_party/licenses/unicode-ident-1.0.26/) |
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
| [utf8_iter](https://github.com/hsivonen/utf8_iter) | 1.0.4 | Apache-2.0 OR MIT | [notices](third_party/licenses/utf8_iter-1.0.4/) |
| [uuid](https://github.com/uuid-rs/uuid) | 1.26.1 | Apache-2.0 OR MIT | [notices](third_party/licenses/uuid-1.26.1/) |
| [vcpkg](https://github.com/mcgoo/vcpkg-rs) | 0.2.15 | MIT/Apache-2.0 | [notices](third_party/licenses/vcpkg-0.2.15/) |
| [version_check](https://github.com/SergioBenitez/version_check) | 0.9.5 | MIT/Apache-2.0 | [notices](third_party/licenses/version_check-0.9.5/) |
| [vswhom](https://github.com/nabijaczleweli/vswhom.rs) | 0.1.0 | MIT | [notices](third_party/licenses/vswhom-0.1.0/) |
| [vswhom-sys](https://github.com/nabijaczleweli/vswhom-sys.rs) | 0.1.3 | MIT | [notices](third_party/licenses/vswhom-sys-0.1.3/) |
| [want](https://github.com/seanmonstar/want) | 0.3.1 | MIT | [notices](third_party/licenses/want-0.3.1/) |
| [wayland-backend](https://github.com/smithay/wayland-rs) | 0.3.17 | MIT | [notices](third_party/licenses/wayland-backend-0.3.17/) |
| [wayland-client](https://github.com/smithay/wayland-rs) | 0.31.15 | MIT | [notices](third_party/licenses/wayland-client-0.31.15/) |
| [wayland-csd-frame](https://github.com/rust-windowing/wayland-csd-frame) | 0.3.0 | MIT | [notices](third_party/licenses/wayland-csd-frame-0.3.0/) |
| [wayland-cursor](https://github.com/smithay/wayland-rs) | 0.31.14 | MIT | [notices](third_party/licenses/wayland-cursor-0.31.14/) |
| [wayland-protocols](https://github.com/smithay/wayland-rs) | 0.32.13 | MIT | [notices](third_party/licenses/wayland-protocols-0.32.13/) |
| [wayland-protocols-plasma](https://github.com/smithay/wayland-rs) | 0.3.12 | MIT | [notices](third_party/licenses/wayland-protocols-plasma-0.3.12/) |
| [wayland-protocols-wlr](https://github.com/smithay/wayland-rs) | 0.3.12 | MIT | [notices](third_party/licenses/wayland-protocols-wlr-0.3.12/) |
| [wayland-scanner](https://github.com/smithay/wayland-rs) | 0.31.11 | MIT | [notices](third_party/licenses/wayland-scanner-0.31.11/) |
| [wayland-sys](https://github.com/smithay/wayland-rs) | 0.31.11 | MIT | [notices](third_party/licenses/wayland-sys-0.31.11/) |
| [weezl](https://github.com/image-rs/weezl) | 0.1.12 | MIT OR Apache-2.0 | [notices](third_party/licenses/weezl-0.1.12/) |
| [weezl](https://github.com/image-rs/weezl) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/weezl-0.2.1/) |
| [wgpu](https://github.com/gfx-rs/wgpu) | 30.0.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/wgpu-30.0.2/) |
| [wgpu-core](https://github.com/gfx-rs/wgpu) | 30.0.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/wgpu-core-30.0.2/) |
| [wgpu-core-deps-windows-linux-android](https://github.com/gfx-rs/wgpu) | 30.0.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/wgpu-core-deps-windows-linux-android-30.0.2/) |
| [wgpu-hal](https://github.com/gfx-rs/wgpu) | 30.0.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/wgpu-hal-30.0.1/) |
| [wgpu-naga-bridge](https://github.com/gfx-rs/wgpu) | 30.0.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/wgpu-naga-bridge-30.0.2/) |
| [wgpu-types](https://github.com/gfx-rs/wgpu) | 30.0.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/wgpu-types-30.0.2/) |
| [winapi-util](https://github.com/BurntSushi/winapi-util) | 0.1.11 | Unlicense OR MIT | [notices](third_party/licenses/winapi-util-0.1.11/) |
| [windows](https://github.com/microsoft/windows-rs) | 0.62.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-0.62.2/) |
| [windows-collections](https://github.com/microsoft/windows-rs) | 0.3.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-collections-0.3.2/) |
| [windows-core](https://github.com/microsoft/windows-rs) | 0.62.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-core-0.62.2/) |
| [windows-future](https://github.com/microsoft/windows-rs) | 0.3.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-future-0.3.2/) |
| [windows-implement](https://github.com/microsoft/windows-rs) | 0.60.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-implement-0.60.2/) |
| [windows-interface](https://github.com/microsoft/windows-rs) | 0.59.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-interface-0.59.3/) |
| [windows-link](https://github.com/microsoft/windows-rs) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-link-0.2.1/) |
| [windows-numerics](https://github.com/microsoft/windows-rs) | 0.3.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-numerics-0.3.1/) |
| [windows-result](https://github.com/microsoft/windows-rs) | 0.4.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-result-0.4.1/) |
| [windows-strings](https://github.com/microsoft/windows-rs) | 0.5.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-strings-0.5.1/) |
| [windows-sys](https://github.com/microsoft/windows-rs) | 0.52.0 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-sys-0.52.0/) |
| [windows-sys](https://github.com/microsoft/windows-rs) | 0.61.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-sys-0.61.2/) |
| [windows-targets](https://github.com/microsoft/windows-rs) | 0.52.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-targets-0.52.6/) |
| [windows-threading](https://github.com/microsoft/windows-rs) | 0.2.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows-threading-0.2.1/) |
| [windows_x86_64_msvc](https://github.com/microsoft/windows-rs) | 0.52.6 | MIT OR Apache-2.0 | [notices](third_party/licenses/windows_x86_64_msvc-0.52.6/) |
| [winit](https://github.com/rust-windowing/winit) | 0.30.13 | Apache-2.0 | [notices](third_party/licenses/winit-0.30.13/) |
| [winnow](https://github.com/winnow-rs/winnow) | 1.0.4 | MIT | [notices](third_party/licenses/winnow-1.0.4/) |
| [winreg](https://github.com/gentoo90/winreg-rs) | 0.56.0 | MIT | [notices](third_party/licenses/winreg-0.56.0/) |
| [wl-clipboard-rs](https://github.com/YaLTeR/wl-clipboard-rs) | 0.9.5 | MIT OR Apache-2.0 | [notices](third_party/licenses/wl-clipboard-rs-0.9.5/) |
| [write-fonts](https://github.com/googlefonts/fontations) | 0.48.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/write-fonts-0.48.1/) |
| [writeable](https://github.com/unicode-org/icu4x) | 0.6.4 | Unicode-3.0 | [notices](third_party/licenses/writeable-0.6.4/) |
| [x11-dl](https://github.com/AltF02/x11-rs.git) | 2.21.0 | MIT | [notices](third_party/licenses/x11-dl-2.21.0/) |
| [x11rb](https://github.com/psychon/x11rb) | 0.13.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/x11rb-0.13.2/) |
| [x11rb-protocol](https://github.com/psychon/x11rb) | 0.13.2 | MIT OR Apache-2.0 | [notices](third_party/licenses/x11rb-protocol-0.13.2/) |
| [xattr](https://github.com/Stebalien/xattr) | 1.6.1 | MIT OR Apache-2.0 | [notices](third_party/licenses/xattr-1.6.1/) |
| [xcursor](https://github.com/esposm03/xcursor-rs) | 0.3.11 | MIT | [notices](third_party/licenses/xcursor-0.3.11/) |
| [xkbcommon](https://github.com/rust-x-bindings/xkbcommon-rs) | 0.9.0 | MIT | [notices](third_party/licenses/xkbcommon-0.9.0/) |
| [xkbcommon-dl](https://github.com/rust-windowing/xkbcommon-dl) | 0.4.2 | MIT | [notices](third_party/licenses/xkbcommon-dl-0.4.2/) |
| [xkeysym](https://github.com/notgull/xkeysym) | 0.2.1 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/xkeysym-0.2.1/) |
| [xmlwriter](https://github.com/RazrFalcon/xmlwriter) | 0.1.0 | MIT | [notices](third_party/licenses/xmlwriter-0.1.0/) |
| [xmp-writer](https://github.com/typst/xmp-writer) | 0.3.3 | MIT OR Apache-2.0 | [notices](third_party/licenses/xmp-writer-0.3.3/) |
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
| [zeno](https://github.com/dfrg/zeno) | 0.3.3 | Apache-2.0 OR MIT | [notices](third_party/licenses/zeno-0.3.3/) |
| [zerocopy](https://github.com/google/zerocopy) | 0.8.59 | BSD-2-Clause OR Apache-2.0 OR MIT | [notices](third_party/licenses/zerocopy-0.8.59/) |
| [zerocopy-derive](https://github.com/google/zerocopy) | 0.8.59 | BSD-2-Clause OR Apache-2.0 OR MIT | [notices](third_party/licenses/zerocopy-derive-0.8.59/) |
| [zerofrom](https://github.com/unicode-org/icu4x) | 0.1.8 | Unicode-3.0 | [notices](third_party/licenses/zerofrom-0.1.8/) |
| [zerofrom-derive](https://github.com/unicode-org/icu4x) | 0.1.8 | Unicode-3.0 | [notices](third_party/licenses/zerofrom-derive-0.1.8/) |
| [zeroize](https://github.com/RustCrypto/utils) | 1.9.0 | Apache-2.0 OR MIT | [notices](third_party/licenses/zeroize-1.9.0/) |
| [zerotrie](https://github.com/unicode-org/icu4x) | 0.2.5 | Unicode-3.0 | [notices](third_party/licenses/zerotrie-0.2.5/) |
| [zerovec](https://github.com/unicode-org/icu4x) | 0.11.8 | Unicode-3.0 | [notices](third_party/licenses/zerovec-0.11.8/) |
| [zerovec-derive](https://github.com/unicode-org/icu4x) | 0.11.6 | Unicode-3.0 | [notices](third_party/licenses/zerovec-derive-0.11.6/) |
| [zlib-rs](https://github.com/trifectatechfoundation/zlib-rs) | 0.6.8 | Zlib | [notices](third_party/licenses/zlib-rs-0.6.8/) |
| [zmij](https://github.com/dtolnay/zmij) | 1.0.23 | MIT | [notices](third_party/licenses/zmij-1.0.23/) |
| [zune-core](https://github.com/etemesi254/zune-image) | 0.5.3 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/zune-core-0.5.3/) |
| [zune-jpeg](https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg) | 0.5.15 | MIT OR Apache-2.0 OR Zlib | [notices](third_party/licenses/zune-jpeg-0.5.15/) |
| [zvariant](https://github.com/z-galaxy/zbus/) | 5.15.0 | MIT | [notices](third_party/licenses/zvariant-5.15.0/) |
| [zvariant_derive](https://github.com/z-galaxy/zbus/) | 5.15.0 | MIT | [notices](third_party/licenses/zvariant_derive-5.15.0/) |
| [zvariant_utils](https://github.com/z-galaxy/zbus/) | 4.2.0 | MIT | [notices](third_party/licenses/zvariant_utils-4.2.0/) |

## Windows package

The Windows package bundles CPython 3.13.16 (PSF license, retained in `python/LICENSE.txt`), SymPy 1.14.0 and mpmath 1.3.0 (BSD licenses in their distribution metadata), and the Poppler Windows build v26.09.0-0 with its DLLs, data and upstream notices. The exact download URLs and hashes are recorded in `runtime-assets.json`. Poppler's Windows build and source recipes are maintained at https://github.com/oschwartz10612/poppler-windows; Poppler source is at https://poppler.freedesktop.org/. AccessKit's Windows adapter is MIT or Apache-2.0. The optional llama.cpp Windows runtime retains the same MIT notice as the Linux runtime. App-local Visual C++ runtime DLLs are Microsoft redistributable code, copyright Microsoft Corporation, distributed under the Visual Studio license's redistribution terms; their hashes are recorded in `manifest.json`.

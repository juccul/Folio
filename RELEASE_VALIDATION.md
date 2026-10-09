# Folio 0.1.4 release validation

## Flatpak updater repair

The original update fixture used a one-second portal polling interval and a
preapproved update permission. That omitted the first-use path on a default
desktop. An independent reproduction with the default portal produced the exact
45-second timeout; changing only polling to one second bypassed it.

The repair directly starts the explicitly requested transaction. Ten private
D-Bus contract tests cover absent/stale availability, progress queued before the
method reply, authorization failure, already-deployed updates, mismatched commits,
bounded metadata and cancellation. A read-only latest-deployment Spawn checks the
commit before Ready and before restart. A signed metadata repair test restricts
same-version changes to an unstaged Flatpak commit with unchanged Windows fields.
Twenty-two updater tests and strict all-target updater Clippy passed.

The release gate is an isolated native UI update from repaired 0.1.3 to repaired
0.1.4 with the default real Flatpak portal, no polling override and no permission
pregrant. It must click Update, approve the first-use dialog, reach Restart to
update, back up, restart and preserve the fixture library. Final evidence and
hashes are recorded under artifacts/update-hotfix. The local installation is
patched to repaired 0.1.3 so its owner can repeat that upgrade themselves.

Windows binaries are not changed by this Linux repair. Their original complete
corresponding source remains available separately; repaired source is provided
for the updated Flatpak packages. The previous validation record follows.

The release includes the standard HSV color box, automatic system theme detection,
zoom-independent circle-selection checks, held-circle priority, dialog Escape
handling and Windows version-resource fixes since 0.1.3.

## Local source checks

- 312 workspace Rust tests passed; four existing optional/OCR tests were ignored.
- HSV conversion round-trips 4,096 RGB colors with alpha intact and checks box
  corners and grayscale hue preservation.
- Strict workspace/all-target Clippy and formatting passed.
- Native color-picker verification uses an isolated Xvfb and D-Bus demo session
  with real mouse input and keyboard controls. Both Light and Dark passed all
  checks: Save/Cancel, draft-only changes, pointer focus, Enter confirmation,
  drag clamping/release, grayscale hue memory, transparency, ink/text/paper/theme
  persistence and undo preservation. Twenty screenshots were captured and the
  footer was verified at 1000 × 620.
- Native circle checks passed at 100% and 800% zoom and with overlapping scratch
  detection, including live hold, move, undo/redo and durable close.
- Native dialog/Escape isolation and system-theme portal checks passed, including
  live changes, manual overrides, dialog lifecycle and saved preferences.
- Ten Windows updater recovery/progress tests and the math protocol test passed.
- Four Rust readiness tests cover work lasting longer than thirty seconds,
  missing/repeated progress, the preparation ceiling and bounded protocol parsing.
- Four packaging tests and 21 actual ZIP-import math regressions passed. All
  pinned math wheel members and original license bytes remain available. Windows
  portable ZIP entries dropped from 4,520 to 660.
- The exact published 0.1.3 portable helper prepared the compact prototype in
  19.684 seconds, versus 53.31 seconds for the original layout. Its actual restart
  reopened the same library with all notes, pages, saved objects and six explicit
  preferences intact. The old thirty-second limit remains in published 0.1.3;
  0.1.4 follows real preparation progress and still stops stalled helpers safely.
- Windows native mouse selection saved the expected RGB color. Native UI,
  synthetic Windows Ink pressure/tilt, offline math/PDF, manual upgrade and
  uninstall checks passed on the preceding final-picker payload. The final
  updater/package payload is verified separately before publication.
- All packaging and upgrade verification uses disposable installations and fixture
  libraries. The user's installed 0.1.3 and notes remain separate.

Final native, Windows installer, Flatpak runtime and update verification reports
and package hashes are saved under `artifacts/release-0.1.4` during release assembly.
The complete corresponding source uses the release version and pinned runtime
sources. The release retains the Ed25519 update key and GPG Flatpak signing key;
private keys are kept outside source and release archives.

The following record is retained for the previous release.

# Folio 0.1.3 release validation

The source repository and release downloads are public at
https://github.com/juccul/Folio. The signed Flatpak repository is hosted at
https://juccul.github.io/Folio/flatpak/. Anonymous summary, signature and installation
reference downloads matched the local signed repository. Private signing keys are
stored outside the source and distribution trees. An audit of 1,566 published
history blobs and 1,860 working files found no matches for private keys or common
GitHub, AWS, Google or Slack access-token formats. Only main, release tags and the
separate distribution branch are published; local task checkpoint refs are excluded.

## Completed checks

- Linux workspace: **299 Rust tests passed**, four existing opt-in/OCR tests ignored.
- Windows workspace: **295 Rust tests passed** on the native MSVC/Windows 11 VM
  on the final native source build, with 393 compiled-source fingerprints matched.
- Strict workspace/all-target Clippy and formatting passed.
- Fresh-checkout Linux math setup passed after fixing a missing Cargo target
  directory exposed by GitHub CI. The fix only affects build-tool setup.
- **11 updater tests** cover signed metadata, numeric version comparisons,
  prerelease rejection, bounded metadata, corrupt packages, range-resume behavior,
  cache verification, user-action gating, duplicate clicks, Flatpak initial-signal
  races, wrong remote commits, permission denial and failed/empty transactions.
- **Seven Python recovery tests** passed on Linux and Windows: installer failure,
  partial backup failure, portable replacement, checksum tampering, unsafe archives,
  mismatched binaries and libraries inside the installation directory.
- Native Linux smoke checks rendered the Update, Downloading and Restart controls
  and verified their position before the window controls. Accessibility/shortcuts,
  creation-options and single-Library-button checks passed.
- Six native layout cases passed at 1000×620 and 1366×768 with scales 0.8, 1.0 and
  1.6, including first launch. The first run failed a favorite-button scroll lookup;
  the unchanged test passed on rerun. Both logs are retained.
- Actual Flatpak bundle installation passed version, executable hash, offline
  SymPy/mpmath, Unicode requests, malformed-request recovery, PDF rasterization,
  AppStream and native UI smoke checks on Freedesktop Platform 25.08.
- A disposable 0.1.2 Flatpak installation downloaded and deployed 0.1.3 through
  the real Flatpak update portal from a signed local HTTP OSTree repository. A
  second run used the restart portal and confirmed the launched sandbox used the
  advertised 0.1.3 commit. A further real-portal run downloaded and restarted into
  the exact signed commit from the public HTTPS endpoint. Its first fixture export
  had a timestamp newer than the release and was correctly rejected by Flatpak;
  exporting the old-version fixture with an earlier timestamp passed.
  Package permissions remained IPC/network, X11/Wayland
  and GPU access, with no host filesystem/host-command permission added.
- Actual Windows installer and portable update helpers waited for graceful app
  exit, installed 0.1.3, verified the executable hash, retained the previous payload,
  reopened the same custom library and preserved note IDs, saved objects and order.
  The test also exercised Windows canonical path prefixes and spaces.
- Actual manual Windows upgrade from 0.1.2 passed native UI, app-local CRT,
  offline math/PDF runtime and synthetic Windows Ink checks. Installation did not
  touch the library; opening migrated the disposable schema-3 fixture to schema 6
  without changing saved objects. Uninstall preserved all validation databases.
  The first UI Automation attachment failed; using a foreground native window and
  an explicit process-ID lookup passed on rerun. First-attempt diagnostics remain.

All upgrade tests used disposable installations and copied fixture libraries.
Windows uninstall registration and shortcut were backed up and restored around
installation tests. The VM's ordinary installation and real user notes were not
upgraded. Flatpak portal permission approval used a private permission-store
instance; the user's desktop update permissions were not changed.

## Distribution checks

- Signed Ed25519 metadata identifies immutable versioned Windows assets by size,
  SHA-256 and executable hash, and the exact GPG-signed Flatpak commit.
- The public Flatpak installation reference and single-file bundle include the
  update remote and public signing key. Catalog metadata and icons are included.
- Migration from an actual 0.1.2 bundle through an explicitly named public remote
  passed and changed the origin to the signed remote. Opening `.flatpakref` on an
  already installed bundle fails even with `--reinstall`; the documented migration
  uses a new bundle or `remote-add` followed by named-remote installation.
- Windows registration and shortcut were verified restored; the ordinary 0.1.1
  installation remained untouched, and the validation VM was shut down.
- Corresponding source includes the locked Cargo registry sources, local patches,
  notices and matching Windows/Flatpak Poppler source and packaging recipes.
- Release checksums are provided in `SHA256SUMS` alongside the signed manifest.

## Package identity

The Flatpak commit and Windows executable digest are in `folio-update.json`.
The Linux release executable digest is
`aa2013c68c64876e99501b2fba52a9f21cc962213cd4cbc4a7cefd9bf7013af4`.

## Limits

The checks do not guarantee every machine, power-loss condition, antivirus policy
or physical tablet. The Flatpak tests ran on this Fedora host with isolated user
installations; a system installation requiring authorization depends on the host
portal and policy. First-use update permission was preapproved only in the private
test permission store. Native GUI tests use a virtual display; Windows pen tests
use synthetic Windows Ink. Local bundle checks disabled the documents portal
because this host lacks its expected document mount. No app sandbox permissions
were widened to work around that host condition.

Detailed evidence is retained locally in `artifacts/release-0.1.3/`.

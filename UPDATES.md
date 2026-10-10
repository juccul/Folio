# Folio updates

Folio checks a small signed release manifest at startup and every ten minutes.
A newer stable version appears at the right of the title bar, immediately before
the window controls. Clicking **Update** starts the download. When verification
finishes, the button becomes **Restart to update**. There are no automatic
package downloads or automatic restarts. Settings → Updates provides a manual
check and failure details.

Cached offers stay hidden at startup until a fresh signed manifest confirms them.
If the published version is no newer than the running app, Folio clears the offer
and any pending restart target, then stores the current metadata. Staged packages
and recovery files remain available for recovery. A failed check reports its error
without changing the saved metadata.

Restart finishes the current text edit, saves every open document and workspace,
and creates a `before-update-VERSION-UUID.foliobackup` in the current library.
A failed save or backup keeps Folio open. Imports, recognition, equations and
other unfinished document operations must finish before restart. Backups are
ordinary Folio archives and can be restored into a separate library from Settings.
They include the database, history, preferences and document assets.

## Windows

The installer and portable ZIP both support updates. A private helper runs from
Folio's application-data directory, outside the installation. The helper verifies
the staged package again, checks write access and free space, and refuses to
replace an installation used by another Folio window. It never forcibly kills an
application or writer. After the current Folio process exits, it runs the per-user
installer with the existing installation directory, or swaps the portable payload.
It checks the resulting executable against the signed release and relaunches the
same library. The previous payload is retained beside the installation for recovery.
If installation fails, the helper restores that payload and reopens Folio.

Folio follows actual package hashing, extraction and verification progress during
preparation. It permits slow storage to finish while the app remains open, stops
a helper after two minutes without progress, and has a thirty-minute preparation
ceiling. The portable package stores pure-Python math dependencies in a standard
importable ZIP and preserves license originals in a separate archive with
searchable full notices, reducing installation file writes.

Keep libraries outside the program directory. Portable updates replace the entire
program directory; keep personal files outside it too. Custom installation and
library paths, including Unicode and spaces, are supported. A complete package
is required: copying only `folio.exe` omits the embedded Python update helper,
PDF tools and math runtime. Update logs and the retained-payload path are under
`Folio/updates/v2/job-*/` in local application data.

To update manually, close all Folio windows and run the latest setup EXE.
Use the same installation directory. Do not uninstall with a third-party cleaner
or remove your library directory. The installer never removes note libraries.
For portable installations, extract into a new folder and open the same library
with `--data-dir`; do not merge a new ZIP into a running copy.

## Flatpak

Updates use Flatpak's signed OSTree repository and `org.freedesktop.portal.Flatpak`.
The app does not receive host filesystem access or unrestricted host-command
permissions. Download/install begins only after clicking Update. Flatpak deploys
the new version atomically while the running app continues using its old deployment.
Restart uses the portal's latest-version flag and waits for the old library writer
to exit. Other Flatpak software managers may independently download updates
according to the user's system preferences; Folio does not control those preferences.

New installations can use `https://juccul.github.io/Folio/folio.flatpakref`.
New single-file bundles include the same update remote and its public signing key.
If an older single-file bundle lacks the Folio update remote, close Folio and install
the new bundle with `flatpak install --user --reinstall PATH.flatpak` to upgrade
and connect the remote (omit `--user` for a system installation).

Alternatively, migrate an older installation directly from the public remote:

```sh
flatpak remote-add --user --if-not-exists folio https://juccul.github.io/Folio/folio.flatpakrepo
flatpak install --user --reinstall folio io.github.folio.Notes
```

Opening a `.flatpakref` alone reports "already installed" for an old bundle,
even with `--reinstall`; use the bundle or the named-remote commands above.
The application ID remains `io.github.folio.Notes`, so the existing library stays
available. Never use `flatpak uninstall --delete-data` to upgrade.

If a release adds sandbox permissions, the update portal cannot install it.
Folio reports the failure; use the system software manager or Flatpak CLI for that
update. A disconnected remote, cancelled permission prompt or interrupted network
operation leaves the running deployment intact and offers a retry.

## Moving from the retired releases

The new 0.1.0 distribution replaces the retired 0.1.0–0.1.4 releases. It contains
the latest code and retains database schema 6 and document format 4; its lower
version number does not downgrade your notes. Existing installations require one
manual replacement because their updater only offers higher version numbers.
Close every Folio window, then use the new Windows installer or complete portable
ZIP as described above. For Flatpak, reinstall the new bundle or the named public
remote in the same user or system installation. Keep your library and app data.

The reset uses `updates/v2/` under Folio's application-data directory for signed
metadata, packages and helper jobs. Old `updates/release.json`, staged packages
and recovery jobs remain untouched and are never imported into the new update
sequence. A successful signed check also replaces obsolete metadata in the active
namespace, including offers retained across a Flatpak reinstall. Later releases
keep this namespace and must increase the version; signature verification and
downgrade protection remain in place.

## Publishing future releases

The public source and release repository is `https://github.com/juccul/Folio`.
Installed apps read its releases anonymously. The signed Flatpak remote is
`https://juccul.github.io/Folio/flatpak/`; install
`https://juccul.github.io/Folio/folio.flatpakref` to connect to it.
Each release also provides its complete corresponding GPL source archive.
Never embed a GitHub token or a private signing key in the app.

`packaging/updates/channel.json` contains the release repository and the public
Ed25519 verification key embedded in the executable. `folio-repository.gpg` is
only the public Flatpak repository key. Private keys are stored outside the
workspace, under `~/.local/share/folio-release-keys/`, with private permissions.
Back up those keys securely; losing them breaks updates for installed clients.

1. Increase the workspace version for every release and hotfix. Published update
   versions are immutable; replacing assets under an old version will not notify
   clients already running that version.
2. Run the checks, build Windows natively with MSVC and Linux with the supported
   Flatpak baseline, and stage the complete runtime packages.
3. Build Flatpak with `scripts/package-flatpak.py --update-repo-url HTTPS_URL
   --gpg-sign FINGERPRINT --gpg-homedir PRIVATE_DIRECTORY
   --gpg-public-key packaging/updates/folio-repository.gpg`, plus the normal
   binary/output arguments. Publish its signed repository, including `summary`,
   `summary.sig`, refs, objects and static deltas.
4. Generate `folio-update.json` with `scripts/sign-update.py --dist DIST
   --flatpak-repo REPO --key PRIVATE_ED25519_PEM` only after the actual final
   installer, ZIP and Flatpak commit exist. This signs the version, immutable
   asset URLs, sizes, SHA-256 digests, executable hash and Flatpak commit.
5. Upload packages and corresponding source to the distribution release. Publish
   the Flatpak remote first; publish the signed manifest last and mark that release
   latest only once every asset is reachable. Check anonymous access to each URL.
6. Verify a disposable previous installation from the new version sequence can
   discover, download, restart and retain
   its library through both platform paths. Record the exact hashes and limitations
   under ignored `artifacts/validation/` and summarize them in the release description.

No updater can guarantee success on every machine: disk failures, power loss,
security software, inaccessible drives and host portal policies remain external
conditions. Folio checks these where possible and preserves notes and recovery
paths when an operation fails.

Flatpak's first update may also show a desktop permission dialog. Approval is
handled by the host portal; a denied or unavailable dialog is reported as a
recoverable failure. Folio does not silently alter the desktop's permission store.
Libraries under `/tmp` or `/var/tmp` cannot be restarted safely into a new sandbox;
move them to persistent app storage first.

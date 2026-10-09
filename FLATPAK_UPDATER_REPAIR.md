# Flatpak updater repair for Folio 0.1.3 and 0.1.4

These two Flatpak releases were reissued with an explicitly requested updater
repair. The original code waited 45 seconds for an availability signal, while
Flatpak's default portal polls every 30 minutes. The repair starts the portal
transaction when Update is clicked, waits for first-use authorization, and
checks the actual deployed commit before offering Restart to update. It checks
the deployment again before restarting. No package is fetched by release checks.

The repaired 0.1.3 can update to the repaired 0.1.4 through the normal UI. An
already installed original build needs one manual replacement with the repaired
bundle of the same version; downloading the bundle alone does not patch a
currently running process. Close and reopen Folio after that replacement.
For a system installation, use `flatpak install --system --reinstall PATH.flatpak`;
for a user installation, use `flatpak install --user --reinstall PATH.flatpak`.
Neither command deletes the app's library.

The release pages identify the reissued Flatpak/source/metadata files. Windows
packages are unchanged, with their original corresponding source retained as
`folio-VERSION-original-source.tar.gz`. Original source tags are retained as
`vVERSION-original`. The repaired client accepts a signed same-version change
only to the Flatpak commit, while no deployment has been staged and every
Windows payload remains identical. An already verified restart target stays
pinned. This repair is an exception; future hotfixes should increase the version.


The updater library passes 22 tests, including 10 private D-Bus Flatpak contract tests. Final default-portal/native update evidence is recorded with the reissued releases.

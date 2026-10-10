# Windows title bar hotfix

Home and the update notice now consume pointer presses at a wrapper around the
button. The child still receives its press and release, while the surrounding
title bar only starts native movement for background presses. Previously a
Home or Update press entered Windows' synchronous move loop and could lose the
release needed to activate the button. Right presses on these controls also
stay out of the title bar's native window menu. Disabled download/preparation
notices consume their presses too.

The native `--smoke-test` now sends pointer press/release events to the available
and downloading notices and verifies that Home opens the library. The update
fixture uses a disabled updater, so it downloads no packages and never replaces
an installation. The same smoke check runs through `scripts/verify-windows.ps1`.

Apply this hotfix to the current source and to branches based on `v0.1.3` and
`v0.1.4`. The 0.1.3 backport additionally includes the existing 0.1.4 Windows
preparation-progress fix (`4683bf8`), replacing the fixed thirty-second deadline
with real-work progress, an inactivity timeout and an absolute preparation limit.
The 0.1.4 branch already contains that fix. Linux resize changes are separate.

Backport branches preserve their release versions for rebuilding. Creating the
branches does not change published tags, binaries or signed release metadata.
An existing installation needs a rebuilt package to receive the fix. A future
release should increase the version so installed clients discover it normally;
reissuing existing assets requires regenerating and signing metadata after the
actual Windows payloads are built and tested.

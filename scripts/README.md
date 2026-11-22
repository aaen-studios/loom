# scripts

Calx: building and testing everything, releases, the install script for servers, and generating the design tokens. Run each from the repository's root.

- `calx scripts/check.calx` checks and tests all of loom's Calx code, as CI does: formatting, that every file builds, every file's tests, and the design tokens.
- `calx scripts/icons.calx` makes the Mac and iPhone app icons from the PNGs in `design/logo/png`.
- `calx scripts/wordmark.calx` outlines the wordmark into `design/logo/wordmark.svg`.
- `calx scripts/tokens.calx` generates the design tokens from `design/tokens.toml` into `design/generated/`. `--check` fails if they're out of date.
- `calx scripts/ios.calx` builds the core for the iPhone app as `build/ios/LoomCore.xcframework` and generates `ios/Loom.xcodeproj` from `ios/project.yml`, on a Mac. `--sim` builds for the simulator alone, and `--sample` makes the project without the core ([ios/README.md](../ios/README.md)).
- `calx scripts/release.calx` makes and checks releases (PLAN.md, "Updates and versions"): `sign` writes and signs `manifest.json` with the release key, `verify` checks it, `pack` puts `loom.exe` into loom's installer under a signed header, and `bridge` writes the `update.json` v0.1's own updater reads, signed with v0.1's key, to move v0.1 over. `key` and `seed` make the release key and give it for the CI secret. The comment at its top says what each takes.

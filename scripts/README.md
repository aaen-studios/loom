# scripts

Calx: building and testing everything, releases, the install script for servers, and generating the design tokens. Run each from the repository's root.

- `calx scripts/check.calx` checks and tests all of loom's Calx code, as CI does: formatting, that every file builds, every file's tests, and the design tokens.
- `calx scripts/tokens.calx` generates the design tokens from `design/tokens.toml` into `design/generated/`. `--check` fails if they're out of date.
- `calx scripts/ios.calx` builds the core for the iPhone app as `build/ios/LoomCore.xcframework` and generates `ios/Loom.xcodeproj` from `ios/project.yml`, on a Mac. `--sim` builds for the simulator alone, and `--sample` makes the project without the core ([ios/README.md](../ios/README.md)).

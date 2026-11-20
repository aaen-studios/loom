# scripts

Calx: building and testing everything, releases, the install script for servers, and generating the design tokens. Run each from the repository's root.

- `calx scripts/check.calx` checks and tests all of loom's Calx code, as CI does: formatting, that every file builds, every file's tests, and the design tokens.
- `calx scripts/tokens.calx` generates the design tokens from `design/tokens.toml` into `design/generated/`. `--check` fails if they're out of date.

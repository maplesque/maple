# Publishing

This repository publishes the `maple-render-core` crate through GitHub Actions.

## One-time setup

1. Create a crates.io API token with publish permissions.
2. In GitHub repo settings, add it as:
   - **Name:** `CARGO_REGISTRY_TOKEN`
   - **Secret value:** your crates.io token

## Release flow (recommended)

1. Bump `version` in `crates/maple-render-core/Cargo.toml`.
2. Commit and push the version bump.
3. Create and push a tag in this format:

   ```bash
   git tag core-v<version>
   git push origin core-v<version>
   ```

4. CI runs checks, then publishes `maple-render-core` to crates.io.

> The workflow validates that the tag version matches the crate version.

## Manual publish flow

You can also publish from **Actions → CI → Run workflow**:

- Set input `publish = true`
- Run workflow on the branch containing the release commit

## What CI does before publish

The `CI` workflow runs:

- `cargo fmt --all -- --check`
- `cargo clippy --locked --all-targets -- -D warnings` (root crate)
- `cargo check --locked` (root crate)
- `cargo test --locked --lib` (root crate)
- `cargo check --locked --lib --target wasm32-unknown-unknown` (root wasm exports)
- `cargo check --manifest-path crates/maple-render-core/Cargo.toml`
- `cargo test --manifest-path crates/maple-render-core/Cargo.toml`
- `cargo publish --dry-run --allow-dirty --manifest-path crates/maple-render-core/Cargo.toml`

Then it executes real publish when triggered by tag/manual publish mode.

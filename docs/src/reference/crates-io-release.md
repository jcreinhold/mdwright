# Crates.io release

The release workflow publishes the component crates to crates.io with a single `cargo publish --workspace` and, in
parallel, lets cargo-dist create the GitHub Release with binary artifacts. The two are **decoupled**: a crates.io hiccup
does not block the binary release or the docs deploy, and is repaired afterwards with the **Release recovery** workflow.
The workflow runs when a `v<semver>` tag is pushed. A manual `dry_run` dispatch runs the same gates but skips the live
crates.io upload (it runs `cargo publish --workspace --dry-run`) and GitHub Release creation.

## One-time setup

Create a scoped crates.io token with `publish-new`, `publish-update`, and `yank` permissions. Add it to the GitHub
repository as the Actions secret `CARGO_REGISTRY_TOKEN`.

## Local preflight

Run the local gate before tagging. `scripts/prerelease.sh` mirrors the workflow's `verify` job
command-for-command (fmt, clippy, tests, docs, generated-doc `--check`, mdBook, the public-API diff over all nine
crates, the docs.rs packaging simulation, and actionlint), so passing it is the fast feedback loop for a green release:

```sh
scripts/prerelease.sh
```

If the public-API diff reports an intentional change, regenerate the baselines in the same commit:

```sh
scripts/update-api-review.sh
```

## Version and changelog

The tag must match the workspace package version exactly. For version `0.1.0`, tag `v0.1.0`.

`CHANGELOG.md` must contain a release section named `## [0.1.0]`. The release workflow extracts that section before any
crate is published.

## Dry run

Before tagging, run the `Release` workflow manually with `dry_run: true`. It verifies the workspace, builds the
cargo-dist artifacts, checks package contents, simulates docs.rs from packaged tarballs, runs
`cargo publish --workspace --dry-run` (a full packaging + verify build of every crate without uploading), and creates
no GitHub Release.

## Publish

After the release commit is on `main`, create and push the tag:

```sh
git tag -s v0.1.0 -m "mdwright v0.1.0"
git push origin v0.1.0
```

The workflow runs `cargo publish --workspace`, which uploads the publishable members (`xtask` and `examples/extending`
are `publish = false`) in dependency order and waits for each crate to land in the index before the next crate's verify
build — no manual ordering or inter-crate sleep. The cargo-dist binary GitHub Release and the docs Pages deploy run in
parallel and do **not** wait on this step, so a crates.io failure never blocks them.

crates.io versions are immutable, so if publishing fails after a crate has uploaded, recovery depends on why:

- **Partial publish** (the common case — an index-propagation race, not a build break): the crate contents are fine.
  Do not bump the version or re-tag, and do **not** re-run `Release` (`cargo publish --workspace` aborts on the first
  crate already on the index before reaching the missing ones). Instead run the **Release recovery** workflow via
  `workflow_dispatch` with `version` set to the workspace version. It skips the crates already on crates.io and uploads
  only the missing ones; it is idempotent and a fully published version is a no-op. The binary GitHub Release already
  exists (it is decoupled), so recovery only fills the crates.io gap.
- **Contents must change** (a genuine build break): bump the workspace version, update the changelog, and tag a new
  commit. The already-published crates keep their old version.

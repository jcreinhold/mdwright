# Crates.io release

The release workflow publishes the component crates to crates.io and then lets cargo-dist create the GitHub Release with
binary artifacts. The workflow runs when a `v<semver>` tag is pushed. A manual `dry_run` dispatch runs the same gates
but skips crates.io upload and GitHub Release creation.

## One-time setup

Create a scoped crates.io token with `publish-new`, `publish-update`, and `yank` permissions. Add it to the GitHub
repository as the Actions secret `CARGO_REGISTRY_TOKEN`.

## Local preflight

Run the gates before tagging:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --no-fail-fast
cargo doc --workspace --no-deps
mdbook build docs/
cargo xtask doc-rules --check
cargo xtask doc-cli --check
cargo xtask doc-config --check
python3 scripts/check_package_docsrs.py --allow-dirty
actionlint .github/workflows/*.yml
```

Check public API drift:

```sh
for crate in mdwright-latex mdwright-math mdwright-document mdwright-format mdwright-lint mdwright-config mdwright-lsp mdwright; do
  cargo public-api --simplified -p "$crate" > /tmp/"$crate"-public.txt
  diff -u "docs/api-review/$crate-public.txt" /tmp/"$crate"-public.txt
done
```

If a public API change is intentional, regenerate the baselines in the same commit:

```sh
scripts/update-api-review.sh
```

## Version and changelog

The tag must match the workspace package version exactly. For version `0.1.0`, tag `v0.1.0`.

`CHANGELOG.md` must contain a release section named `## [0.1.0]`. The release workflow extracts that section before any
crate is published.

## Dry run

Before tagging, run the `Release` workflow manually with `dry_run: true`. It verifies the workspace, builds the
cargo-dist artifacts, checks package contents, simulates docs.rs from packaged tarballs, and skips live publishing.

## Publish

After the release commit is on `main`, create and push the tag:

```sh
git tag -s v0.1.0 -m "mdwright v0.1.0"
git push origin v0.1.0
```

The workflow publishes crates in dependency order:

```text
mdwright-latex -> mdwright-math -> mdwright-document -> mdwright-format -> mdwright-lint -> mdwright-config -> mdwright-lsp -> mdwright
```

It waits 90 seconds between crates so crates.io can index each newly published dependency before downstream crates are
published.

If publishing fails after a crate has uploaded, stop. Crates.io versions are immutable. Fix the problem, bump the
workspace version, update the changelog, and tag a new commit.

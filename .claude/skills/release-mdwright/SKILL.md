---
name: release-mdwright
description: Cut an mdwright release (version bump, CHANGELOG, baselines, tag) that publishes via CI. Use when releasing mdwright, publishing the workspace crates to crates.io, bumping the workspace version for a release, or cutting a vX.Y.Z tag.
disable-model-invocation: true
---

# Release mdwright

[`docs/src/reference/crates-io-release.md`](../../../docs/src/reference/crates-io-release.md) is the narrative
checklist and [`docs/src/reference/release-evidence.md`](../../../docs/src/reference/release-evidence.md) is the
release-candidate evidence procedure. **`.github/workflows/release.yml` is the executable source of truth** — when the
two disagree, trust the workflow. This skill is the checklist plus the cross-file invariants CI only catches *after* you
tag, when it is too late: crates.io versions are **immutable** (yank-only), so a botched publish burns a version
permanently.

**Publishing happens only in CI.** Pushing a `vX.Y.Z` git tag fires `release.yml`, which re-runs the full gate, then
runs `cargo publish --workspace` (uploads the nine component crates to crates.io in dependency order, waiting for index
propagation internally — `xtask` and `examples/extending` are `publish = false` and excluded). In **parallel and
decoupled** from that, cargo-dist builds the binary artifacts, creates the GitHub Release, and triggers the docs Pages
deploy — so a crates.io hiccup never blocks the binary release. NEVER run `cargo publish` locally and do not propose a
local-publish plan — the workflow is the only supported path. A *partial* crates.io publish is finished by the separate
**Release recovery** workflow, not by re-running `release.yml` (see recovery below).

The publish set, in the dependency order `cargo publish --workspace` resolves (leaf crates first so each is indexed
before its dependents):

```text
mdwright-latex → mdwright-math → mdwright-mathrender → mdwright-document → mdwright-format → mdwright-lint → mdwright-config → mdwright-lsp → mdwright
```

## Steps

Steps 1–6 are reversible prep — do them freely. Step 7 (the tag push) is irreversible: it triggers the crates.io
publish. **Stop and get explicit human confirmation before running it.**

### 1. Pre-flight gate

Run the local gate from a clean tree. `scripts/prerelease.sh` mirrors the workflow's `verify` job command-for-command
(fmt, clippy, tests, docs, the three generated-doc `--check`s, mdBook, the public-API diff over all nine crates, the
docs.rs packaging simulation, and actionlint):

```sh
scripts/prerelease.sh
```

Stop on any failure — this is the same gate CI runs, so passing locally is the fast feedback loop. For a real release
*candidate* (not a patch with an obvious diff), also aggregate the heavier evidence (parser audit, mdformat parity,
production soak, fuzz replay, benches) per `release-evidence.md`:

```sh
cargo xtask release-evidence --output target/mdwright/release
```

### 2. Version bump (one version, ten places)

Pick the new `X.Y.Z`. It is pre-1.0, so a breaking change or new feature bumps the **minor**; otherwise the patch. An
MSRV bump is a minor-version change — never a patch (see `CONTRIBUTING.md`); if bumping, also update `rust-version` in
`Cargo.toml` and the `rust:` matrix in `.github/workflows/ci.yml`.

In the root `Cargo.toml`, set the version in **every** place — they must all match or publishing resolves the wrong
inter-crate dependency:

- `[workspace.package].version = "X.Y.Z"` (the binary and every crate inherit this).
- each of the nine `[workspace.dependencies]` `mdwright*` entries' `version = "X.Y.Z"`.

The workflow reads the `mdwright` package version via `cargo metadata` and asserts `"v${version}" == "${tag}"` before
anything publishes, so a half-updated version fails the run. Run `cargo build` afterward so `Cargo.lock` updates.

### 3. CHANGELOG

Move the `## [Unreleased]` entries into a new `## [X.Y.Z]` section (compose fresh if empty). The heading must match the
tag **exactly**: tag `v0.2.0` → heading `## [0.2.0]`. The `publish-crates` job greps for that section with an `awk`
match and **fails the publish if it is missing**; cargo-dist derives the GitHub Release body from `CHANGELOG.md` in the
`host` job.

### 4. Public-API baselines (only if the public API changed intentionally)

The `verify` job diffs `cargo public-api --simplified -p <crate>` against the committed baseline for each of the nine
crates and fails on any drift. If you changed a public surface on purpose, regenerate and commit the baselines **in the
same commit** as the version + CHANGELOG:

```sh
scripts/update-api-review.sh          # rewrites docs/api-review/<crate>-public.txt
```

Review the diff before committing — an unintended public-API change caught here is far cheaper than after a publish.

### 5. Generated docs in sync

If the change touched rules, the CLI surface, or config schema, the generators must be re-run and the output committed,
or the `--check` gates in step 1 (and CI) fail:

```sh
cargo xtask doc-rules        # docs/src/rules/*.md, SUMMARY.md, index.md
cargo xtask doc-cli          # docs/src/reference/cli.md
cargo xtask doc-config       # docs/src/reference/configuration.md
```

### 6. Dry run

Rehearse the entire pipeline without uploading anything: run the **Release** workflow via `workflow_dispatch` with
`dry_run: true` (Actions → Release → Run workflow). It runs `verify`, builds the cargo-dist artifacts, checks package
contents, simulates docs.rs from the packaged tarballs, runs `cargo publish --workspace --dry-run` (full packaging +
verify build, no upload), and creates no GitHub Release. Do this once the release commit is on a branch.

```sh
gh workflow run release.yml -f dry_run=true
gh run watch
```

### 7. PR, merge, then tag — invariants gate

Open a PR with the version + CHANGELOG + (if any) baseline/doc changes; merge after `ci.yml` is green. The workflow
publishes from the **tagged commit**, so `release.yml`, the version, and the `## [X.Y.Z]` CHANGELOG section must already
be in it. Before tagging, re-verify the match-exactly invariants on the merge commit:

- `git rev-parse --abbrev-ref HEAD` is `main` and up to date with origin.
- `[workspace.package].version` and all nine `[workspace.dependencies]` `mdwright*` versions equal the intended `X.Y.Z`.
- `CHANGELOG.md` has a `## [X.Y.Z]` heading.
- `git status` is clean and the public-API baselines / generated docs are committed.

**Confirm with the human, then push the tag** (this is the irreversible step):

```sh
git tag -s vX.Y.Z -m "mdwright vX.Y.Z"     # -s signed (preferred), or -a unsigned annotated
git push origin vX.Y.Z
gh run watch
```

A tag with a `-` suffix (e.g. `v0.2.0-rc.1`) is matched by the workflow and cargo-dist auto-marks the GitHub Release a
prerelease.

### 8. Post-publish

- `cargo search mdwright` — all nine crates show the new version (`cargo publish --workspace` waits for index
  propagation per crate, so let the run finish).
- Within ~10 min, confirm `https://docs.rs/mdwright/X.Y.Z` built (a docs.rs failure is recoverable only by a patch
  publish with the fix).
- Confirm the GitHub Release exists with the cargo-dist binary artifacts and installer attached, and that the
  `trigger-pages` job deployed the docs site. These run decoupled from crates.io, so they can complete even if the
  `publish-crates` job failed — check `publish-crates` separately.
- Add a fresh `## [Unreleased]` heading to the top of `CHANGELOG.md`.

## When publish fails mid-run

crates.io versions are immutable, so the fix depends on *why* it failed.

**Partial publish** (the common case) — some crates uploaded, the rest did not (an index-propagation race). The crate
*contents* are fine; only the upload is incomplete. Do **not** bump the version and do **not** re-tag. Do **not** re-run
the **Release** workflow: `cargo publish --workspace` is all-or-nothing on re-run and aborts on the first crate already
on the index before reaching the missing ones. Instead run the **Release recovery** workflow
(`.github/workflows/release-recover.yml`) via `workflow_dispatch` with `version` set to the workspace version (no
leading `v`): it skips the crates already on crates.io and uploads only the missing ones, one at a time, so it is
idempotent and completes the release without burning a version. The binary GitHub Release already exists (decoupled), so
recovery only fills the crates.io gap. Rehearse with `dry_run: true` first if unsure.

**Contents must change** — a genuine build break, not a propagation race. Bump the patch version, repeat steps 2–7, and
re-tag at the new merge commit; the already-published crates keep their old version.

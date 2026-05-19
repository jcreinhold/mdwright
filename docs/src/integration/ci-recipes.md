# CI recipes

Snippets for CI providers other than GitHub Actions. All assume mdwright is on `$PATH`.

## GitLab CI

```yaml,no-check
mdwright:
  image: rust:1.91
  cache:
    paths:
      - .cargo/
  script:
    - cargo install --root .cargo mdwright --locked
    - ./.cargo/bin/mdwright check --check .
    - ./.cargo/bin/mdwright fmt-check .
  rules:
    - changes:
        - "**/*.md"
```

## CircleCI

```yaml,no-check
version: 2.1
jobs:
  mdwright:
    docker:
      - image: cimg/rust:1.91
    steps:
      - checkout
      - run: cargo install mdwright --locked
      - run: mdwright check --check .
      - run: mdwright fmt-check .
workflows:
  docs:
    jobs:
      - mdwright
```

## Buildkite

```yaml,no-check
steps:
  - label: ":memo: mdwright"
    command: |
      cargo install mdwright --locked
      mdwright check --check .
      mdwright fmt-check .
    plugins:
      - docker#v5.10.0:
          image: rust:1.91
```

## Drone

```yaml,no-check
kind: pipeline
name: mdwright

steps:
  - name: mdwright
    image: rust:1.91
    commands:
      - cargo install mdwright --locked
      - mdwright check --check .
      - mdwright fmt-check .
```

## Bare-metal / cron

A nightly job that lints a docs corpus and posts a report:

```sh
#!/usr/bin/env bash
set -euo pipefail
cd "$DOCS_REPO"
git pull --quiet
mdwright check --format=json . > /tmp/mdwright-report.jsonl
jq -s 'length' /tmp/mdwright-report.jsonl | xargs -I {} \
  echo "mdwright: {} diagnostics in $DOCS_REPO"
```

The JSON v2 schema is stable; consume it programmatically (see [Diagnostic schema](../reference/diagnostic-schema.md)).

## See also

- [GitHub Actions](github-actions.md): the most-tested path.
- [Pre-commit](pre-commit.md): local-machine equivalent.

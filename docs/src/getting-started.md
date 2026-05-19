# Getting started

This walkthrough takes ten minutes. By the end you will have linted a Markdown file, fixed a diagnostic, reformatted the
file, and configured one rule.

## Set up

Create a directory with one Markdown file:

```sh
mkdir mdwright-demo && cd mdwright-demo
```

Save the following as `README.md`:

```markdown
# Demo

See https://example.com for the spec.

The Euler identity, $e^{i\pi} + 1 = 0$, is famous.

Here is some code:
```

## Lint

```sh
mdwright check README.md
```

You see two diagnostics:

```text
error[bare-url]: bare URL should be wrapped in angle brackets or rendered as a link
  --> README.md:3:5
   |
 3 | See https://example.com for the spec.
   |     ^^^^^^^^^^^^^^^^^^^
   = help: CommonMark autolinks need angle brackets—`<https://example.com>`—to render as a link.
   = fix (safe): <https://example.com>
   = note: see `mdwright explain bare-url`

error[unbalanced-backtick]: unterminated fenced code block
  --> README.md:9:1
...
```

Read the long-form explanation of the first rule:

```sh
mdwright explain bare-url
```

The bottom line is the documentation URL—open it for the same content rendered with examples.

## Fix the easy one

`bare-url` carries a safe fix. Apply it:

```sh
mdwright fix README.md
```

Re-run `mdwright check`; the bare-URL diagnostic is gone. The unbalanced-backtick diagnostic remains because closing a
fence cannot be inferred safely.

## Fix the hard one by hand

Add the closing fence to `README.md`:

````markdown
Here is some code:

```sh
echo hello
```
````

Re-run `mdwright check`. Output is empty: the file is clean.

## Reformat

```sh
mdwright fmt README.md
```

`fmt` rewrites the file in place. Run `git diff` (in a real project) to see what changed. The defaults preserve source
style—emphasis delimiters, list markers, thematic breaks, line wrap—so the diff is usually small. Display math,
inline math, and fenced code blocks pass through verbatim. Opt in to canonicalisation per knob in `.mdwright.toml`; see
[Formatter policy](format/policy.md) and [Style knobs](format/style.md).

## Configure one rule

mdwright reads configuration from the nearest `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` with a
`[tool.mdwright]` table, walking up from `$PWD` until it hits a `.git/` directory. Create `.mdwright.toml`:

```toml
[lint]
rules = "default,-bare-url"
```

Now `mdwright check` does not flag bare URLs. See [Configuration](configuration.md) for the complete schema.

## Where to go next

- [Lint vs. format](concepts/lint-vs-format.md)—when each subcommand fires.
- [Math regions](concepts/math-regions.md)—what mdwright protects and why.
- [Integration → Pre-commit](integration/pre-commit.md)—wire mdwright into your VCS hooks.
- [Rules catalogue](rules/index.md)—every rule with rationale and examples.

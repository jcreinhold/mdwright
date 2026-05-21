# CLI reference

Auto-generated from clap's `--help` output by `cargo xtask doc-cli`. Edit the CLI definition in
`crates/mdwright/src/cli.rs` (or the rule registry for `list-rules`); never edit this file by hand.

## `mdwright`

```text
Lints Markdown for stylistic and structural issues, with a public rule trait so projects can extend the standard library, plus a verified round-trip formatter.

Usage: mdwright [OPTIONS] <COMMAND>

Commands:
  check       Lint Markdown files and report diagnostics
  fix         Lint and apply safe autofixes in place
  fmt         Reformat Markdown files
  fmt-check   Verify formatting without writing
  list-rules  Print the rule catalogue
  explain     Print the long-form explanation of one lint rule
  render      Format the input and emit the rendered HTML to stdout
  preview     Format the input and render a static terminal Markdown preview
  lsp         Run as a Language Server Protocol server over stdio
  help        Print this message or the help of the given subcommand(s)

Options:
      --config <CONFIG>
          Explicit path to a config file. When omitted, mdwright walks up from `$PWD` looking, at each ancestor, for `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` containing a `[tool.mdwright]` table (in that precedence). The walk stops at the filesystem root or the first directory containing `.git/` (the workspace boundary). If nothing matches, built-in defaults apply

  -v, --verbose...
          Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. `RUST_LOG` overrides this when set

      --max-input-bytes <BYTES>
          Refuse to read any single file (or stdin payload) larger than this many bytes. mdwright treats its input as untrusted; this cap bounds memory use against pathological inputs. Default 10 MB is generous enough that no real Markdown document trips it. Pass `0` to disable the cap entirely

          [default: 10000000]

      --reject-control-chars
          Refuse files (or stdin payloads) that contain C0 control bytes other than TAB, LF, FF, and CR. `CommonMark` accepts these verbatim (it only substitutes NUL with U+FFFD), but their presence is usually evidence the input is not Markdown, and pulldown's silent NUL rewrite makes round-trip idempotence undefined on such inputs. Off by default; opt-in for callers (CI gates, docs pipelines) that prefer hard rejection

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```

## `mdwright check`

```text
Lint Markdown files and report diagnostics

Usage: mdwright check [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...
          Files and directories to scan. Directories are searched recursively. If omitted, `.` is scanned. A literal `-` reads stdin as `<stdin>`

Options:
      --check
          Exit with status 1 if any non-advisory diagnostic is found

      --config <CONFIG>
          Explicit path to a config file. When omitted, mdwright walks up from `$PWD` looking, at each ancestor, for `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` containing a `[tool.mdwright]` table (in that precedence). The walk stops at the filesystem root or the first directory containing `.git/` (the workspace boundary). If nothing matches, built-in defaults apply

      --rules <RULES>
          Rule-selection spec. If omitted, the `[lint] rules` value from the config file applies (or the curated default set if no config is found). See module docs for syntax

  -v, --verbose...
          Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. `RUST_LOG` overrides this when set

      --format <FORMAT>
          Output format

          Possible values:
          - pretty:  Human-readable, optionally coloured
          - compact: `file:line:col: rule: message` per line
          - json:    JSON Lines, v2 schema. See `docs/src/reference/diagnostic-schema.md`
          - json-v1: JSON Lines, v1 schema. Deprecated; emits a deprecation warning on stderr. Will be removed in a future release

          [default: pretty]

      --max-input-bytes <BYTES>
          Refuse to read any single file (or stdin payload) larger than this many bytes. mdwright treats its input as untrusted; this cap bounds memory use against pathological inputs. Default 10 MB is generous enough that no real Markdown document trips it. Pass `0` to disable the cap entirely

          [default: 10000000]

      --color <COLOR>
          When to colour pretty output. `auto` (default) colours when stdout is a TTY; `always` forces colour; `never` disables it. Compact and JSON output are never coloured regardless

          [default: auto]
          [possible values: auto, always, never]

      --reject-control-chars
          Refuse files (or stdin payloads) that contain C0 control bytes other than TAB, LF, FF, and CR. `CommonMark` accepts these verbatim (it only substitutes NUL with U+FFFD), but their presence is usually evidence the input is not Markdown, and pulldown's silent NUL rewrite makes round-trip idempotence undefined on such inputs. Off by default; opt-in for callers (CI gates, docs pipelines) that prefer hard rejection

  -j, --jobs <JOBS>
          Worker threads; 0 = rayon default (one per logical CPU)

          [default: 0]

      --no-suppress
          Ignore `<!-- mdwright: allow ... -->` suppression comments. Use to audit which diagnostics are silenced and where

  -h, --help
          Print help (see a summary with '-h')
```

## `mdwright fix`

```text
Lint and apply safe autofixes in place

Usage: mdwright fix [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...
          Files and directories to scan. Directories are searched recursively. If omitted, `.` is scanned. A literal `-` reads stdin as `<stdin>`

Options:
      --check
          Exit with status 1 if any non-advisory diagnostic is found

      --config <CONFIG>
          Explicit path to a config file. When omitted, mdwright walks up from `$PWD` looking, at each ancestor, for `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` containing a `[tool.mdwright]` table (in that precedence). The walk stops at the filesystem root or the first directory containing `.git/` (the workspace boundary). If nothing matches, built-in defaults apply

      --rules <RULES>
          Rule-selection spec. If omitted, the `[lint] rules` value from the config file applies (or the curated default set if no config is found). See module docs for syntax

  -v, --verbose...
          Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. `RUST_LOG` overrides this when set

      --format <FORMAT>
          Output format

          Possible values:
          - pretty:  Human-readable, optionally coloured
          - compact: `file:line:col: rule: message` per line
          - json:    JSON Lines, v2 schema. See `docs/src/reference/diagnostic-schema.md`
          - json-v1: JSON Lines, v1 schema. Deprecated; emits a deprecation warning on stderr. Will be removed in a future release

          [default: pretty]

      --max-input-bytes <BYTES>
          Refuse to read any single file (or stdin payload) larger than this many bytes. mdwright treats its input as untrusted; this cap bounds memory use against pathological inputs. Default 10 MB is generous enough that no real Markdown document trips it. Pass `0` to disable the cap entirely

          [default: 10000000]

      --color <COLOR>
          When to colour pretty output. `auto` (default) colours when stdout is a TTY; `always` forces colour; `never` disables it. Compact and JSON output are never coloured regardless

          [default: auto]
          [possible values: auto, always, never]

      --reject-control-chars
          Refuse files (or stdin payloads) that contain C0 control bytes other than TAB, LF, FF, and CR. `CommonMark` accepts these verbatim (it only substitutes NUL with U+FFFD), but their presence is usually evidence the input is not Markdown, and pulldown's silent NUL rewrite makes round-trip idempotence undefined on such inputs. Off by default; opt-in for callers (CI gates, docs pipelines) that prefer hard rejection

  -j, --jobs <JOBS>
          Worker threads; 0 = rayon default (one per logical CPU)

          [default: 0]

      --no-suppress
          Ignore `<!-- mdwright: allow ... -->` suppression comments. Use to audit which diagnostics are silenced and where

  -h, --help
          Print help (see a summary with '-h')
```

## `mdwright fmt`

```text
Reformat Markdown files

Usage: mdwright fmt [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...
          Files and directories to reformat. If omitted, `.` is used. A literal `-` reads from stdin and writes to stdout

Options:
      --check
          Exit 1 if any file would change; never write. Same shape as `prettier --check` / `rustfmt --check`

      --config <CONFIG>
          Explicit path to a config file. When omitted, mdwright walks up from `$PWD` looking, at each ancestor, for `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` containing a `[tool.mdwright]` table (in that precedence). The walk stops at the filesystem root or the first directory containing `.git/` (the workspace boundary). If nothing matches, built-in defaults apply

      --diff
          Write a unified diff to stdout instead of editing files. Mutually exclusive with `--check`

  -v, --verbose...
          Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. `RUST_LOG` overrides this when set

      --max-input-bytes <BYTES>
          Refuse to read any single file (or stdin payload) larger than this many bytes. mdwright treats its input as untrusted; this cap bounds memory use against pathological inputs. Default 10 MB is generous enough that no real Markdown document trips it. Pass `0` to disable the cap entirely

          [default: 10000000]

      --stdin-filename <STDIN_FILENAME>
          File name to report when reading from stdin. Defaults to `<stdin>`. Useful when integrating with editors that pipe the buffer through

      --no-validate
          Skip the HTML-equivalence safety check that runs by default. The check parses both source and formatted output to HTML and refuses to write when they differ. Use this only if you have independent verification that the formatter is safe for the input, for example, a CI pipeline that already runs the check elsewhere

      --reject-control-chars
          Refuse files (or stdin payloads) that contain C0 control bytes other than TAB, LF, FF, and CR. `CommonMark` accepts these verbatim (it only substitutes NUL with U+FFFD), but their presence is usually evidence the input is not Markdown, and pulldown's silent NUL rewrite makes round-trip idempotence undefined on such inputs. Off by default; opt-in for callers (CI gates, docs pipelines) that prefer hard rejection

      --explain-divergence
          When the HTML-equivalence gate rejects a file, print a unified diff of the source's HTML against the formatted output's HTML to stderr. Diagnostic surface for triaging gate failures; does not change the gate's pass/fail decision

      --explain-format
          Explain formatter decisions on stderr. Does not change write, check, diff, or validation behavior

      --range <LINE:COL-LINE:COL>
          Format only the smallest set of whole top-level blocks covering `LINE:COL-LINE:COL` (both ends inclusive of start, exclusive of end; 0-based LSP convention). Reads from stdin only; writes the covering blocks to stdout. Mutually exclusive with `--check` and `--diff`.

          Example: `--range 2:0-2:5` formats the block containing columns 0..5 of line 2.

      --math-render <MATH_RENDER>
          Delimiter rewrite policy for math regions at emit time. `none` (default) passes math through verbatim: today's behaviour. `commonmark-katex` is the same emission as `none` but greppable as an intent signal in build logs. `dollar` rewrites `\[…\]` to `$$ … $$` and `\(…\)` to `$ … $` for downstream renderers that prefer dollar delimiters; LaTeX environments are not rewritten. Overrides `[fmt.math] render` in the config file

          [possible values: none, commonmark-katex, dollar]

  -h, --help
          Print help (see a summary with '-h')
```

## `mdwright fmt-check`

```text
Verify formatting without writing

Usage: mdwright fmt-check [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...  Files and directories to check. If omitted, `.` is used. A literal `-` reads stdin and checks whether it would change

Options:
      --config <CONFIG>
          Explicit path to a config file. When omitted, mdwright walks up from `$PWD` looking, at each ancestor, for `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` containing a `[tool.mdwright]` table (in that precedence). The walk stops at the filesystem root or the first directory containing `.git/` (the workspace boundary). If nothing matches, built-in defaults apply
      --diff
          Write a unified diff to stdout for files that would change
      --stdin-filename <STDIN_FILENAME>
          File name to report when reading from stdin. Defaults to `<stdin>`. Useful when integrating with editors that pipe the buffer through
  -v, --verbose...
          Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. `RUST_LOG` overrides this when set
      --max-input-bytes <BYTES>
          Refuse to read any single file (or stdin payload) larger than this many bytes. mdwright treats its input as untrusted; this cap bounds memory use against pathological inputs. Default 10 MB is generous enough that no real Markdown document trips it. Pass `0` to disable the cap entirely [default: 10000000]
      --no-validate
          Skip the HTML-equivalence safety check that runs by default. The check parses both source and formatted output to HTML and refuses to write when they differ. Use this only if you have independent verification that the formatter is safe for the input, for example, a CI pipeline that already runs the check elsewhere
      --explain-divergence
          When the HTML-equivalence gate rejects a file, print a unified diff of the source's HTML against the formatted output's HTML to stderr. Diagnostic surface for triaging gate failures; does not change the gate's pass/fail decision
      --reject-control-chars
          Refuse files (or stdin payloads) that contain C0 control bytes other than TAB, LF, FF, and CR. `CommonMark` accepts these verbatim (it only substitutes NUL with U+FFFD), but their presence is usually evidence the input is not Markdown, and pulldown's silent NUL rewrite makes round-trip idempotence undefined on such inputs. Off by default; opt-in for callers (CI gates, docs pipelines) that prefer hard rejection
      --explain-format
          Explain formatter decisions on stderr. Does not change check, diff, or validation behavior
      --math-render <MATH_RENDER>
          Delimiter rewrite policy for math regions at emit time. Overrides `[fmt.math] render` in the config file [possible values: none, commonmark-katex, dollar]
  -h, --help
          Print help
```

## `mdwright render`

```text
Format the input and emit the rendered HTML to stdout.

Pipes the formatted output through the same HTML renderer the `format_validated` gate uses. Captured stdout is raw HTML by default; terminals may request ANSI-highlighted HTML with `--color`, and `--open` writes the HTML to a temporary file before opening it in the system browser.

Usage: mdwright render [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...
          File to render. A literal `-` (or an empty list) reads from stdin. Multiple paths are concatenated in argument order with a single newline between, then rendered as one document

Options:
      --config <CONFIG>
          Explicit path to a config file. When omitted, mdwright walks up from `$PWD` looking, at each ancestor, for `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` containing a `[tool.mdwright]` table (in that precedence). The walk stops at the filesystem root or the first directory containing `.git/` (the workspace boundary). If nothing matches, built-in defaults apply

      --stdin-filename <STDIN_FILENAME>
          File name to report when reading from stdin. Defaults to `<stdin>`. Cosmetic; surfaced in error messages only

      --math-render <MATH_RENDER>
          Delimiter rewrite policy for math regions. See the corresponding flag on `mdwright fmt` for the modes

          [possible values: none, commonmark-katex, dollar]

  -v, --verbose...
          Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. `RUST_LOG` overrides this when set

      --max-input-bytes <BYTES>
          Refuse to read any single file (or stdin payload) larger than this many bytes. mdwright treats its input as untrusted; this cap bounds memory use against pathological inputs. Default 10 MB is generous enough that no real Markdown document trips it. Pass `0` to disable the cap entirely

          [default: 10000000]

      --render-profile <RENDER_PROFILE>
          HTML spelling profile. `pulldown` preserves the default renderer; `cmark-gfm` matches cmark-gfm spelling for renderer differences that do not require changing parser semantics. Overrides `[render] profile` in the config file

          [possible values: pulldown, cmark-gfm]

      --color <COLOR>
          When to colour HTML output. Captured stdout remains raw HTML under `auto`; `always` forces ANSI syntax highlighting and `never` disables it

          [default: auto]
          [possible values: auto, always, never]

      --reject-control-chars
          Refuse files (or stdin payloads) that contain C0 control bytes other than TAB, LF, FF, and CR. `CommonMark` accepts these verbatim (it only substitutes NUL with U+FFFD), but their presence is usually evidence the input is not Markdown, and pulldown's silent NUL rewrite makes round-trip idempotence undefined on such inputs. Off by default; opt-in for callers (CI gates, docs pipelines) that prefer hard rejection

      --open
          Write rendered HTML to a temporary `.html` file and open it in the system browser. Stdout is left empty; stderr reports the file path

  -h, --help
          Print help (see a summary with '-h')
```

## `mdwright preview`

```text
Format the input and render a static terminal Markdown preview

Usage: mdwright preview [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...
          Files to preview. A literal `-` (or an empty list) reads from stdin. Multiple paths are concatenated in argument order with a single newline between, then previewed as one document

Options:
      --config <CONFIG>
          Explicit path to a config file. When omitted, mdwright walks up from `$PWD` looking, at each ancestor, for `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` containing a `[tool.mdwright]` table (in that precedence). The walk stops at the filesystem root or the first directory containing `.git/` (the workspace boundary). If nothing matches, built-in defaults apply

      --stdin-filename <STDIN_FILENAME>
          File name to report when reading from stdin. Defaults to `<stdin>`. Cosmetic; surfaced in error messages only

      --color <COLOR>
          When to colour terminal output. `auto` colours when stdout is a TTY; `always` forces colour; `never` disables it

          [default: auto]
          [possible values: auto, always, never]

  -v, --verbose...
          Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. `RUST_LOG` overrides this when set

      --math <MATH>
          How terminal preview handles math regions

          Possible values:
          - unicode: Render the conservative supported LaTeX subset as Unicode, falling back to source when unsupported
          - source:  Preserve math source bytes
          - off:     Disable special terminal math rendering

          [default: unicode]

      --max-input-bytes <BYTES>
          Refuse to read any single file (or stdin payload) larger than this many bytes. mdwright treats its input as untrusted; this cap bounds memory use against pathological inputs. Default 10 MB is generous enough that no real Markdown document trips it. Pass `0` to disable the cap entirely

          [default: 10000000]

      --reject-control-chars
          Refuse files (or stdin payloads) that contain C0 control bytes other than TAB, LF, FF, and CR. `CommonMark` accepts these verbatim (it only substitutes NUL with U+FFFD), but their presence is usually evidence the input is not Markdown, and pulldown's silent NUL rewrite makes round-trip idempotence undefined on such inputs. Off by default; opt-in for callers (CI gates, docs pipelines) that prefer hard rejection

  -h, --help
          Print help (see a summary with '-h')
```

## `mdwright list-rules`

```text
Print the rule catalogue

Usage: mdwright list-rules [OPTIONS]

Options:
      --config <CONFIG>          Explicit path to a config file. When omitted, mdwright walks up from `$PWD` looking, at each ancestor, for `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` containing a `[tool.mdwright]` table (in that precedence). The walk stops at the filesystem root or the first directory containing `.git/` (the workspace boundary). If nothing matches, built-in defaults apply
  -v, --verbose...               Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. `RUST_LOG` overrides this when set
      --max-input-bytes <BYTES>  Refuse to read any single file (or stdin payload) larger than this many bytes. mdwright treats its input as untrusted; this cap bounds memory use against pathological inputs. Default 10 MB is generous enough that no real Markdown document trips it. Pass `0` to disable the cap entirely [default: 10000000]
      --reject-control-chars     Refuse files (or stdin payloads) that contain C0 control bytes other than TAB, LF, FF, and CR. `CommonMark` accepts these verbatim (it only substitutes NUL with U+FFFD), but their presence is usually evidence the input is not Markdown, and pulldown's silent NUL rewrite makes round-trip idempotence undefined on such inputs. Off by default; opt-in for callers (CI gates, docs pipelines) that prefer hard rejection
  -h, --help                     Print help
```

## `mdwright explain`

```text
Print the long-form explanation of one lint rule

Usage: mdwright explain [OPTIONS] <RULE>

Arguments:
  <RULE>  Kebab-case rule name (e.g. `bare-url`, `math/unbalanced-delim`)

Options:
      --config <CONFIG>          Explicit path to a config file. When omitted, mdwright walks up from `$PWD` looking, at each ancestor, for `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` containing a `[tool.mdwright]` table (in that precedence). The walk stops at the filesystem root or the first directory containing `.git/` (the workspace boundary). If nothing matches, built-in defaults apply
  -v, --verbose...               Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. `RUST_LOG` overrides this when set
      --max-input-bytes <BYTES>  Refuse to read any single file (or stdin payload) larger than this many bytes. mdwright treats its input as untrusted; this cap bounds memory use against pathological inputs. Default 10 MB is generous enough that no real Markdown document trips it. Pass `0` to disable the cap entirely [default: 10000000]
      --reject-control-chars     Refuse files (or stdin payloads) that contain C0 control bytes other than TAB, LF, FF, and CR. `CommonMark` accepts these verbatim (it only substitutes NUL with U+FFFD), but their presence is usually evidence the input is not Markdown, and pulldown's silent NUL rewrite makes round-trip idempotence undefined on such inputs. Off by default; opt-in for callers (CI gates, docs pipelines) that prefer hard rejection
  -h, --help                     Print help
```

## `mdwright lsp`

```text
Run as a Language Server Protocol server over stdio

Usage: mdwright lsp [OPTIONS]

Options:
      --config <CONFIG>          Explicit path to a config file. When omitted, mdwright walks up from `$PWD` looking, at each ancestor, for `.mdwright.toml`, `mdwright.toml`, or `pyproject.toml` containing a `[tool.mdwright]` table (in that precedence). The walk stops at the filesystem root or the first directory containing `.git/` (the workspace boundary). If nothing matches, built-in defaults apply
  -v, --verbose...               Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace. `RUST_LOG` overrides this when set
      --max-input-bytes <BYTES>  Refuse to read any single file (or stdin payload) larger than this many bytes. mdwright treats its input as untrusted; this cap bounds memory use against pathological inputs. Default 10 MB is generous enough that no real Markdown document trips it. Pass `0` to disable the cap entirely [default: 10000000]
      --reject-control-chars     Refuse files (or stdin payloads) that contain C0 control bytes other than TAB, LF, FF, and CR. `CommonMark` accepts these verbatim (it only substitutes NUL with U+FFFD), but their presence is usually evidence the input is not Markdown, and pulldown's silent NUL rewrite makes round-trip idempotence undefined on such inputs. Off by default; opt-in for callers (CI gates, docs pipelines) that prefer hard rejection
  -h, --help                     Print help
```

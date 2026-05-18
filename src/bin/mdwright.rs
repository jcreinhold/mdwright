//! `mdwright` — math-resilient Markdown linter and formatter.
//! Subcommands:
//!
//! - `check`     — lint files; non-zero exit if any non-advisory diag
//! - `fix`       — apply safe autofixes in place
//! - `fmt`       — reformat Markdown
//! - `fmt-check` — verify formatting without writing
//! - `list-rules` — print the rule catalogue
//!
//! Output formats: `pretty` (default, coloured when tty),
//! `compact` (one diag per line, grep-friendly), `json` (JSON Lines).
//!
//! Rule selection uses `--rules <spec>` where `<spec>` is a
//! comma-separated list of tokens:
//!
//! - `all` — the full standard library, including opt-in rules.
//! - `default` — the curated default-on subset.
//! - `<name>` — start from `{<name>}` (the named rule only).
//! - `+<name>` — additive: add this rule to the working set.
//! - `-<name>` — subtractive: remove this rule from the working set.
//!
//! Examples:
//!
//! ```text
//! mdwright check
//! mdwright check --rules all
//! mdwright check --rules default,-bare-url
//! mdwright check --rules default,+escaped-emphasis,+subscript-damage
//! mdwright check --rules unbalanced-backtick,adjacent-code-no-space
//! ```

use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand, ValueEnum};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use mdwright::{
    CheckpointTable, Config, Diagnostic, Document, FmtOptions, FormatError, FormatMode, LineIndex, LintOptions,
    RuleSet, Severity, Snippet, contains_rejected_control_chars, discover_markdown, format_range_with_checkpoints,
    rule_doc_url, stdlib,
};
use owo_colors::OwoColorize;
use rayon::prelude::*;
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(
    name = "mdwright",
    version,
    about = "Math-resilient Markdown linter and formatter",
    long_about = "Lints Markdown for stylistic and structural issues, with a public \
                  rule trait so projects can extend the standard library. A \
                  round-trip formatter follows in a later phase."
)]
struct Cli {
    /// Explicit path to a config file. When omitted, mdwright walks
    /// up from `$PWD` looking, at each ancestor, for `.mdwright.toml`,
    /// `mdwright.toml`, or `pyproject.toml` containing a
    /// `[tool.mdwright]` table (in that precedence). The walk stops at
    /// the filesystem root or the first directory containing `.git/`
    /// (the workspace boundary). If nothing matches, built-in defaults
    /// apply.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace.
    /// `RUST_LOG` overrides this when set.
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Refuse to read any single file (or stdin payload) larger than
    /// this many bytes. mdwright treats its input as untrusted; this
    /// cap bounds memory use against pathological inputs. Default
    /// 10 MB is generous enough that no real Markdown document trips
    /// it. Pass `0` to disable the cap entirely.
    #[arg(long, value_name = "BYTES", default_value_t = 10_000_000, global = true)]
    max_input_bytes: usize,

    /// Refuse files (or stdin payloads) that contain C0 control bytes
    /// other than TAB, LF, FF, and CR. `CommonMark` accepts these
    /// verbatim (it only substitutes NUL with U+FFFD), but their
    /// presence is usually evidence the input is not Markdown — and
    /// pulldown's silent NUL rewrite makes round-trip idempotence
    /// undefined on such inputs. Off by default; opt-in for callers
    /// (CI gates, docs pipelines) that prefer hard rejection.
    #[arg(long, global = true)]
    reject_control_chars: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Lint Markdown files and report diagnostics.
    Check(LintArgs),
    /// Lint and apply safe autofixes in place.
    Fix(LintArgs),
    /// Reformat Markdown files.
    Fmt(FmtArgs),
    /// Verify formatting without writing.
    FmtCheck(FmtArgs),
    /// Print the rule catalogue.
    ListRules,
    /// Print the long-form explanation of one lint rule.
    Explain {
        /// Kebab-case rule name (e.g. `bare-url`, `math/unbalanced-delim`).
        rule: String,
    },
    /// Run as a Language Server Protocol server over stdio.
    Lsp,
}

#[derive(Args, Debug)]
struct LintArgs {
    /// Files and directories to scan. Directories are searched
    /// recursively; if empty, stdin is read (path reported as
    /// `<stdin>`).
    paths: Vec<PathBuf>,

    /// Exit with status 1 if any non-advisory diagnostic is found.
    #[arg(long)]
    check: bool,

    /// Rule-selection spec. If omitted, the `[lint] rules` value
    /// from the config file applies (or the curated default set if
    /// no config is found). See module docs for syntax.
    #[arg(long)]
    rules: Option<String>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Pretty)]
    format: OutputFormat,

    /// When to colour pretty output. `auto` (default) colours when
    /// stdout is a TTY; `always` forces colour; `never` disables it.
    /// Compact and JSON output are never coloured regardless.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,

    /// Worker threads; 0 = rayon default (one per logical CPU).
    #[arg(short = 'j', long, default_value_t = 0)]
    jobs: usize,

    /// Ignore `<!-- mdwright: allow ... -->` suppression comments.
    /// Use to audit which diagnostics are silenced and where.
    #[arg(long)]
    no_suppress: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum, Default)]
enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Args, Debug)]
#[allow(clippy::struct_excessive_bools)]
struct FmtArgs {
    /// Files and directories to reformat. A literal `-` element
    /// (or an empty list) reads from stdin and writes to stdout.
    paths: Vec<PathBuf>,

    /// Exit 1 if any file would change; never write. Same shape as
    /// `prettier --check` / `rustfmt --check`.
    #[arg(long)]
    check: bool,

    /// Write a unified diff to stdout instead of editing files.
    /// Mutually exclusive with `--check`.
    #[arg(long, conflicts_with = "check")]
    diff: bool,

    /// File name to report when reading from stdin. Defaults to
    /// `<stdin>`. Useful when integrating with editors that pipe
    /// the buffer through.
    #[arg(long)]
    stdin_filename: Option<PathBuf>,

    /// Skip the HTML-equivalence safety check that runs by default.
    /// The check parses both source and formatted output to HTML and
    /// refuses to write when they differ. Use this only if you have
    /// independent verification that the formatter is safe for the
    /// input — for example, a CI pipeline that already runs the
    /// check elsewhere.
    #[arg(long)]
    no_validate: bool,

    /// When the HTML-equivalence gate rejects a file, print a unified
    /// diff of the source's HTML against the formatted output's HTML
    /// to stderr. Diagnostic surface for triaging gate failures; does
    /// not change the gate's pass/fail decision.
    #[arg(long)]
    explain_divergence: bool,

    /// Formatter mode. `normalise` (default) applies every enabled
    /// rewrite; `verbatim` emits source bytes 1-to-1.
    #[arg(long, value_enum, default_value_t = ModeArg::Normalise)]
    mode: ModeArg,

    /// Format only the smallest set of whole top-level blocks covering
    /// `LINE:COL-LINE:COL` (both ends inclusive of start, exclusive of
    /// end; 0-based LSP convention). Reads from stdin only; writes the
    /// covering blocks to stdout. Mutually exclusive with `--check`
    /// and `--diff`.
    ///
    /// Example: `--range 2:0-2:5` formats the block containing
    /// columns 0..5 of line 2.
    #[arg(long, value_name = "LINE:COL-LINE:COL", conflicts_with_all = ["check", "diff"])]
    range: Option<RangeArg>,
}

#[derive(Copy, Clone, Debug)]
struct RangeArg {
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
}

impl std::str::FromStr for RangeArg {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (lhs, rhs) = s
            .split_once('-')
            .ok_or_else(|| format!("expected LINE:COL-LINE:COL, got {s:?}"))?;
        let parse_pair = |p: &str| -> Result<(usize, usize), String> {
            let (l, c) = p
                .split_once(':')
                .ok_or_else(|| format!("expected LINE:COL, got {p:?}"))?;
            let line = l.parse::<usize>().map_err(|e| format!("bad line {l:?}: {e}"))?;
            let col = c.parse::<usize>().map_err(|e| format!("bad column {c:?}: {e}"))?;
            Ok((line, col))
        };
        let (sl, sc) = parse_pair(lhs)?;
        let (el, ec) = parse_pair(rhs)?;
        Ok(Self {
            start_line: sl,
            start_col: sc,
            end_line: el,
            end_col: ec,
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum ModeArg {
    Normalise,
    Verbatim,
}

impl From<ModeArg> for FormatMode {
    fn from(m: ModeArg) -> Self {
        match m {
            ModeArg::Normalise => Self::Normalise,
            ModeArg::Verbatim => Self::Verbatim,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    /// Human-readable, optionally coloured.
    Pretty,
    /// `file:line:col: rule: message` per line.
    Compact,
    /// JSON Lines, v2 schema. See `docs/src/reference/diagnostic-schema.md`.
    Json,
    /// JSON Lines, v1 schema. Deprecated; emits a deprecation
    /// warning on stderr. Will be removed in a future release.
    #[value(name = "json-v1")]
    JsonV1,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(err) => {
            let mut stderr = io::stderr().lock();
            let _write = writeln!(stderr, "mdwright: error: {err:#}");
            ExitCode::from(2)
        }
    }
}

/// Bundle of input-boundary policy flags propagated to every entry
/// point that reads source bytes (file or stdin). Keeping the two
/// knobs together prevents a future input-boundary flag from
/// reshuffling every signature.
#[derive(Copy, Clone, Debug)]
struct InputPolicy {
    /// Hard byte cap; `0` disables.
    max_bytes: usize,
    /// Reject inputs with C0 controls other than TAB/LF/FF/CR.
    reject_controls: bool,
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    let config_path = cli.config;
    let policy = InputPolicy {
        max_bytes: cli.max_input_bytes,
        reject_controls: cli.reject_control_chars,
    };
    match cli.command {
        Command::ListRules => {
            print_rule_catalogue()?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Explain { rule } => run_explain(&rule),
        Command::Check(args) => run_lint(&args, false, config_path.as_deref(), policy),
        Command::Fix(args) => run_lint(&args, true, config_path.as_deref(), policy),
        Command::Fmt(args) => run_fmt(&args, false, config_path.as_deref(), policy),
        Command::FmtCheck(mut args) => {
            args.check = true;
            run_fmt(&args, true, config_path.as_deref(), policy)
        }
        Command::Lsp => run_lsp(),
    }
}

/// Hand off to the LSP server, blocking until the client sends `exit`
/// or the transport closes. A multi-threaded tokio runtime is
/// constructed here so the rest of the binary stays sync.
fn run_lsp() -> Result<ExitCode> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build LSP tokio runtime")?;
    runtime.block_on(mdwright::lsp::serve());
    Ok(ExitCode::SUCCESS)
}

/// Print the long-form explanation of one stdlib rule. Returns a
/// non-zero exit code with a "did you mean" suggestion when the
/// rule name is unknown.
fn run_explain(name: &str) -> Result<ExitCode> {
    if let Some(rule) = stdlib::by_name(name) {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        let body = rule.explain().trim();
        if body.is_empty() {
            writeln!(
                out,
                "{}: {}\n\nNo long-form explanation registered. Run `mdwright list-rules` for the one-line summary.",
                rule.name(),
                rule.description(),
            )?;
        } else {
            writeln!(out, "{}: {}\n", rule.name(), rule.description())?;
            writeln!(out, "{body}")?;
        }
        writeln!(out, "\nSee: {}", rule_doc_url(rule.name()))?;
        return Ok(ExitCode::SUCCESS);
    }

    let mut stderr = io::stderr().lock();
    writeln!(stderr, "mdwright: error: unknown rule '{name}'")?;
    if let Some(suggestion) = closest_rule_name(name) {
        writeln!(stderr, "  help: did you mean '{suggestion}'?")?;
    }
    writeln!(stderr, "  see `mdwright list-rules` for the full catalogue.")?;
    Ok(ExitCode::from(1))
}

/// Jaro-Winkler similarity over every stdlib rule name. Returns the
/// closest match if its similarity exceeds 0.7, else `None`.
fn closest_rule_name(query: &str) -> Option<&'static str> {
    let mut best: Option<(&'static str, f64)> = None;
    for name in stdlib::names() {
        let score = strsim::jaro_winkler(query, name);
        match best {
            Some((_, b)) if score <= b => {}
            _ => best = Some((name, score)),
        }
    }
    best.and_then(|(name, score)| if score > 0.7 { Some(name) } else { None })
}

/// Return an error if `len` exceeds the configured cap. `cap == 0`
/// means "no cap" — matches the `--max-input-bytes 0` escape hatch.
fn enforce_input_cap(label: &str, len: usize, cap: usize) -> Result<()> {
    if cap > 0 && len > cap {
        bail!(
            "{label}: input is {len} bytes; exceeds --max-input-bytes cap of {cap}. \
             Raise the cap with `--max-input-bytes <BYTES>` (or `0` to disable)."
        );
    }
    Ok(())
}

/// Reject inputs carrying C0 controls other than TAB/LF/FF/CR when
/// the operator opted in. No-op when `reject_controls` is false.
fn enforce_no_rejected_controls(label: &str, source: &str, reject_controls: bool) -> Result<()> {
    if reject_controls && contains_rejected_control_chars(source) {
        bail!(
            "{label}: input contains C0 control bytes outside TAB/LF/FF/CR. \
             Pulldown's NUL→U+FFFD rewrite makes round-trip undefined on \
             such inputs; drop `--reject-control-chars` to accept them."
        );
    }
    Ok(())
}

/// Apply both input-boundary checks in order. Size first (cheap and
/// catches the runaway case before the predicate walks the bytes).
fn enforce_input_policy(label: &str, source: &str, policy: InputPolicy) -> Result<()> {
    enforce_input_cap(label, source.len(), policy.max_bytes)?;
    enforce_no_rejected_controls(label, source, policy.reject_controls)
}

/// Read stdin into `buf` with a hard byte cap. Reads via `take(cap+1)`
/// so we can distinguish "exactly at cap" from "more than cap" without
/// pulling the rest of the stream into memory. Control-char rejection
/// runs after the read so the diagnostic mentions the original input.
// Sequential stdin read; the lock guard is released as soon as the
// I/O chain ends. Suppress the nursery hint that wants the guard
// dropped a statement earlier — doing so would force splitting the
// `take` chain across blocks for no actual contention.
#[allow(clippy::significant_drop_tightening)]
fn read_stdin_capped(buf: &mut String, policy: InputPolicy, label: &str) -> Result<()> {
    use std::io::Read as _;
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let cap = policy.max_bytes;
    if cap == 0 {
        handle.read_to_string(buf).context("read stdin")?;
    } else {
        let limit = u64::try_from(cap).unwrap_or(u64::MAX).saturating_add(1);
        handle.take(limit).read_to_string(buf).context("read stdin")?;
        enforce_input_cap(label, buf.len(), cap)?;
    }
    enforce_no_rejected_controls(label, buf, policy.reject_controls)
}

fn run_fmt(
    args: &FmtArgs,
    force_check: bool,
    config_path: Option<&std::path::Path>,
    policy: InputPolicy,
) -> Result<ExitCode> {
    let cfg = resolve_config(config_path)?;
    let opts = cfg.fmt_options().clone().with_mode(args.mode.into());
    let check = args.check || force_check;

    if let Some(range_arg) = args.range {
        if !(args.paths.is_empty() || args.paths.iter().any(|p| p.as_os_str() == "-")) {
            bail!("--range reads from stdin; pass `-` for paths or omit them");
        }
        return run_fmt_range_stdin(&opts, range_arg, args, policy);
    }

    if args.paths.is_empty() || args.paths.iter().any(|p| p.as_os_str() == "-") {
        return run_fmt_stdin(&opts, args, check, policy);
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for p in &args.paths {
        if !p.exists() {
            bail!("path does not exist: {}", p.display());
        }
        files.extend(discover_markdown(p));
    }
    files.sort();
    files.dedup();
    if !opts.exclude_globs().is_empty() {
        let exclude = build_exclude(opts.exclude_globs(), cfg.source_dir())?;
        files.retain(|path| !exclude.matched(path, false).is_ignore());
    }

    let changed = AtomicUsize::new(0);
    let divergent = AtomicUsize::new(0);
    let stdout_lock = Mutex::new(());
    let stderr_lock = Mutex::new(());
    let validate = !args.no_validate;

    let results: Vec<Result<()>> = files
        .par_iter()
        .map(|path| -> Result<()> {
            let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
            enforce_input_policy(&path.display().to_string(), &source, policy)?;
            let doc = Document::parse(&source);
            let formatted = match (validate, doc.format_validated(&opts)) {
                (true, Ok(s)) => s,
                (true, Err(FormatError::SemanticDivergence { source: src, formatted, diff_summary })) => {
                    divergent.fetch_add(1, Ordering::Relaxed);
                    let guard = stderr_lock.lock().map_err(|_| anyhow!("stderr lock poisoned"))?;
                    let mut err = io::stderr().lock();
                    writeln!(
                        err,
                        "mdwright: refusing to write {}: format changes meaning ({diff_summary}) (rerun with --no-validate to override)",
                        path.display()
                    )?;
                    if args.explain_divergence {
                        write_unified_diff(&mut err, &path.display().to_string(), &src, &formatted)?;
                    }
                    drop(guard);
                    drop(formatted);
                    return Ok(());
                }
                (false, _) => doc.format(&opts),
            };
            if formatted == source {
                return Ok(());
            }
            changed.fetch_add(1, Ordering::Relaxed);
            if args.diff {
                let guard = stdout_lock.lock().map_err(|_| anyhow!("stdout lock poisoned"))?;
                let stdout = io::stdout();
                let mut out = stdout.lock();
                write_unified_diff(&mut out, &path.display().to_string(), &source, &formatted)?;
                drop(guard);
            } else if !check {
                fs::write(path, &formatted).with_context(|| format!("write {}", path.display()))?;
            }
            Ok(())
        })
        .collect();
    for r in results {
        r?;
    }

    let changed = changed.load(Ordering::Relaxed);
    let divergent = divergent.load(Ordering::Relaxed);
    if divergent > 0 {
        Ok(ExitCode::from(2))
    } else if check && changed > 0 {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

fn run_fmt_range_stdin(
    opts: &FmtOptions,
    range_arg: RangeArg,
    args: &FmtArgs,
    policy: InputPolicy,
) -> Result<ExitCode> {
    let name = args
        .stdin_filename
        .as_deref()
        .map_or_else(|| "<stdin>".to_owned(), |p| p.display().to_string());
    let mut buf = String::new();
    read_stdin_capped(&mut buf, policy, &name)?;
    let line_index = LineIndex::new(&buf);
    let lo = line_index
        .byte_of_position_0based(&buf, range_arg.start_line, range_arg.start_col)
        .ok_or_else(|| {
            anyhow!(
                "range start {}:{} is past end of input ({} bytes)",
                range_arg.start_line,
                range_arg.start_col,
                buf.len()
            )
        })?;
    let hi = line_index
        .byte_of_position_0based(&buf, range_arg.end_line, range_arg.end_col)
        .ok_or_else(|| {
            anyhow!(
                "range end {}:{} is past end of input ({} bytes)",
                range_arg.end_line,
                range_arg.end_col,
                buf.len()
            )
        })?;
    if hi < lo {
        bail!("range end ({hi}) precedes range start ({lo})");
    }
    let table = CheckpointTable::build(&buf);
    let formatted = format_range_with_checkpoints(&buf, opts, &table, lo..hi);
    let stdout = io::stdout();
    let mut out = stdout.lock();
    out.write_all(formatted.as_bytes())?;
    Ok(ExitCode::SUCCESS)
}

fn run_fmt_stdin(opts: &FmtOptions, args: &FmtArgs, check: bool, policy: InputPolicy) -> Result<ExitCode> {
    let name = args
        .stdin_filename
        .as_deref()
        .map_or_else(|| "<stdin>".to_owned(), |p| p.display().to_string());
    let mut buf = String::new();
    read_stdin_capped(&mut buf, policy, &name)?;
    let doc = Document::parse(&buf);
    let formatted = if args.no_validate {
        doc.format(opts)
    } else {
        match doc.format_validated(opts) {
            Ok(s) => s,
            Err(FormatError::SemanticDivergence { diff_summary, .. }) => {
                let mut err = io::stderr().lock();
                writeln!(
                    err,
                    "mdwright: refusing to format {name}: format changes meaning ({diff_summary}) (rerun with --no-validate to override)",
                )?;
                return Ok(ExitCode::from(2));
            }
        }
    };
    if check {
        if formatted != buf {
            return Ok(ExitCode::from(1));
        }
        return Ok(ExitCode::SUCCESS);
    }
    let stdout = io::stdout();
    let mut out = stdout.lock();
    if args.diff {
        write_unified_diff(&mut out, &name, &buf, &formatted)?;
    } else {
        out.write_all(formatted.as_bytes())?;
    }
    Ok(ExitCode::SUCCESS)
}

/// Minimal unified diff: emits per-line `-old` / `+new` with no
/// context. The output is enough for code review of a reformat run
/// and avoids a heavyweight crate dependency. (sessions 10+ may
/// promote this to a real Myers diff via `similar` if needed.)
fn write_unified_diff<W: Write>(out: &mut W, path: &str, old: &str, new: &str) -> Result<()> {
    writeln!(out, "--- a/{path}")?;
    writeln!(out, "+++ b/{path}")?;
    let old_lines: Vec<&str> = old.split('\n').collect();
    let new_lines: Vec<&str> = new.split('\n').collect();
    let len = old_lines.len().max(new_lines.len());
    let mut i = 0usize;
    while i < len {
        let o = old_lines.get(i).copied();
        let n = new_lines.get(i).copied();
        if o == n {
            i = i.saturating_add(1);
            continue;
        }
        if let Some(o) = o {
            writeln!(out, "-{o}")?;
        }
        if let Some(n) = n {
            writeln!(out, "+{n}")?;
        }
        i = i.saturating_add(1);
    }
    Ok(())
}

fn run_lint(
    args: &LintArgs,
    apply_fixes: bool,
    config_path: Option<&std::path::Path>,
    policy: InputPolicy,
) -> Result<ExitCode> {
    if args.jobs > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.jobs)
            .build_global()
            .context("configure rayon thread pool")?;
    }

    let cfg = resolve_config(config_path)?;
    let rules_spec = args.rules.as_deref().unwrap_or_else(|| cfg.rules_spec());
    let mut rules = parse_rules_spec(rules_spec)?;
    apply_config_to_rules(&mut rules, &cfg)?;

    let use_color = match args.color {
        ColorChoice::Always => matches!(args.format, OutputFormat::Pretty),
        ColorChoice::Never => false,
        ColorChoice::Auto => matches!(args.format, OutputFormat::Pretty) && io::stdout().is_terminal(),
    };
    let lint_opts = LintOptions {
        respect_suppressions: !args.no_suppress,
    };

    if args.paths.is_empty() {
        return run_stdin(
            &rules,
            lint_opts,
            apply_fixes,
            args.check,
            args.format,
            use_color,
            policy,
        );
    }

    let mut files: Vec<PathBuf> = Vec::new();
    for p in &args.paths {
        if !p.exists() {
            bail!("path does not exist: {}", p.display());
        }
        files.extend(discover_markdown(p));
    }
    files.sort();
    files.dedup();
    if !cfg.exclude_globs().is_empty() {
        let exclude = build_exclude(cfg.exclude_globs(), cfg.source_dir())?;
        files.retain(|path| !exclude.matched(path, false).is_ignore());
    }

    let totals = AtomicUsize::new(0);
    let non_advisory = AtomicUsize::new(0);
    let fixed = AtomicUsize::new(0);
    let stdout_lock = Mutex::new(());

    let results: Vec<Result<()>> = files
        .par_iter()
        .map(|path| -> Result<()> {
            let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
            enforce_input_policy(&path.display().to_string(), &source, policy)?;
            let doc = Document::parse(&source);
            let diags = doc.lint_with(&rules, lint_opts);
            let count = diags.len();
            let non_adv = diags.iter().filter(|d| !d.advisory).count();

            let (final_source, final_diags, applied) = if apply_fixes && !diags.is_empty() {
                let (new_src, n) = doc.apply_safe_fixes(&diags);
                if n > 0 && new_src != source {
                    fs::write(path, &new_src).with_context(|| format!("write {}", path.display()))?;
                }
                let post_doc = Document::parse(&new_src);
                let post_diags = post_doc.lint_with(&rules, lint_opts);
                (new_src, post_diags, n)
            } else {
                (source, diags, 0)
            };

            totals.fetch_add(count, Ordering::Relaxed);
            non_advisory.fetch_add(non_adv, Ordering::Relaxed);
            fixed.fetch_add(applied, Ordering::Relaxed);

            if final_diags.is_empty() && applied == 0 {
                return Ok(());
            }

            let line_index = LineIndex::new(&final_source);
            let path_display = path.display().to_string();
            let guard = stdout_lock
                .lock()
                .map_err(|_| anyhow::anyhow!("stdout lock poisoned"))?;
            let stdout = io::stdout();
            let mut out = stdout.lock();
            emit(
                &mut out,
                &path_display,
                &final_source,
                &line_index,
                &final_diags,
                args.format,
                use_color,
                applied,
            )?;
            drop(guard);
            Ok(())
        })
        .collect();

    for r in results {
        r?;
    }

    let total = totals.load(Ordering::Relaxed);
    let non_adv = non_advisory.load(Ordering::Relaxed);
    let applied = fixed.load(Ordering::Relaxed);

    if matches!(args.format, OutputFormat::Pretty) {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        if apply_fixes {
            writeln!(
                out,
                "{} {applied} fix(es) applied; {total} diagnostic(s) remain ({non_adv} non-advisory).",
                "summary:".bold(),
            )?;
        } else {
            writeln!(
                out,
                "{} {total} diagnostic(s) over {} file(s); {non_adv} non-advisory.",
                "summary:".bold(),
                files.len(),
            )?;
        }
    }

    if args.check && non_adv > 0 {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

/// Resolve the configuration: explicit `--config PATH` if given, else
/// walk up from CWD. Errors from either path mention the file involved.
fn resolve_config(explicit: Option<&std::path::Path>) -> Result<Config> {
    let cfg = match explicit {
        Some(p) => Config::load_explicit(p),
        None => {
            let cwd = std::env::current_dir().context("read current directory")?;
            Config::discover(&cwd)
        }
    };
    cfg.map_err(|e| anyhow!("{e}"))
}

/// Apply config-time rule modifications: extend `info-string-typo`'s
/// allowlist when `[lint.info-strings] extra` is set. Only fires if
/// the rule is in the active set; an explicit `--rules foo,bar`
/// without `info-string-typo` leaves the set untouched.
fn apply_config_to_rules(rules: &mut RuleSet, cfg: &Config) -> Result<()> {
    if !cfg.extra_info_strings().is_empty() && rules.contains("info-string-typo") {
        let _removed = rules.remove("info-string-typo");
        rules
            .add(Box::new(stdlib::InfoStringTypo::with_extra(
                cfg.extra_info_strings().to_vec(),
            )))
            .map_err(|e| anyhow!("{e}"))?;
    }
    Ok(())
}

/// Build a `Gitignore` matcher from the configured patterns. The
/// base is the directory containing the loaded `mdwright.toml`, so
/// patterns are anchored to "the project root" — `docs/vendored/**`
/// resolves the same way regardless of which subdirectory the user
/// invokes `mdwright` from. When no config file is present (defaults
/// case), `$PWD` is the base.
fn build_exclude(patterns: &[String], base: Option<&std::path::Path>) -> Result<Gitignore> {
    let cwd;
    let base = match base {
        Some(b) => b,
        None => {
            cwd = std::env::current_dir().context("read current directory")?;
            cwd.as_path()
        }
    };
    let mut builder = GitignoreBuilder::new(base);
    for pattern in patterns {
        builder
            .add_line(None, pattern)
            .map_err(|e| anyhow!("invalid exclude pattern '{pattern}': {e}"))?;
    }
    builder.build().map_err(|e| anyhow!("{e}"))
}

fn run_stdin(
    rules: &RuleSet,
    lint_opts: LintOptions,
    apply_fixes: bool,
    check: bool,
    format: OutputFormat,
    use_color: bool,
    policy: InputPolicy,
) -> Result<ExitCode> {
    let mut buf = String::new();
    read_stdin_capped(&mut buf, policy, "<stdin>")?;

    let doc = Document::parse(&buf);
    let diags = doc.lint_with(rules, lint_opts);
    let non_adv = diags.iter().filter(|d| !d.advisory).count();

    if apply_fixes {
        let (fixed, _n) = doc.apply_safe_fixes(&diags);
        let stdout = io::stdout();
        let mut out = stdout.lock();
        out.write_all(fixed.as_bytes())?;
    } else {
        let line_index = LineIndex::new(&buf);
        let stdout = io::stdout();
        let mut out = stdout.lock();
        emit(&mut out, "<stdin>", &buf, &line_index, &diags, format, use_color, 0)?;
    }

    if check && non_adv > 0 {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

/// Parse a `--rules` spec like `default,-bare-url,+escaped-emphasis`
/// into a fully-constructed [`RuleSet`].
///
/// The first token decides the base set; subsequent tokens use `+`
/// to add, `-` to remove. A bare name as the first token starts from
/// `{<name>}`; subsequent bare names also start the set fresh — the
/// CLI accepts comma-separated bare lists like
/// `--rules unbalanced-backtick,adjacent-code-no-space` for
/// convenience.
fn parse_rules_spec(spec: &str) -> Result<RuleSet> {
    let tokens: Vec<&str> = spec.split(',').map(str::trim).filter(|t| !t.is_empty()).collect();
    if tokens.is_empty() {
        return Ok(RuleSet::stdlib_defaults());
    }
    let mut rs: Option<RuleSet> = None;
    for tok in tokens {
        if let Some(name) = tok.strip_prefix('+') {
            let rule = stdlib::by_name(name).ok_or_else(|| anyhow::anyhow!("unknown rule in --rules: {name}"))?;
            let target = rs.get_or_insert_with(RuleSet::new);
            if !target.contains(name) {
                target.add(rule).map_err(|e| anyhow::anyhow!("{e}"))?;
            }
        } else if let Some(name) = tok.strip_prefix('-') {
            let target = rs.get_or_insert_with(RuleSet::stdlib_defaults);
            target.remove(name);
        } else if tok == "all" {
            rs = Some(RuleSet::stdlib_all());
        } else if tok == "default" {
            rs = Some(RuleSet::stdlib_defaults());
        } else {
            // Bare name: union into the working set, starting from
            // empty if this is the first token.
            let rule = stdlib::by_name(tok).ok_or_else(|| anyhow::anyhow!("unknown rule in --rules: {tok}"))?;
            let target = rs.get_or_insert_with(RuleSet::new);
            if !target.contains(tok) {
                target.add(rule).map_err(|e| anyhow::anyhow!("{e}"))?;
            }
        }
    }
    Ok(rs.unwrap_or_else(RuleSet::stdlib_defaults))
}

fn emit<W: Write>(
    out: &mut W,
    path: &str,
    source: &str,
    line_index: &LineIndex,
    diags: &[Diagnostic],
    fmt: OutputFormat,
    color: bool,
    fixed: usize,
) -> Result<()> {
    match fmt {
        OutputFormat::Pretty => emit_pretty(out, path, source, line_index, diags, color, fixed),
        OutputFormat::Compact => emit_compact(out, path, diags),
        OutputFormat::Json => emit_json_v2(out, path, source, line_index, diags),
        OutputFormat::JsonV1 => {
            // Deprecation warning per phase-4 plan; printed before the
            // first record so downstream tools that stream output see
            // the warning even when stdout is not flushed.
            let mut stderr = io::stderr().lock();
            writeln!(
                stderr,
                "mdwright: warning: --format=json-v1 is deprecated; switch to --format=json (v2 schema)."
            )?;
            drop(stderr);
            emit_json_v1(out, path, diags)
        }
    }
}

/// rustc-style frame: severity tag, file location, source snippet
/// with caret underline, help line, and a pointer to `mdwright
/// explain`.
fn emit_pretty<W: Write>(
    out: &mut W,
    path: &str,
    source: &str,
    line_index: &LineIndex,
    diags: &[Diagnostic],
    color: bool,
    fixed: usize,
) -> Result<()> {
    if fixed > 0 {
        let tag = if color {
            format!("{}", "fixed".green().bold())
        } else {
            "fixed".to_owned()
        };
        writeln!(out, "{tag}: {fixed} issue(s) auto-repaired in {path}")?;
    }
    // Right-align the line gutter to the widest line number across
    // this file's diagnostics so the `|` column stays vertical.
    let gutter_width = diags.iter().map(|d| digit_width(d.line)).max().unwrap_or(1).max(2);

    for d in diags {
        emit_one_pretty(out, path, source, line_index, d, color, gutter_width)?;
    }
    Ok(())
}

fn digit_width(mut n: usize) -> usize {
    let mut w = 1usize;
    while n >= 10 {
        n /= 10;
        w = w.saturating_add(1);
    }
    w
}

fn emit_one_pretty<W: Write>(
    out: &mut W,
    path: &str,
    source: &str,
    line_index: &LineIndex,
    d: &Diagnostic,
    color: bool,
    gutter_width: usize,
) -> Result<()> {
    // Header: `error[rule]: message` (severity-coloured)
    let severity = d.severity();
    let sev_str = severity.as_str();
    let rule_brackets = format!("[{}]", d.rule);
    if color {
        let sev_painted = match severity {
            Severity::Error => format!("{}", sev_str.red().bold()),
            Severity::Warning => format!("{}", sev_str.yellow().bold()),
            Severity::Advisory => format!("{}", sev_str.cyan().bold()),
        };
        writeln!(out, "{sev_painted}{}: {}", rule_brackets.bold(), d.message)?;
    } else {
        writeln!(out, "{sev_str}{rule_brackets}: {}", d.message)?;
    }

    // Arrow line: `   --> path:line:col`
    let arrow_pad = " ".repeat(gutter_width);
    let arrow = if color {
        format!("{}", "-->".blue().bold())
    } else {
        "-->".to_owned()
    };
    writeln!(out, "{arrow_pad}{arrow} {path}:{}:{}", d.line, d.column)?;

    // Source frame
    if let Some(snippet) = Snippet::from_span(line_index, source, &d.span) {
        let bar = if color {
            format!("{}", "|".blue().bold())
        } else {
            "|".to_owned()
        };
        // Blank gutter line before the source.
        writeln!(out, "{arrow_pad} {bar}")?;
        // Source line with right-aligned line number.
        let line_no_str = format!("{:>width$}", snippet.line_no, width = gutter_width);
        let line_no_painted = if color {
            format!("{}", line_no_str.blue().bold())
        } else {
            line_no_str
        };
        writeln!(out, "{line_no_painted} {bar} {}", snippet.line_text)?;
        // Caret line: spaces up to col_start, then '^' repeated.
        let caret_count = snippet.col_end.saturating_sub(snippet.col_start).max(1) as usize;
        let pad_count = snippet.col_start.saturating_sub(1) as usize;
        let pad = " ".repeat(pad_count);
        let carets_raw: String = std::iter::repeat_n('^', caret_count).collect();
        let carets = if color {
            match severity {
                Severity::Error => format!("{}", carets_raw.red().bold()),
                Severity::Warning => format!("{}", carets_raw.yellow().bold()),
                Severity::Advisory => format!("{}", carets_raw.cyan().bold()),
            }
        } else {
            carets_raw
        };
        writeln!(out, "{arrow_pad} {bar} {pad}{carets}")?;
        writeln!(out, "{arrow_pad} {bar}")?;
    }

    // Help line: first short paragraph of `explain()` if we can
    // recover the rule, falling back to the description. (Rule
    // lookup is `O(n)` over the stdlib; the pretty path is cold.)
    if let Some(help) = help_line_for(&d.rule) {
        let eq = if color {
            format!("{}", "=".blue().bold())
        } else {
            "=".to_owned()
        };
        let help_tag = if color {
            format!("{}", "help".bold())
        } else {
            "help".to_owned()
        };
        writeln!(out, "{arrow_pad} {eq} {help_tag}: {help}")?;
    }

    if let Some(fix) = d.fix.as_ref() {
        let eq = if color {
            format!("{}", "=".blue().bold())
        } else {
            "=".to_owned()
        };
        let tag = if color {
            format!("{}", "fix".bold())
        } else {
            "fix".to_owned()
        };
        let safety = if fix.safe { "safe" } else { "suggestion" };
        writeln!(
            out,
            "{arrow_pad} {eq} {tag} ({safety}): {}",
            single_line_preview(&fix.replacement)
        )?;
    }

    // Footer
    let eq = if color {
        format!("{}", "=".blue().bold())
    } else {
        "=".to_owned()
    };
    let note_tag = if color {
        format!("{}", "note".bold())
    } else {
        "note".to_owned()
    };
    writeln!(out, "{arrow_pad} {eq} {note_tag}: see `mdwright explain {}`", d.rule)?;
    writeln!(out)?;
    Ok(())
}

/// Collapse a multi-line replacement into a one-line preview so the
/// pretty frame stays readable. Full replacement is still emitted in
/// JSON.
fn single_line_preview(s: &str) -> String {
    let trimmed = s.trim_end_matches('\n');
    if let Some(idx) = trimmed.find('\n') {
        let head = trimmed.get(..idx).unwrap_or("");
        format!("{head} …")
    } else {
        trimmed.to_owned()
    }
}

/// First short paragraph of the rule's `explain()`, used as the
/// `help:` line. Returns `None` for unknown or unexplained rules.
fn help_line_for(rule_name: &str) -> Option<String> {
    let rule = stdlib::by_name(rule_name)?;
    let body = rule.explain().trim();
    if body.is_empty() {
        return Some(rule.description().to_owned());
    }
    // Take the first paragraph under "## What it does", or — if the
    // template wasn't followed — the first non-heading paragraph.
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        let t = line.trim();
        if t.starts_with("## ") || t.is_empty() {
            continue;
        }
        // Found a non-heading, non-blank line; collect until blank.
        let mut buf = String::from(t);
        for next in lines.by_ref() {
            let n = next.trim();
            if n.is_empty() {
                break;
            }
            buf.push(' ');
            buf.push_str(n);
        }
        return Some(buf);
    }
    Some(rule.description().to_owned())
}

fn emit_compact<W: Write>(out: &mut W, path: &str, diags: &[Diagnostic]) -> Result<()> {
    for d in diags {
        writeln!(out, "{path}:{}:{}: {}: {}", d.line, d.column, d.rule, d.message)?;
    }
    Ok(())
}

// --- JSON Lines v2 -----------------------------------------------------

#[derive(Serialize)]
struct JsonV2Record<'a> {
    schema_version: u32,
    path: &'a str,
    severity: &'static str,
    rule: JsonV2Rule<'a>,
    source: JsonV2Source<'a>,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<JsonV2Fix<'a>>,
}

#[derive(Serialize)]
struct JsonV2Rule<'a> {
    name: &'a str,
    description: &'a str,
    url: String,
}

#[derive(Serialize)]
struct JsonV2Source<'a> {
    line: u32,
    column: u32,
    span_start: usize,
    span_end: usize,
    snippet: &'a str,
}

#[derive(Serialize)]
struct JsonV2Fix<'a> {
    replacement: &'a str,
    safe: bool,
}

fn emit_json_v2<W: Write>(
    out: &mut W,
    path: &str,
    source: &str,
    line_index: &LineIndex,
    diags: &[Diagnostic],
) -> Result<()> {
    for d in diags {
        let snippet = Snippet::from_span(line_index, source, &d.span);
        let (line, column, snippet_text, span_start, span_end) = match snippet {
            Some(s) => (s.line_no, s.col_start, s.line_text, d.span.start, d.span.end),
            None => (
                u32::try_from(d.line).unwrap_or(u32::MAX),
                u32::try_from(d.column).unwrap_or(u32::MAX),
                "",
                d.span.start,
                d.span.end,
            ),
        };
        let rule_name = d.rule.as_ref();
        let rule_desc = stdlib::by_name(rule_name)
            .map(|r| r.description().to_owned())
            .unwrap_or_default();
        let url = rule_doc_url(rule_name);
        let record = JsonV2Record {
            schema_version: 2,
            path,
            severity: d.severity().as_str(),
            rule: JsonV2Rule {
                name: rule_name,
                description: &rule_desc,
                url,
            },
            source: JsonV2Source {
                line,
                column,
                span_start,
                span_end,
                snippet: snippet_text,
            },
            message: &d.message,
            fix: d.fix.as_ref().map(|f| JsonV2Fix {
                replacement: &f.replacement,
                safe: f.safe,
            }),
        };
        serde_json::to_writer(&mut *out, &record).context("serialize v2 diagnostic")?;
        writeln!(out)?;
    }
    Ok(())
}

// --- JSON Lines v1 (deprecated) ---------------------------------------

fn emit_json_v1<W: Write>(out: &mut W, path: &str, diags: &[Diagnostic]) -> Result<()> {
    for d in diags {
        let path_esc = json_escape(path);
        let msg = json_escape(&d.message);
        let fix = match d.fix.as_ref() {
            None => "null".to_owned(),
            Some(f) => format!(
                r#"{{"replacement":"{}","safe":{}}}"#,
                json_escape(&f.replacement),
                f.safe
            ),
        };
        writeln!(
            out,
            r#"{{"path":"{path_esc}","line":{},"column":{},"span_start":{},"span_end":{},"rule":"{}","advisory":{},"message":"{msg}","fix":{fix}}}"#,
            d.line, d.column, d.span.start, d.span.end, d.rule, d.advisory,
        )?;
    }
    Ok(())
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len().saturating_add(2));
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Install the `tracing` subscriber. `RUST_LOG` wins if set; otherwise
/// the `-v` count maps to warn (0) / info (1) / debug (2) / trace (≥ 3),
/// scoped to the `mdwright` crate so transitive dependencies stay quiet.
/// Idempotent: a second call (e.g. in tests) is a no-op.
fn init_tracing(verbose: u8) {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt;
    use tracing_subscriber::fmt::format::FmtSpan;
    use tracing_subscriber::prelude::*;

    let filter = if std::env::var_os("RUST_LOG").is_some() {
        EnvFilter::from_default_env()
    } else {
        let level = match verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        };
        EnvFilter::new(format!("mdwright={level},warn"))
    };
    let ansi = io::stderr().is_terminal();
    let fmt_layer = fmt::layer()
        .with_ansi(ansi)
        .with_writer(io::stderr)
        .with_span_events(FmtSpan::CLOSE)
        .compact();
    let _init = tracing_subscriber::registry().with(filter).with(fmt_layer).try_init();
}

fn print_rule_catalogue() -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(out, "Rules (use with `--rules`; advisory rules do not fail `--check`):")?;
    let all = RuleSet::stdlib_all();
    for rule in all.iter() {
        let mut tags = Vec::new();
        if !rule.is_default() {
            tags.push("opt-in");
        }
        if rule.is_advisory() {
            tags.push("advisory");
        }
        if rule.produces_fix() {
            tags.push("fixable");
        }
        let tag_str = if tags.is_empty() {
            String::new()
        } else {
            format!(" ({})", tags.join(", "))
        };
        writeln!(out, "  {}{tag_str}", rule.name())?;
        writeln!(out, "    {}", rule.description())?;
    }
    Ok(())
}

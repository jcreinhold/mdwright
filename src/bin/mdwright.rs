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
    Config, Diagnostic, Document, FmtOptions, FormatError, FormatMode, LintOptions, RuleSet,
    discover_markdown, stdlib,
};
use owo_colors::OwoColorize;
use rayon::prelude::*;

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
    /// Path to an `mdwright.toml`. If omitted, mdwright searches
    /// `$PWD`, then walks up the directory tree, and finally falls
    /// back to `$PWD/pyproject.toml`'s `[tool.mdwright]` table.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Increase log verbosity. `-v` = info, `-vv` = debug, `-vvv` = trace.
    /// `RUST_LOG` overrides this when set.
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count, global = true)]
    verbose: u8,

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

    /// Worker threads; 0 = rayon default (one per logical CPU).
    #[arg(short = 'j', long, default_value_t = 0)]
    jobs: usize,

    /// Ignore `<!-- mdwright: allow ... -->` suppression comments.
    /// Use to audit which diagnostics are silenced and where.
    #[arg(long)]
    no_suppress: bool,
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
    /// JSON Lines (one object per line).
    Json,
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

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    let config_path = cli.config;
    match cli.command {
        Command::ListRules => {
            print_rule_catalogue()?;
            Ok(ExitCode::SUCCESS)
        }
        Command::Check(args) => run_lint(&args, false, config_path.as_deref()),
        Command::Fix(args) => run_lint(&args, true, config_path.as_deref()),
        Command::Fmt(args) => run_fmt(&args, false, config_path.as_deref()),
        Command::FmtCheck(mut args) => {
            args.check = true;
            run_fmt(&args, true, config_path.as_deref())
        }
    }
}

fn run_fmt(
    args: &FmtArgs,
    force_check: bool,
    config_path: Option<&std::path::Path>,
) -> Result<ExitCode> {
    let cfg = Config::load(config_path).map_err(|e| anyhow!("{e}"))?;
    let opts = cfg.fmt_options().clone().with_mode(args.mode.into());
    let check = args.check || force_check;

    if args.paths.is_empty() || args.paths.iter().any(|p| p.as_os_str() == "-") {
        return run_fmt_stdin(&opts, args, check);
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
            let doc = Document::parse(&source);
            let formatted = match (validate, doc.format_validated(&opts)) {
                (true, Ok(s)) => s,
                (true, Err(FormatError::HtmlDivergence { formatted, source_html, formatted_html })) => {
                    divergent.fetch_add(1, Ordering::Relaxed);
                    let guard = stderr_lock.lock().map_err(|_| anyhow!("stderr lock poisoned"))?;
                    let mut err = io::stderr().lock();
                    writeln!(
                        err,
                        "mdwright: refusing to write {}: format changes HTML rendering (rerun with --no-validate to override)",
                        path.display()
                    )?;
                    if args.explain_divergence {
                        write_unified_diff(
                            &mut err,
                            &format!("{}.html", path.display()),
                            &source_html,
                            &formatted_html,
                        )?;
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

fn run_fmt_stdin(opts: &FmtOptions, args: &FmtArgs, check: bool) -> Result<ExitCode> {
    let name = args
        .stdin_filename
        .as_deref()
        .map_or_else(|| "<stdin>".to_owned(), |p| p.display().to_string());
    let mut buf = String::new();
    io::Read::read_to_string(&mut io::stdin().lock(), &mut buf).context("read stdin")?;
    let doc = Document::parse(&buf);
    let formatted = if args.no_validate {
        doc.format(opts)
    } else {
        match doc.format_validated(opts) {
            Ok(s) => s,
            Err(FormatError::HtmlDivergence { .. }) => {
                let mut err = io::stderr().lock();
                writeln!(
                    err,
                    "mdwright: refusing to format {name}: format changes HTML rendering (rerun with --no-validate to override)",
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
) -> Result<ExitCode> {
    if args.jobs > 0 {
        rayon::ThreadPoolBuilder::new()
            .num_threads(args.jobs)
            .build_global()
            .context("configure rayon thread pool")?;
    }

    let cfg = Config::load(config_path).map_err(|e| anyhow!("{e}"))?;
    let rules_spec = args.rules.as_deref().unwrap_or_else(|| cfg.rules_spec());
    let mut rules = parse_rules_spec(rules_spec)?;
    apply_config_to_rules(&mut rules, &cfg)?;

    let use_color = matches!(args.format, OutputFormat::Pretty) && io::stdout().is_terminal();
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
            let source =
                fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
            let doc = Document::parse(&source);
            let diags = doc.lint_with(&rules, lint_opts);
            let count = diags.len();
            let non_adv = diags.iter().filter(|d| !d.advisory).count();

            let (final_diags, applied) = if apply_fixes && !diags.is_empty() {
                let (new_src, n) = Document::apply_safe_fixes(&source, &diags);
                if n > 0 && new_src != source {
                    fs::write(path, &new_src)
                        .with_context(|| format!("write {}", path.display()))?;
                }
                let post_doc = Document::parse(&new_src);
                (post_doc.lint_with(&rules, lint_opts), n)
            } else {
                (diags, 0)
            };

            totals.fetch_add(count, Ordering::Relaxed);
            non_advisory.fetch_add(non_adv, Ordering::Relaxed);
            fixed.fetch_add(applied, Ordering::Relaxed);

            if final_diags.is_empty() && applied == 0 {
                return Ok(());
            }

            let path_display = path.display().to_string();
            let guard = stdout_lock
                .lock()
                .map_err(|_| anyhow::anyhow!("stdout lock poisoned"))?;
            let stdout = io::stdout();
            let mut out = stdout.lock();
            emit(
                &mut out,
                &path_display,
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
) -> Result<ExitCode> {
    let mut buf = String::new();
    io::Read::read_to_string(&mut io::stdin().lock(), &mut buf).context("read stdin")?;

    let doc = Document::parse(&buf);
    let diags = doc.lint_with(rules, lint_opts);
    let non_adv = diags.iter().filter(|d| !d.advisory).count();

    if apply_fixes {
        let (fixed, _n) = Document::apply_safe_fixes(&buf, &diags);
        let stdout = io::stdout();
        let mut out = stdout.lock();
        out.write_all(fixed.as_bytes())?;
    } else {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        emit(&mut out, "<stdin>", &diags, format, use_color, 0)?;
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
    let tokens: Vec<&str> = spec
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    if tokens.is_empty() {
        return Ok(RuleSet::stdlib_defaults());
    }
    let mut rs: Option<RuleSet> = None;
    for tok in tokens {
        if let Some(name) = tok.strip_prefix('+') {
            let rule = stdlib::by_name(name)
                .ok_or_else(|| anyhow::anyhow!("unknown rule in --rules: {name}"))?;
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
            let rule = stdlib::by_name(tok)
                .ok_or_else(|| anyhow::anyhow!("unknown rule in --rules: {tok}"))?;
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
    diags: &[Diagnostic],
    fmt: OutputFormat,
    color: bool,
    fixed: usize,
) -> Result<()> {
    match fmt {
        OutputFormat::Pretty => emit_pretty(out, path, diags, color, fixed),
        OutputFormat::Compact => emit_compact(out, path, diags),
        OutputFormat::Json => emit_json(out, path, diags),
    }
}

fn emit_pretty<W: Write>(
    out: &mut W,
    path: &str,
    diags: &[Diagnostic],
    color: bool,
    fixed: usize,
) -> Result<()> {
    let banner = if color {
        format!("{}", path.bold().underline())
    } else {
        path.to_owned()
    };
    writeln!(out, "{banner}")?;
    if fixed > 0 {
        let tag = if color {
            format!("{}", "fixed".green())
        } else {
            "fixed".to_owned()
        };
        writeln!(out, "  {tag}: {fixed} issue(s) auto-repaired")?;
    }
    for d in diags {
        let rule_tag = if color {
            if d.advisory {
                format!("{}", d.rule.yellow())
            } else {
                format!("{}", d.rule.red().bold())
            }
        } else {
            d.rule.to_string()
        };
        writeln!(out, "  {}:{}: {rule_tag}: {}", d.line, d.column, d.message)?;
    }
    writeln!(out)?;
    Ok(())
}

fn emit_compact<W: Write>(out: &mut W, path: &str, diags: &[Diagnostic]) -> Result<()> {
    for d in diags {
        writeln!(
            out,
            "{path}:{}:{}: {}: {}",
            d.line, d.column, d.rule, d.message
        )?;
    }
    Ok(())
}

fn emit_json<W: Write>(out: &mut W, path: &str, diags: &[Diagnostic]) -> Result<()> {
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
    let _init = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .try_init();
}

fn print_rule_catalogue() -> Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(
        out,
        "Rules (use with `--rules`; advisory rules do not fail `--check`):"
    )?;
    let all = RuleSet::stdlib_all();
    for rule in all.iter() {
        let mut tags = Vec::new();
        if !rule.is_default() {
            tags.push("opt-in");
        }
        if rule.is_advisory() {
            tags.push("advisory");
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

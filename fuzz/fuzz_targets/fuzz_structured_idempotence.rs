#![no_main]
//! Structurally-biased idempotence fuzzer.
//!
//! `fuzz_idempotence` mutates raw bytes; libFuzzer learns Markdown
//! shape from the seed corpus but reaches deep nested constructs
//! (footnote inside loose list inside blockquote, aligning math env,
//! GFM table with alignment row) slowly. This target consumes an
//! `arbitrary::Unstructured` stream and assembles a Markdown source
//! from block-shaped templates, then asserts the same oracle as
//! `fuzz_idempotence` *with* `FmtOptions` also driven by the byte
//! stream (wrap × mode × math.normalise).
//!
//! Generator is local rather than re-using `tests/common/proptest_gen.rs`
//! because proptest strategies are sampling-based and incompatible with
//! `arbitrary::Unstructured`'s byte-driven model — but the *bias* (which
//! constructs and at what weights) tracks the proptest generator.

use arbitrary::{Arbitrary, Unstructured};
use libfuzzer_sys::fuzz_target;
use mdwright_document::Document;
use mdwright_format::{FmtOptions, MathOptions, Wrap};

const MAX_OUTPUT: usize = 16_384;
const MAX_BLOCKS: usize = 16;

fn word(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let n = u8::arbitrary(u).map(|b| 1 + (b % 6))?;
    let mut s = String::with_capacity(usize::from(n));
    for _ in 0..n {
        let c = b'a' + (u8::arbitrary(u)? % 26);
        s.push(char::from(c));
    }
    Ok(s)
}

fn words(n: usize, u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let mut parts = Vec::with_capacity(n);
    for _ in 0..n {
        parts.push(word(u)?);
    }
    Ok(parts.join(" "))
}

/// Block templates, weighted toward shapes that have historically
/// surfaced idempotence bugs (lists, fences, math envs, tables).
fn gen_block(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let tag = u8::arbitrary(u)? % 16;
    Ok(match tag {
        0 => {
            let level = 1 + (u8::arbitrary(u)? % 6);
            let hashes = "#".repeat(usize::from(level));
            format!("{hashes} {}\n", words(3, u)?)
        }
        1 => format!("{}\n", words(6, u)?),
        2 => {
            let n = 1 + (u8::arbitrary(u)? % 4);
            let mut out = String::new();
            for _ in 0..n {
                out.push_str(&format!("- {}\n", words(3, u)?));
            }
            out
        }
        3 => {
            let n = 1 + (u8::arbitrary(u)? % 4);
            let mut out = String::new();
            for i in 0..n {
                out.push_str(&format!("{}. {}\n", i + 1, words(3, u)?));
            }
            out
        }
        4 => format!("> {}\n", words(4, u)?),
        5 => format!("```\n{}\n```\n", words(3, u)?),
        6 => format!("```rust\nlet {} = 1;\n```\n", word(u)?),
        7 => {
            let h1 = word(u)?;
            let h2 = word(u)?;
            let c1 = word(u)?;
            let c2 = word(u)?;
            format!("| {h1} | {h2} |\n|:---|---:|\n| {c1} | {c2} |\n")
        }
        8 => format!("`{}`\n", word(u)?),
        9 => format!("*{}*\n", word(u)?),
        10 => format!("**{}**\n", word(u)?),
        11 => format!("[{}](https://example.com)\n", word(u)?),
        12 => {
            let a = word(u)?;
            let b = word(u)?;
            format!("\\[\n{a} = {b}\n\\]\n")
        }
        13 => {
            let a = word(u)?;
            let b = word(u)?;
            let c = word(u)?;
            let d = word(u)?;
            format!("\\begin{{align}}\n{a} &= {b} \\\\\n{c} &= {d}\n\\end{{align}}\n")
        }
        14 => {
            let label = word(u)?;
            let body = words(2, u)?;
            format!("Body [^{label}].\n\n[^{label}]: {body}\n")
        }
        _ => "---\n".to_owned(),
    })
}

fn gen_document(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let n = (u8::arbitrary(u)? as usize % MAX_BLOCKS).saturating_add(1);
    let mut out = String::new();
    for _ in 0..n {
        let block = gen_block(u)?;
        out.push_str(&block);
        out.push('\n');
        if out.len() > MAX_OUTPUT {
            break;
        }
    }
    Ok(out)
}

/// Map a byte to `FmtOptions` so the option space is fuzzed alongside
/// the source. Only knobs with public setters are varied; that's wrap,
/// mode, and math. Italic / list-marker / placement live behind the
/// schema and aren't reachable from a programmatic builder yet.
fn opts_from_byte(byte: u8) -> FmtOptions {
    // Mirrors `fuzz_idempotence.rs::opts_from_byte`. `At(120)` is in
    // the rotation specifically to exercise the atomicity contract
    // — table rows, ATX heading bodies, and fenced info strings
    // must stay on one line at every wrap budget.
    let wrap = match byte & 0b111 {
        0 => Wrap::Keep,
        1 => Wrap::No,
        2 => Wrap::At(60),
        3 => Wrap::At(80),
        _ => Wrap::At(120),
    };
    // Bit 2 is reserved; preserves the option-byte width so existing
    // corpus seeds remain meaningful.
    let math = MathOptions {
        normalise: byte & 0b1000 != 0,
        ..MathOptions::default()
    };
    FmtOptions::default().with_wrap(wrap).with_math(math)
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let Ok(option_byte) = u8::arbitrary(&mut u) else {
        return;
    };
    let opts = opts_from_byte(option_byte);
    let Ok(src) = gen_document(&mut u) else {
        return;
    };
    let once = mdwright_format::format_document(&Document::parse(&src), &opts);
    let twice = mdwright_format::format_document(&Document::parse(&once), &opts);
    assert_eq!(once, twice, "format is not idempotent (opt byte {option_byte:#04x})");
});

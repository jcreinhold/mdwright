#![no_main]
//! Idempotence: `format(parse(format(parse(s))))` must equal
//! `format(parse(s))`. A second format must be a no-op.
//!
//! The first input byte drives the `FmtOptions` space (wrap × mode ×
//! math.normalise) so option × construct interactions are exercised,
//! not only the default-style path.

use libfuzzer_sys::fuzz_target;
use mdwright::{Document, FmtOptions, FormatMode, MathOptions, Wrap};

const MAX_INPUT: usize = 65_536;

fn opts_from_byte(byte: u8) -> FmtOptions {
    let wrap = match byte & 0b11 {
        0 => Wrap::Keep,
        1 => Wrap::No,
        2 => Wrap::At(60),
        _ => Wrap::At(80),
    };
    let mode = if byte & 0b100 != 0 {
        FormatMode::Verbatim
    } else {
        FormatMode::Normalise
    };
    let math = MathOptions {
        normalise: byte & 0b1000 != 0,
    };
    FmtOptions::default()
        .with_wrap(wrap)
        .with_mode(mode)
        .with_math(math)
}

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_INPUT {
        return;
    }
    let Some((&option_byte, rest)) = data.split_first() else {
        return;
    };
    let Ok(s) = std::str::from_utf8(rest) else {
        return;
    };
    let opts = opts_from_byte(option_byte);
    let once = Document::parse(s).format(&opts);
    let twice = Document::parse(&once).format(&opts);
    assert_eq!(once, twice, "format is not idempotent (opt byte {option_byte:#04x})");
});

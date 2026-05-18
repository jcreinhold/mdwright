//! Strikethrough runs (GFM §6.5).
//!
//! Unit struct: the IR records that a `Strikethrough` event was seen
//! so lint rules can act on it. Structural emit preserves the source
//! `~~…~~` bytes verbatim.

#![allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct Strikethrough;

impl Strikethrough {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strikethrough_is_uniquely_inhabited() {
        assert_eq!(Strikethrough::new(), Strikethrough);
    }
}

//! Thematic breaks (CM §4.1).
//!
//! A thematic break is one of three canonical lines: `---`, `***`, or
//! `___`. The IR records the parse-time choice; structural emit
//! echoes the source line verbatim and never consults
//! [`crate::config::FmtOptions::thematic_break_style`]. Style
//! canonicalisation (always-dash etc.) is a separate post-pass.

use crate::config::ThematicStyle;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThematicBreak {
    style: ThematicStyle,
}

impl ThematicBreak {
    #[tracing::instrument(level = "trace")]
    pub(crate) fn new(style: ThematicStyle) -> Self {
        Self { style }
    }

    #[cfg(test)]
    pub(crate) fn style(self) -> ThematicStyle {
        self.style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_round_trips() {
        for s in [
            ThematicStyle::Dash,
            ThematicStyle::Asterisk,
            ThematicStyle::Underscore,
            ThematicStyle::Preserve,
        ] {
            assert_eq!(ThematicBreak::new(s).style(), s);
        }
    }
}

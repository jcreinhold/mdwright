//! Thematic breaks (CM §4.1).
//!
//! A thematic break is one of three canonical lines: `---`, `***`, or
//! `___`. The choice is a formatter policy (see
//! [`crate::config::FmtOptions::thematic_break_style`]); the type
//! records which the IR has committed to. Prompt-16's "always emit
//! `---` regardless of source" rule is now the default value of that
//! policy field, not a hard-coded constant in the emitter.

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
        ] {
            assert_eq!(ThematicBreak::new(s).style(), s);
        }
    }

}

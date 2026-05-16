//! Thematic breaks (CM §4.1).
//!
//! A thematic break is one of three canonical lines: `---`, `***`, or
//! `___`. The choice is a formatter policy (see
//! [`crate::config::FmtOptions::thematic_break_style`]); the type
//! records which the IR has committed to. Prompt-16's "always emit
//! `---` regardless of source" rule is now the default value of that
//! policy field, not a hard-coded constant in the emitter.

use crate::config::ThematicStyle;
use crate::format::doc::{Doc, concat, hard_line, text};
use crate::format::pretty::PrettyCtx;

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

    /// Emit the CM §4.1 line: three repetitions of the configured byte,
    /// terminated by a hard newline. The byte source is
    /// [`PrettyCtx::opts`]'s `thematic_break_style`; the value carried
    /// on `self` is the parse-time choice and is currently overridden
    /// by the formatter setting.
    #[tracing::instrument(level = "trace", skip_all)]
    #[allow(clippy::unused_self)]
    pub(crate) fn pretty<'a>(self, ctx: &PrettyCtx<'a>, _id: crate::tree::NodeId) -> Doc<'a> {
        let b = ctx.opts.thematic_break_style().as_byte();
        let line: String = std::iter::repeat_n(char::from(b), 3).collect();
        concat([text(line), hard_line()])
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

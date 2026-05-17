//! Thematic breaks (CM §4.1).
//!
//! A thematic break is one of three canonical lines: `---`, `***`, or
//! `___`. The IR records the parse-time choice; structural emit
//! echoes the source line verbatim and never consults
//! [`crate::config::FmtOptions::thematic_break_style`]. Style
//! canonicalisation (always-dash etc.) is a separate post-pass.

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

    #[cfg(test)]
    pub(crate) fn style(self) -> ThematicStyle {
        self.style
    }

    /// Emit the source thematic-break line verbatim, terminated by a
    /// hard newline. Reads source bytes via
    /// [`crate::tree::Tree::raw_text`]; never consults `FmtOptions`.
    #[tracing::instrument(level = "trace", skip_all)]
    #[allow(clippy::unused_self)]
    pub(crate) fn pretty<'a>(self, ctx: &PrettyCtx<'a>, id: crate::tree::NodeId) -> Doc<'a> {
        let raw = ctx.tree.raw_text(ctx.source, id).trim_end_matches('\n');
        concat([text(raw.to_owned()), hard_line()])
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

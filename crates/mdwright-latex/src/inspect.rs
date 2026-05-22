//! Narrow inspection of math-body source for downstream linters.
//!
//! Linters that need to enumerate every TeX command, environment, and text-mode
//! region inside a math body should use this surface rather than the parser:
//! the parser rejects commands outside mdwright's Unicode subset, so its tree
//! does not see them. The inspect walk operates on the token stream and yields
//! every command sighting, whether or not mdwright can render it.

use crate::SourceSpan;
use crate::lexer::{Token, TokenKind, TokenStream};

/// One event from a left-to-right walk of math-body source.
///
/// Spans are byte ranges into the source slice passed to `inspect_math_body`.
/// `Command` and environment names are returned without the leading backslash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandEvent<'src> {
    /// A `\name` use that is not `\begin`, `\end`, or a recognised text-mode
    /// command. The span covers the backslash and the command name.
    Command {
        /// Command name without the leading backslash.
        name: &'src str,
        /// Byte range covering the command token in the source.
        span: SourceSpan,
    },
    /// A `\begin{name}` opener. The span covers `\begin` and the brace group.
    EnvironmentEnter {
        /// Environment name as written inside the braces.
        name: &'src str,
        /// Byte range from `\begin` through the closing brace.
        span: SourceSpan,
    },
    /// A matched `\end{name}` closer. The span covers `\end` and the brace group.
    EnvironmentExit {
        /// Environment name as written inside the braces.
        name: &'src str,
        /// Byte range from `\end` through the closing brace.
        span: SourceSpan,
    },
    /// Entry into a text-mode region (`\text{...}` and friends). The span covers
    /// the opening brace only.
    TextModeEnter {
        /// Byte range of the opening brace.
        span: SourceSpan,
    },
    /// Exit from a text-mode region. The span covers the closing brace only.
    TextModeExit {
        /// Byte range of the closing brace.
        span: SourceSpan,
    },
}

/// Walk `source` as a TeX math body and return the command-usage event stream.
///
/// The walk is lexer-based: it does not run the parser, does not reject
/// commands, and allocates only the result vector. Unbalanced groups inside
/// `\begin{...}` or text-mode commands are tolerated by recovery — they may
/// produce fewer paired `EnvironmentEnter`/`Exit` or `TextModeEnter`/`Exit`
/// events but never dangling ones.
#[must_use]
pub fn inspect_math_body(source: &str) -> Vec<CommandEvent<'_>> {
    let stream = TokenStream::new(source);
    let tokens = stream.tokens();
    let mut events = Vec::new();
    let mut text_stack: Vec<usize> = Vec::new();
    let mut env_stack: Vec<&str> = Vec::new();
    let mut depth: usize = 0;

    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        match token.kind() {
            TokenKind::CommandWord(raw) => {
                let name = raw.strip_prefix('\\').unwrap_or(raw);
                let next_index = index.saturating_add(1);
                if name == "begin" {
                    if let Some((env_name, group_end_index, end_span)) = read_braced_name(source, tokens, next_index) {
                        let span = SourceSpan::new(token.span().start(), end_span.end());
                        events.push(CommandEvent::EnvironmentEnter { name: env_name, span });
                        env_stack.push(env_name);
                        index = group_end_index.saturating_add(1);
                        continue;
                    }
                    events.push(CommandEvent::Command {
                        name,
                        span: token.span(),
                    });
                } else if name == "end" {
                    if let Some((env_name, group_end_index, end_span)) = read_braced_name(source, tokens, next_index) {
                        let span = SourceSpan::new(token.span().start(), end_span.end());
                        if env_stack.last() == Some(&env_name) {
                            env_stack.pop();
                        }
                        events.push(CommandEvent::EnvironmentExit { name: env_name, span });
                        index = group_end_index.saturating_add(1);
                        continue;
                    }
                    events.push(CommandEvent::Command {
                        name,
                        span: token.span(),
                    });
                } else if is_text_mode_command(name) {
                    events.push(CommandEvent::Command {
                        name,
                        span: token.span(),
                    });
                    if let Some(open_index) = skip_trivia(tokens, next_index)
                        && let Some(open_token) = tokens.get(open_index)
                        && matches!(open_token.kind(), TokenKind::LeftBrace)
                    {
                        events.push(CommandEvent::TextModeEnter {
                            span: open_token.span(),
                        });
                        text_stack.push(depth.saturating_add(1));
                    }
                } else {
                    events.push(CommandEvent::Command {
                        name,
                        span: token.span(),
                    });
                }
            }
            TokenKind::LeftBrace => {
                depth = depth.saturating_add(1);
            }
            TokenKind::RightBrace => {
                if text_stack.last() == Some(&depth) {
                    text_stack.pop();
                    events.push(CommandEvent::TextModeExit { span: token.span() });
                }
                depth = depth.saturating_sub(1);
            }
            TokenKind::ControlSymbol(_)
            | TokenKind::LeftBracket
            | TokenKind::RightBracket
            | TokenKind::LeftParen
            | TokenKind::RightParen
            | TokenKind::Superscript
            | TokenKind::Subscript
            | TokenKind::Alignment
            | TokenKind::RowSeparator
            | TokenKind::Comment(_)
            | TokenKind::Whitespace(_)
            | TokenKind::Number(_)
            | TokenKind::Identifier(_)
            | TokenKind::Punctuation(_)
            | TokenKind::UnicodeSymbol(_)
            | TokenKind::Error
            | TokenKind::Eof => {}
        }
        index = index.saturating_add(1);
    }

    events
}

fn skip_trivia(tokens: &[Token<'_>], start: usize) -> Option<usize> {
    let mut index = start;
    while let Some(token) = tokens.get(index) {
        if matches!(token.kind(), TokenKind::Whitespace(_) | TokenKind::Comment(_)) {
            index = index.saturating_add(1);
            continue;
        }
        if matches!(token.kind(), TokenKind::Eof) {
            return None;
        }
        return Some(index);
    }
    None
}

/// Read a `{ name }` group starting at `start`, mirroring the parser's
/// `parse_raw_braced_text` behaviour: gather everything between the braces as
/// raw source text. Returns the borrowed name slice, the index of the closing
/// brace token, and that token's span. Returns `None` if the brace group is
/// absent or unbalanced.
fn read_braced_name<'src>(
    source: &'src str,
    tokens: &[Token<'src>],
    start: usize,
) -> Option<(&'src str, usize, SourceSpan)> {
    let open_index = skip_trivia(tokens, start)?;
    let open_token = tokens.get(open_index)?;
    if !matches!(open_token.kind(), TokenKind::LeftBrace) {
        return None;
    }
    let content_start = open_token.span().end();
    let mut cursor = open_index.saturating_add(1);
    while let Some(token) = tokens.get(cursor) {
        if matches!(token.kind(), TokenKind::RightBrace) {
            let close_span = token.span();
            let content_end = close_span.start();
            let raw = source.get(content_start..content_end)?;
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return None;
            }
            let offset = raw.find(trimmed).unwrap_or(0);
            let start_offset = content_start.saturating_add(offset);
            let end_offset = start_offset.saturating_add(trimmed.len());
            let borrowed = source.get(start_offset..end_offset)?;
            return Some((borrowed, cursor, close_span));
        }
        if matches!(token.kind(), TokenKind::Eof) {
            return None;
        }
        cursor = cursor.saturating_add(1);
    }
    None
}

fn is_text_mode_command(name: &str) -> bool {
    matches!(
        name,
        "text" | "textrm" | "textbf" | "textit" | "textsf" | "texttt" | "textnormal" | "mbox" | "hbox"
    )
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        reason = "tests assert event shape and span text against known inputs"
    )]

    use super::*;

    fn names(events: &[CommandEvent<'_>]) -> Vec<String> {
        events
            .iter()
            .map(|event| match event {
                CommandEvent::Command { name, .. } => format!("cmd:{name}"),
                CommandEvent::EnvironmentEnter { name, .. } => format!("env+:{name}"),
                CommandEvent::EnvironmentExit { name, .. } => format!("env-:{name}"),
                CommandEvent::TextModeEnter { .. } => "text+".to_owned(),
                CommandEvent::TextModeExit { .. } => "text-".to_owned(),
            })
            .collect()
    }

    #[test]
    fn enumerates_top_level_commands_with_spans() {
        let source = r"\alpha + \beta";
        let events = inspect_math_body(source);
        assert_eq!(names(&events), vec!["cmd:alpha", "cmd:beta"]);
        let CommandEvent::Command { span, .. } = events[0] else {
            panic!("expected command event");
        };
        assert_eq!(&source[span.as_range()], r"\alpha");
    }

    #[test]
    fn pairs_begin_and_end_for_environments() {
        let source = r"\begin{matrix}a & b\end{matrix}";
        let events = inspect_math_body(source);
        assert_eq!(names(&events), vec!["env+:matrix", "env-:matrix"]);
    }

    #[test]
    fn captures_starred_environment_names() {
        let source = r"\begin{align*}x\end{align*}";
        let events = inspect_math_body(source);
        assert_eq!(names(&events), vec!["env+:align*", "env-:align*"]);
    }

    #[test]
    fn enters_and_exits_text_mode_on_text_command() {
        let source = r"\text{hello \alpha}";
        let events = inspect_math_body(source);
        assert_eq!(names(&events), vec!["cmd:text", "text+", "cmd:alpha", "text-"],);
    }

    #[test]
    fn pairs_nested_text_with_outer_brace_groups() {
        let source = r"{x \text{y \alpha} z}";
        let events = inspect_math_body(source);
        assert_eq!(names(&events), vec!["cmd:text", "text+", "cmd:alpha", "text-"],);
    }

    #[test]
    fn surfaces_commands_the_parser_rejects() {
        let source = r"\xrightarrow{f} \ce{H2O}";
        let events = inspect_math_body(source);
        assert_eq!(names(&events), vec!["cmd:xrightarrow", "cmd:ce"]);
    }

    #[test]
    fn falls_back_to_command_when_begin_has_no_argument() {
        let source = r"\begin";
        let events = inspect_math_body(source);
        assert_eq!(names(&events), vec!["cmd:begin"]);
    }

    #[test]
    fn does_not_emit_text_events_inside_unrelated_groups() {
        let source = r"{a + b}";
        let events = inspect_math_body(source);
        assert!(events.is_empty());
    }
}

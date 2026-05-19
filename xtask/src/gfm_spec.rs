use std::path::{Path, PathBuf};

/// One example from the vendored cmark-gfm spec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecCase {
    pub number: u32,
    pub section: String,
    pub classes: Vec<String>,
    pub source: String,
    pub expected_html: String,
}

#[must_use]
pub fn spec_path(workspace: &Path) -> PathBuf {
    workspace.join("crates/mdwright/tests/gfm-spec/spec.txt")
}

/// Parse cmark-gfm's `spec.txt` fixture format.
#[must_use]
pub fn parse_spec(text: &str) -> Vec<SpecCase> {
    const FENCE: &str = "````````````````````````````````";
    let mut out = Vec::new();
    let mut section = String::new();
    let mut number: u32 = 0;
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_end();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            section = strip_anchor(rest);
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            section = strip_anchor(rest);
            continue;
        }
        let Some(header_rest) = trimmed.strip_prefix(FENCE) else {
            continue;
        };
        let header_rest = header_rest.trim_start();
        let Some(class_rest) = header_rest.strip_prefix("example") else {
            continue;
        };
        number = number.saturating_add(1);
        let classes = class_rest.split_whitespace().map(str::to_owned).collect::<Vec<_>>();

        let mut source = String::new();
        let mut expected_html = String::new();
        let mut in_source = true;
        for inner in lines.by_ref() {
            let inner_trim = inner.trim_end();
            if inner_trim == FENCE {
                break;
            }
            if in_source && inner_trim == "." {
                in_source = false;
                continue;
            }
            if in_source {
                source.push_str(&inner.replace('→', "\t"));
                source.push('\n');
            } else {
                expected_html.push_str(&inner.replace('→', "\t"));
                expected_html.push('\n');
            }
        }
        out.push(SpecCase {
            number,
            section: section.clone(),
            classes,
            source,
            expected_html,
        });
    }
    out
}

fn strip_anchor(s: &str) -> String {
    s.split(" {#").next().unwrap_or(s).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::parse_spec;

    #[test]
    fn decodes_tab_markers_in_source_and_expected_html() {
        let text = "# Section {#section}\n\
                    ```````````````````````````````` example\n\
                    →foo\n\
                    .\n\
                    <pre><code>→foo\n\
                    </code></pre>\n\
                    ````````````````````````````````\n";
        let cases = parse_spec(text);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].number, 1);
        assert_eq!(cases[0].section, "Section");
        assert_eq!(cases[0].classes, Vec::<String>::new());
        assert_eq!(cases[0].source, "\tfoo\n");
        assert_eq!(cases[0].expected_html, "<pre><code>\tfoo\n</code></pre>\n");
    }

    #[test]
    fn parses_source_classes_and_expected_html() {
        let text = "# Section {#section}\n\
                    ```````````````````````````````` example table\n\
                    | a |\n\
                    | - |\n\
                    .\n\
                    <table>\n\
                    </table>\n\
                    ````````````````````````````````\n";
        let cases = parse_spec(text);
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].number, 1);
        assert_eq!(cases[0].section, "Section");
        assert_eq!(cases[0].classes, ["table"]);
        assert_eq!(cases[0].source, "| a |\n| - |\n");
        assert_eq!(cases[0].expected_html, "<table>\n</table>\n");
    }
}

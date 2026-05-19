use mdwright_document::{ParseOptions, RenderOptions, RenderProfile, render_html, render_html_with_render_options};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn cmark_profile() -> RenderOptions {
    RenderOptions::default().with_profile(RenderProfile::CmarkGfm)
}

#[test]
fn default_render_profile_keeps_pulldown_table_spelling() -> TestResult {
    let src = "| foo | bar |\n| --- | --- |\n| baz | bim |\n";
    let html = render_html(src)?;
    assert_eq!(
        html,
        "<table><thead><tr><th>foo</th><th>bar</th></tr></thead><tbody>\n\
         <tr><td>baz</td><td>bim</td></tr>\n\
         </tbody></table>\n"
    );
    Ok(())
}

#[test]
fn cmark_profile_uses_cmark_table_spelling() -> TestResult {
    let src = "| foo | bar |\n| --- | --- |\n| baz | bim |\n";
    let html = render_html_with_render_options(src, ParseOptions::default(), cmark_profile())?;
    assert_eq!(
        html,
        "<table>\n\
         <thead>\n\
         <tr>\n\
         <th>foo</th>\n\
         <th>bar</th>\n\
         </tr>\n\
         </thead>\n\
         <tbody>\n\
         <tr>\n\
         <td>baz</td>\n\
         <td>bim</td>\n\
         </tr>\n\
         </tbody>\n\
         </table>\n"
    );
    Ok(())
}

#[test]
fn cmark_profile_uses_cmark_task_list_spelling() -> TestResult {
    let html = render_html_with_render_options("- [ ] foo\n- [x] bar\n", ParseOptions::default(), cmark_profile())?;
    assert_eq!(
        html,
        "<ul>\n\
         <li><input type=\"checkbox\" disabled=\"\" /> foo</li>\n\
         <li><input type=\"checkbox\" checked=\"\" disabled=\"\" /> bar</li>\n\
         </ul>\n"
    );
    Ok(())
}

#[test]
fn cmark_profile_escapes_quotes_in_text_and_code() -> TestResult {
    let src = "`Foo\n----\n`\n\n<a title=\"a lot\n---\nof dashes\"/>\n";
    let html = render_html_with_render_options(src, ParseOptions::default(), cmark_profile())?;
    assert_eq!(
        html,
        "<h2>`Foo</h2>\n\
         <p>`</p>\n\
         <h2>&lt;a title=&quot;a lot</h2>\n\
         <p>of dashes&quot;/&gt;</p>\n"
    );
    Ok(())
}

#[test]
fn cmark_profile_percent_encodes_link_destinations() -> TestResult {
    let html = render_html_with_render_options(
        "[link](</my uri> \"title\")\n\n[unicode](/φου)\n\n[quoted](\"title\")\n\n[apostrophe](m')\n",
        ParseOptions::default(),
        cmark_profile(),
    )?;
    assert_eq!(
        html,
        "<p><a href=\"/my%20uri\" title=\"title\">link</a></p>\n\
         <p><a href=\"/%CF%86%CE%BF%CF%85\">unicode</a></p>\n\
         <p><a href=\"%22title%22\">quoted</a></p>\n\
         <p><a href=\"m&#x27;\">apostrophe</a></p>\n"
    );
    Ok(())
}

#[test]
fn cmark_profile_preserves_known_html_block_newline_spelling() -> TestResult {
    let html = render_html_with_render_options("- <div>\n- foo\n", ParseOptions::default(), cmark_profile())?;
    assert_eq!(
        html,
        "<ul>\n\
         <li>\n\
         <div>\n\
         </li>\n\
         <li>foo</li>\n\
         </ul>\n"
    );
    Ok(())
}

#[test]
fn cmark_profile_does_not_change_parser_emphasis_semantics() -> TestResult {
    let html = render_html_with_render_options("__foo, __bar__, baz__\n", ParseOptions::default(), cmark_profile())?;
    assert_eq!(html, "<p><strong>foo, <strong>bar</strong>, baz</strong></p>\n");
    Ok(())
}

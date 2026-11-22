//! Renders the comment's Markdown (SDOC-7) and inserts resolved
//! declaration links (SDOC-8). Authored HTML and external destinations
//! never become active content.

use std::ops::Range;

use hgl_diagnostics::Diagnostics;
use hgl_lexer::TokenKind;
use hgl_source::SourceMap;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};

/// Appends a CommonMark fragment. The callback only returns destinations
/// of declarations present in the generated site.
pub(crate) fn render(output: &mut String, text: &str, link: impl Fn(&str) -> Option<String>) {
    let mut source = Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter();
    let mut events = Vec::new();
    while let Some((event, range)) = source.next() {
        let event = match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
                if matches!(&*info, "hgl" | "hgl no-run") =>
            {
                let mut code = String::new();
                for (event, _) in source.by_ref() {
                    match event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(text) => code.push_str(&text),
                        _ => {}
                    }
                }
                Event::Html(
                    format!(
                        "<pre><code class=\"language-hgl\">{}</code></pre>\n",
                        highlight(&code)
                    )
                    .into(),
                )
            }
            Event::Start(Tag::HtmlBlock) => Event::Html("<pre class=\"authored-html\">".into()),
            Event::End(TagEnd::HtmlBlock) => Event::Html("</pre>\n".into()),
            Event::Html(markup) | Event::InlineHtml(markup) => Event::Text(markup),
            // Strip the destination while preserving the link label or image
            // description. The source comment may refer to an online host.
            Event::Start(Tag::Link { .. } | Tag::Image { .. })
            | Event::End(TagEnd::Link | TagEnd::Image) => continue,
            Event::Code(name) if !formula(text, range) => match link(&name) {
                Some(href) => Event::InlineHtml(
                    format!(
                        "<a href=\"{}\"><code>{}</code></a>",
                        escaped(&href),
                        escaped(&name)
                    )
                    .into(),
                ),
                None => Event::Code(name),
            },
            event => event,
        };
        events.push(event);
    }
    html::push_html(output, events.into_iter());
}

/// Dollar-delimited code spans belong to a formula (SDOC-7), even if the
/// formula text happens to be an in-scope declaration name.
fn formula(text: &str, range: Range<usize>) -> bool {
    let Some(dollar) = range.start.checked_sub(1) else {
        return false;
    };
    text.as_bytes()[dollar] == b'$'
        && text.as_bytes().get(range.end) == Some(&b'$')
        && text[..dollar]
            .bytes()
            .rev()
            .take_while(|&byte| byte == b'\\')
            .count()
            % 2
            == 0
}

fn highlight(code: &str) -> String {
    let mut sources = SourceMap::new();
    let Ok(file) = sources.add("example.hgl", code) else {
        return escaped(code);
    };
    let tokens = hgl_lexer::lex(&sources, file, &mut Diagnostics::new());
    let mut output = String::new();
    if sources.file(file).has_byte_order_mark() {
        output.push('\u{feff}');
    }
    for token in tokens {
        let text = sources.file(file).slice(token.span);
        let class = match token.kind {
            TokenKind::LineComment | TokenKind::BlockComment | TokenKind::DocComment => {
                Some("comment")
            }
            TokenKind::Str => Some("string"),
            kind if kind.is_literal() => Some("literal"),
            kind if kind.is_keyword() => Some("keyword"),
            _ => None,
        };
        if let Some(class) = class {
            output.push_str(&format!("<span class=\"{class}\">{}</span>", escaped(text)));
        } else {
            output.push_str(&escaped(text));
        }
    }
    output
}

/// Escapes a string for an HTML text node or a double-quoted attribute.
pub(crate) fn escaped(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for letter in text.chars() {
        match letter {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(letter),
        }
    }
    output
}

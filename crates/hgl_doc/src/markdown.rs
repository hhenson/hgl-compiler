//! Renders the comment's Markdown (SDOC-7) and inserts resolved
//! declaration links (SDOC-8). Authored destinations are visible as text.
//! Web link labels may navigate, and image labels are rendered as text.

use std::ops::Range;

use hgl_diagnostics::Diagnostics;
use hgl_lexer::TokenKind;
use hgl_source::SourceMap;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};

/// The page text and which optional renderers its Markdown actually uses.
pub(crate) struct Markup {
    pub(crate) html: String,
    pub(crate) renderers: Renderers,
}

impl Markup {
    /// Appends a plain heading at its level in the reference outline.
    pub(crate) fn heading(&mut self, level: usize, title: &str) {
        self.html.push_str(&heading_start(level));
        self.html.push_str(&escaped(title));
        self.html.push_str(&heading_end(level));
    }
}

fn heading_start(level: usize) -> String {
    if level <= 6 {
        format!("<h{level}>")
    } else {
        format!("<div role=\"heading\" aria-level=\"{level}\">")
    }
}

fn heading_end(level: usize) -> String {
    if level <= 6 {
        format!("</h{level}>\n")
    } else {
        "</div>\n".to_owned()
    }
}

/// Browser assets needed by the rendered forms (SDOC-7).
#[derive(Default)]
pub(crate) struct Renderers {
    pub(crate) math: bool,
    pub(crate) mermaid: bool,
}

impl Renderers {
    /// Whether a page needs the renderer loader.
    pub(crate) fn any(&self) -> bool {
        self.math || self.mermaid
    }

    /// Adds one page's renderer needs to those of the whole site.
    pub(crate) fn include(&mut self, other: &Self) {
        self.math |= other.math;
        self.mermaid |= other.mermaid;
    }
}

/// Appends CommonMark under the containing heading (SDOC-7). The link
/// callback only returns destinations present in the generated site.
pub(crate) fn render(
    output: &mut Markup,
    text: &str,
    parent_level: usize,
    link: impl Fn(&str) -> Option<String>,
) {
    render_section(output, text, parent_level, false, link);
}

/// HGL fences in a recognized Examples section can run as tests.
pub(crate) fn render_section(
    output: &mut Markup,
    text: &str,
    parent_level: usize,
    in_examples: bool,
    link: impl Fn(&str) -> Option<String>,
) {
    let mut source = Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter();
    let mut events = Vec::new();
    let mut labels = Vec::new();
    let mut label_destinations = Vec::new();
    while let Some((event, range)) = source.next() {
        let event = match event {
            Event::Start(Tag::Heading { level, .. }) => {
                Event::Html(heading_start(parent_level + level as usize).into())
            }
            Event::End(TagEnd::Heading(level)) => {
                Event::Html(heading_end(parent_level + level as usize).into())
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info)))
                if matches!(&*info, "hgl" | "hgl no-run" | "math" | "mermaid") =>
            {
                let mut code = String::new();
                for (event, _) in source.by_ref() {
                    match event {
                        Event::End(TagEnd::CodeBlock) => break,
                        Event::Text(text) => code.push_str(&text),
                        _ => {}
                    }
                }
                let block = match &*info {
                    "math" => {
                        output.renderers.math = true;
                        format!(
                            "<pre><code data-renderer=\"math\" class=\"language-math\">{}</code></pre>\n",
                            escaped(&code)
                        )
                    }
                    "mermaid" => {
                        output.renderers.mermaid = true;
                        format!(
                            "<pre><code data-renderer=\"mermaid\" class=\"language-mermaid\">{}</code></pre>\n",
                            escaped(&code)
                        )
                    }
                    _ => {
                        let (status, label) = if in_examples && info.as_ref() == "hgl" {
                            ("runnable", "Runnable example")
                        } else {
                            ("shown", "Shown example")
                        };
                        format!(
                            "<figure data-example=\"{status}\"><figcaption>{label}</figcaption>\n<pre><code class=\"language-hgl\">{}</code></pre></figure>\n",
                            highlight(&code)
                        )
                    }
                };
                Event::Html(block.into())
            }
            Event::Start(Tag::HtmlBlock) => Event::Html("<pre class=\"authored-html\">".into()),
            Event::End(TagEnd::HtmlBlock) => Event::Html("</pre>\n".into()),
            Event::Html(markup) | Event::InlineHtml(markup) => Event::Text(markup),
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let linked = labels.is_empty()
                    && dest_url.split_once(':').is_some_and(|(scheme, _)| {
                        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
                    });
                labels.push((dest_url.clone(), linked));
                if !linked {
                    continue;
                }
                Event::Start(Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                })
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                labels.push((dest_url, false));
                continue;
            }
            Event::End(TagEnd::Link | TagEnd::Image) => {
                let (destination, linked) = labels.pop().expect("the label starts before it ends");
                if linked {
                    events.push(Event::End(TagEnd::Link));
                }
                // An image in a link label can add a destination. Wait for
                // the outer label to close so every displayed URL is inert.
                label_destinations.push(destination);
                if labels.is_empty() {
                    for destination in label_destinations.drain(..) {
                        events.push(Event::InlineHtml(
                            format!(" (<code>{}</code>)", escaped(&destination)).into(),
                        ));
                    }
                }
                continue;
            }
            Event::Code(name) if formula(text, &range) => {
                output.renderers.math = true;
                Event::InlineHtml(
                    format!("<code class=\"formula-inline\">{}</code>", escaped(&name)).into(),
                )
            }
            // An authored label may contain code, but adding an anchor
            // inside that label can make a nested link.
            Event::Code(name) if labels.is_empty() => match link(&name) {
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
    html::push_html(&mut output.html, events.into_iter());
}

/// Dollar-delimited code spans belong to a formula (SDOC-7), even if the
/// formula text happens to be an in-scope declaration name.
fn formula(text: &str, range: &Range<usize>) -> bool {
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

//! The layout of SDOC comments on the lossless tree. The analyser checks
//! names in this layout and the documentation stage publishes its text.

use std::ops::Range;

use hgl_source::{Span, line_break_length};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};

use crate::{HasSpan, SyntaxKind, SyntaxNode, SyntaxToken};

mod examples;

pub use examples::{RunnableExample, parse_examples};

/// The comment attached to this node by the parser (SDOC-1).
pub fn attached(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.children_with_tokens()
        .next()
        .and_then(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::DocComment)
}

const HEADINGS: [&str; 10] = [
    "Args:",
    "Type Args:",
    "Returns:",
    "Raises:",
    "Ticks:",
    "Validity:",
    "Properties:",
    "Requires:",
    "Notes:",
    "Examples:",
];

#[derive(Clone, Copy)]
struct Line<'a> {
    text: &'a str,
    start: u32,
}

impl Line<'_> {
    fn blank(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// SDOC-2's normalized text and SDOC-3's ordered sections and prose.
pub struct Comment<'a> {
    lines: Vec<Line<'a>>,
    sections: Vec<Section<'a>>,
}

impl<'a> Comment<'a> {
    /// Reads a documentation comment token, retaining spans for diagnostics.
    pub fn read(comment: &'a SyntaxToken) -> Self {
        let text = comment.text();
        let end = if text.len() >= 5 && text.ends_with("*/") {
            text.len() - 2
        } else {
            text.len()
        };
        let inner = text.get(3..end).unwrap_or("");
        let mut lines = comment_lines(inner, comment.span().start + 3);
        // Text beside the opening delimiter is a summary, independent
        // of the indentation and headings of the lines that follow.
        let (first, body) = lines.split_first_mut().expect("a comment has a first line");
        first.text = first.text.trim_start_matches([' ', '\t']);
        if body.is_empty() {
            first.text = first.text.trim_end_matches([' ', '\t']);
        }
        let common = body
            .iter()
            .filter(|line| !line.blank())
            .map(|line| indent(line.text))
            .min()
            .unwrap_or(0);
        for line in body.iter_mut() {
            let cut = indent(line.text).min(common);
            line.text = &line.text[cut..];
            line.start += cut as u32;
        }
        let normalized = joined(&lines, |line| line.text);
        let mut sections = vec![Section {
            heading: None,
            lines: vec![lines[0]],
        }];
        for (&line, fenced) in lines[1..].iter().zip(fenced_lines(&normalized).skip(1)) {
            let current = sections.last_mut().expect("a comment has a summary");
            if !fenced && !line.blank() && indent(line.text) == 0 {
                if let Some(heading) = HEADINGS
                    .iter()
                    .copied()
                    .find(|heading| *heading == line.text.trim_end())
                {
                    sections.push(Section {
                        heading: Some(heading),
                        lines: Vec::new(),
                    });
                    continue;
                }
                if current.heading.is_some() {
                    sections.push(Section {
                        heading: None,
                        lines: vec![line],
                    });
                    continue;
                }
            }
            current.lines.push(line);
        }
        Comment { lines, sections }
    }

    /// All text in source order, including headings, after SDOC-2 normalization.
    pub fn text(&self) -> String {
        joined(trim_blank(&self.lines), |line| line.text)
    }

    /// Initial unheaded prose, including a summary beside the opening delimiter.
    pub fn summary(&self) -> String {
        self.sections[0].text()
    }

    /// The recognized sections and any intervening unheaded prose, in order.
    pub fn sections(&self) -> impl Iterator<Item = &Section<'a>> {
        self.sections.iter().skip(1)
    }

    /// The HGL fences in the comment (SDOC-7, SDOC-9). Only a fence
    /// marked `hgl` under an Examples section is a runnable test.
    pub fn examples(&self) -> Vec<Example> {
        let mut position = 0;
        let mut examples = Vec::new();
        for section in &self.sections {
            let in_examples = section.heading == Some("Examples:");
            let lines = section.markdown_lines();
            let mut offset = 0;
            let starts: Vec<_> = lines
                .iter()
                .map(|line| {
                    let start = offset;
                    offset += line.text.len() + 1;
                    start
                })
                .collect();
            let text = joined(&lines, |line| line.text);
            let mut markdown = Parser::new_ext(&text, Options::ENABLE_TABLES).into_offset_iter();
            while let Some((event, fence)) = markdown.next() {
                let Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) = event else {
                    continue;
                };
                if !matches!(info.as_ref(), "hgl" | "hgl no-run") {
                    continue;
                }
                let position = in_examples.then(|| {
                    position += 1;
                    position
                });
                let mut code = String::new();
                let mut source = Vec::new();
                for (event, range) in markdown.by_ref() {
                    match event {
                        Event::End(pulldown_cmark::TagEnd::CodeBlock) => break,
                        Event::Text(text) => {
                            code.push_str(&text);
                            source.extend(source_ranges(&lines, &starts, range));
                        }
                        _ => {}
                    }
                }
                examples.push(Example {
                    code,
                    position,
                    runnable: in_examples && info.as_ref() == "hgl",
                    fence: Span::new(
                        source_offset(&lines, &starts, fence.start),
                        source_offset(&lines, &starts, fence.end),
                    ),
                    source,
                });
            }
        }
        examples
    }
}

/// One recognized SDOC-3 section or a stretch of unheaded prose.
pub struct Section<'a> {
    heading: Option<&'static str>,
    lines: Vec<Line<'a>>,
}

impl<'a> Section<'a> {
    /// The section's heading, including its colon. Unheaded prose has none.
    pub fn heading(&self) -> Option<&'static str> {
        self.heading
    }

    /// The section's contents, with its four-space margin removed.
    pub fn text(&self) -> String {
        joined(trim_blank(&self.lines), |line| {
            if self.heading.is_some() {
                without_margin(line.text)
            } else {
                line.text
            }
        })
    }

    /// Section text in Markdown coordinates and each line's original start.
    fn markdown_lines(&self) -> Vec<Line<'a>> {
        trim_blank(&self.lines)
            .iter()
            .map(|line| {
                let text = if self.heading.is_some() {
                    without_margin(line.text)
                } else {
                    line.text
                };
                Line {
                    text,
                    start: line.start + (line.text.len() - text.len()) as u32,
                }
            })
            .collect()
    }

    /// Keys whose spelling SDOC-3 defines, with their original source spans.
    /// The spelling of requirements and property keys is still unspecified.
    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> {
        self.entry_lines().map(|(_, entry)| entry)
    }

    /// The Markdown outside parameter entries. The HTML renderer shows
    /// entries in a table while retaining the section's surrounding prose.
    pub fn prose(&self) -> String {
        let entries = self
            .entry_lines()
            .map(|(line, entry)| line..line + 1 + entry.continuation_length());
        let mut entries = entries.peekable();
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                while entries.peek().is_some_and(|entry| index >= entry.end) {
                    entries.next();
                }
                (!entries.peek().is_some_and(|entry| entry.contains(&index)))
                    .then_some(without_margin(line.text))
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim_matches('\n')
            .to_owned()
    }

    fn entry_lines(&self) -> impl Iterator<Item = (usize, Entry<'_>)> {
        let mut literal = Vec::new();
        if matches!(self.heading, Some("Args:" | "Type Args:")) {
            // The four spaces that delimit the section are outside its
            // Markdown. Use their original lines to recover key spans.
            let normalized = joined(&self.lines, |line| without_margin(line.text));
            literal.extend(fenced_lines(&normalized));
        }
        self.lines
            .iter()
            .enumerate()
            .zip(literal)
            .filter_map(|((index, line), fenced)| {
                if fenced || indent(line.text) != 4 {
                    return None;
                }
                let content = &line.text[4..];
                let length = content
                    .find(|letter: char| !(letter.is_alphanumeric() || letter == '_'))
                    .unwrap_or(content.len());
                let named = content
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_alphabetic() || first == '_');
                let value = content[length..].strip_prefix(':')?;
                named.then_some((
                    index,
                    Entry {
                        name: &content[..length],
                        span: Span::new(line.start + 4, line.start + 4 + length as u32),
                        value,
                        following: &self.lines[index + 1..],
                    },
                ))
            })
    }
}

/// An HGL code fence in a documentation comment (SDOC-7, SDOC-9).
pub struct Example {
    /// The contents read by CommonMark, without the fence.
    pub code: String,
    /// The position among the comment's `Examples:` fences, from one.
    /// A fence outside that section has no position.
    pub position: Option<usize>,
    /// Whether the compiler checks and runs this fence as a test.
    pub runnable: bool,
    /// The original fence, including its delimiters where present.
    pub fence: Span,
    /// The original code spans, one per line. Markdown margins are excluded.
    pub source: Vec<Span>,
}

/// Pulldown ranges index the joined Markdown. The line records retain the
/// original byte positions, including the margins and the source line breaks.
fn source_offset(lines: &[Line<'_>], starts: &[usize], offset: usize) -> u32 {
    let line = starts.partition_point(|&start| start <= offset) - 1;
    lines[line].start + (offset - starts[line]) as u32
}

fn source_ranges<'a>(
    lines: &'a [Line<'a>],
    starts: &'a [usize],
    range: Range<usize>,
) -> impl Iterator<Item = Span> + 'a {
    let first = starts.partition_point(|&start| start <= range.start) - 1;
    lines[first..]
        .iter()
        .zip(&starts[first..])
        .take_while(move |(_, start)| **start < range.end)
        .filter_map(move |(line, &start)| {
            let from = range.start.max(start);
            let to = range.end.min(start + line.text.len());
            (from < to).then(|| {
                Span::new(
                    line.start + (from - start) as u32,
                    line.start + (to - start) as u32,
                )
            })
        })
}

/// A named entry of `Args:` or `Type Args:`, with its description.
pub struct Entry<'a> {
    /// The name before the colon (LEX-9).
    pub name: &'a str,
    /// The original byte span of the name in the source file.
    pub span: Span,
    value: &'a str,
    following: &'a [Line<'a>],
}

impl Entry<'_> {
    /// The text after the colon, with any more deeply indented continuation.
    pub fn text(&self) -> String {
        let end = self.continuation_length();
        let mut text = self
            .value
            .strip_prefix(' ')
            .unwrap_or(self.value)
            .to_string();
        for line in trim_end_blank(&self.following[..end]) {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(without_margin(line.text));
        }
        text
    }

    fn continuation_length(&self) -> usize {
        self.following
            .iter()
            .position(|line| !line.blank() && indent(line.text) <= 4)
            .unwrap_or(self.following.len())
    }
}

/// Marks each LF-separated line touched by a CommonMark fenced code
/// block, including its delimiters (SDOC-7). The input is normalized
/// Markdown without any enclosing SDOC section's four-space margin.
pub fn fenced_lines(text: &str) -> impl Iterator<Item = bool> {
    let mut fences = Parser::new_ext(text, Options::ENABLE_TABLES)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            matches!(
                event,
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_)))
            )
            .then_some(range)
        })
        .peekable();
    let mut start = 0;
    text.split('\n').map(move |line| {
        let end = start + line.len();
        while fences.peek().is_some_and(|fence| fence.end <= start) {
            fences.next();
        }
        // A fenced blank line still belongs to the block.
        let fenced = fences.peek().is_some_and(|fence| fence.start <= end);
        start = end + 1;
        fenced
    })
}

fn comment_lines(text: &str, start: u32) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    let mut from = 0;
    let mut at = 0;
    while at < text.len() {
        let Some(length) = line_break_length(&text.as_bytes()[at..]) else {
            at += 1;
            continue;
        };
        lines.push(Line {
            text: &text[from..at],
            start: start + from as u32,
        });
        at += length;
        from = at;
    }
    lines.push(Line {
        text: &text[from..],
        start: start + from as u32,
    });
    lines
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

fn without_margin(line: &str) -> &str {
    line.strip_prefix("    ").unwrap_or(line)
}

fn trim_end_blank<'a, 'text>(lines: &'a [Line<'text>]) -> &'a [Line<'text>] {
    let end = lines
        .iter()
        .rposition(|line| !line.blank())
        .map_or(0, |last| last + 1);
    &lines[..end]
}

fn trim_blank<'a, 'text>(lines: &'a [Line<'text>]) -> &'a [Line<'text>] {
    let start = lines
        .iter()
        .position(|line| !line.blank())
        .unwrap_or(lines.len());
    trim_end_blank(&lines[start..])
}

fn joined<'text>(lines: &[Line<'text>], text: impl Fn(&Line<'text>) -> &'text str) -> String {
    lines.iter().map(text).collect::<Vec<_>>().join("\n")
}

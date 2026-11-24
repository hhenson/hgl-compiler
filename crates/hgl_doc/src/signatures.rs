//! Links names in written declaration headers (SDOC-4), using the same
//! analysed source scope as comment references. Strings, comments and
//! parameter keys stay literal; no signature text is reconstructed.

use std::collections::HashSet;

use hgl_diagnostics::Diagnostics;
use hgl_lexer::TokenKind;
use hgl_source::SourceMap;

use crate::markdown::escaped;

/// Render a checked header, preserving its spelling and whitespace.
pub(crate) fn render(text: &str, link: impl Fn(&str) -> Option<String>) -> String {
    let mut sources = SourceMap::new();
    let Ok(file) = sources.add("signature.hgl", text) else {
        return escaped(text);
    };
    let tokens = hgl_lexer::lex(&sources, file, &mut Diagnostics::new());
    let tokens: Vec<_> = tokens
        .iter()
        .filter(|token| !token.kind.is_trivia() && token.kind != TokenKind::Newline)
        .collect();
    // A retained block initializer can bind its own locals. They are
    // not module references even when a visible declaration shares a name.
    let locals: HashSet<_> = tokens
        .windows(2)
        .filter(|pair| {
            matches!(
                pair[0].kind,
                TokenKind::KwLet | TokenKind::KwVar | TokenKind::KwFor
            ) && pair[1].kind == TokenKind::Ident
        })
        .map(|pair| &text[pair[1].span.start as usize..pair[1].span.end as usize])
        .collect();
    let mut output = String::new();
    let mut rendered = 0;
    let mut index = 0;
    while index < tokens.len() {
        let first = tokens[index];
        if first.kind != TokenKind::Ident {
            index += 1;
            continue;
        }
        let start = first.span.start as usize;
        let mut end = first.span.end as usize;
        let mut name = text[start..end].to_string();
        let mut after = index + 1;
        while after + 1 < tokens.len()
            && matches!(tokens[after].kind, TokenKind::ColonColon | TokenKind::Dot)
            && tokens[after + 1].kind == TokenKind::Ident
        {
            name.push_str(tokens[after].kind.fixed_text().unwrap());
            let token = tokens[after + 1];
            name.push_str(&text[token.span.start as usize..token.span.end as usize]);
            end = token.span.end as usize;
            after += 2;
        }
        let binding = locals.contains(name.split(['.', ':']).next().unwrap_or_default())
            || tokens
                .get(after)
                .is_some_and(|token| token.kind == TokenKind::Colon)
            || index
                .checked_sub(1)
                .is_some_and(|before| tokens[before].kind == TokenKind::Dot);
        output.push_str(&escaped(&text[rendered..start]));
        if !binding && let Some(href) = link(&name) {
            output.push_str(&format!(
                "<a href=\"{}\">{}</a>",
                escaped(&href),
                escaped(&text[start..end]),
            ));
        } else {
            output.push_str(&escaped(&text[start..end]));
        }
        rendered = end;
        index = after;
    }
    output.push_str(&escaped(&text[rendered..]));
    output
}

//! Links names in written declaration headers (SDOC-4), using the same
//! analysed source scope as comment references. Strings, comments and
//! parameter keys stay literal; no signature text is reconstructed.

use std::collections::HashSet;

use hgl_diagnostics::Diagnostics;
use hgl_lexer::TokenKind;
use hgl_parser::ast::Item;
use hgl_parser::{HasSpan, SyntaxKind, SyntaxNode};
use hgl_source::{SourceMap, Span};

use crate::markdown::escaped;

/// Render a checked header, preserving its spelling and whitespace.
pub(crate) fn render(text: &str, link: impl Fn(&str) -> Option<String>) -> String {
    let mut sources = SourceMap::new();
    let Ok(file) = sources.add("signature.hgl", text) else {
        return escaped(text);
    };
    let tokens = hgl_lexer::lex(&sources, file, &mut Diagnostics::new());
    let mut header = tokens
        .iter()
        .filter(|token| !token.kind.is_trivia() && token.kind != TokenKind::Newline);
    // A field or enum member is retained without its enclosing type.
    // Parse it as a constant so its initializer keeps a syntax tree;
    // the synthetic prefix never enters the rendered text.
    let (file, tokens, prefix) = if header.next().is_some_and(|t| t.kind == TokenKind::Ident)
        && header
            .next()
            .is_some_and(|t| matches!(t.kind, TokenKind::Colon | TokenKind::Eq))
    {
        let Ok(file) = sources.add("member-signature.hgl", format!("const {text}")) else {
            return escaped(text);
        };
        let tokens = hgl_lexer::lex(&sources, file, &mut Diagnostics::new());
        (file, tokens, "const ".len())
    } else {
        (file, tokens, 0)
    };
    let text = sources.file(file).text();
    // Headers can omit their declaration's body. The total parser still
    // retains the complete anonymous functions within their initializers.
    let syntax = hgl_parser::parse(
        &sources,
        file,
        &tokens,
        hgl_parser::Entry::Declarations,
        hgl_parser::Limits::default(),
        &mut Diagnostics::new(),
    )
    .syntax();
    let anonymous = anonymous_bindings(&syntax);
    let declarations = declaration_bindings(&syntax);
    let locals = local_bindings(&syntax);
    let tokens: Vec<_> = tokens
        .iter()
        .filter(|token| !token.kind.is_trivia() && token.kind != TokenKind::Newline)
        .collect();
    let mut output = String::new();
    let mut rendered = prefix;
    let mut index = 0;
    while index < tokens.len() {
        let first = tokens[index];
        if first.kind != TokenKind::Ident || (first.span.start as usize) < prefix {
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
            || declarations.contains(&first.span)
            || anonymous.iter().any(|(bound, scope)| {
                bound == name.split(['.', ':']).next().unwrap_or_default()
                    && scope.start <= first.span.start
                    && first.span.end <= scope.end
            })
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

/// Binding sites stay plain for constants, callables and types alike;
/// only references in the remainder of a signature provide navigation.
fn declaration_bindings(syntax: &SyntaxNode) -> Vec<Span> {
    syntax
        .children()
        .filter_map(Item::cast)
        .filter_map(|item| match item {
            Item::Function(item) => item.name(),
            Item::NativeFunction(item) => item.name(),
            Item::Constant(item) => item.name(),
            Item::Struct(item) => item.name(),
            Item::Enum(item) => item.name(),
            Item::Operator(item) => item.name(),
            Item::NativeType(item) => item.name(),
            Item::Test(item) => item.name(),
            _ => None,
        })
        .map(|name| name.span())
        .collect()
}

/// EXPR-3/17, CTRL-12 and FN-19: read each binding pattern from its
/// syntax node, including both dictionary iteration names. Conservatively
/// leave these local spellings plain throughout the retained header.
fn local_bindings(syntax: &SyntaxNode) -> HashSet<String> {
    let mut bindings = HashSet::new();
    for node in syntax.descendants().filter(|node| {
        matches!(
            node.kind(),
            SyntaxKind::LocalDecl
                | SyntaxKind::ForStmt
                | SyntaxKind::ListComprehension
                | SyntaxKind::PackComprehension
        )
    }) {
        let tokens: Vec<_> = node
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .collect();
        // Only a ForStmt's pattern has a direct comma; expressions of
        // its iterable and body are child nodes, not tokens of the pattern.
        let count = if node.kind() == SyntaxKind::ForStmt
            && tokens.iter().any(|token| token.kind() == SyntaxKind::Comma)
        {
            2
        } else {
            1
        };
        for name in tokens
            .iter()
            .filter(|token| token.kind() == SyntaxKind::Ident)
            .take(count)
        {
            bindings.insert(name.text().to_string());
        }
    }
    bindings
}

/// FN-14: anonymous parameters bind their name and uses within their body,
/// including captured uses in nested functions, but not later expressions.
fn anonymous_bindings(syntax: &SyntaxNode) -> Vec<(String, Span)> {
    let mut bindings = Vec::new();
    for function in syntax
        .descendants()
        .filter(|node| node.kind() == SyntaxKind::FnExpr)
    {
        let body = function
            .children()
            .find(|node| matches!(node.kind(), SyntaxKind::ArrowBody | SyntaxKind::Block));
        let parameters = function
            .children()
            .find(|node| node.kind() == SyntaxKind::ParamList);
        for parameter in parameters
            .into_iter()
            .flat_map(|list| list.children())
            .filter(|node| node.kind() == SyntaxKind::LambdaParam)
        {
            let Some(name) = parameter
                .children_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|token| token.kind() == SyntaxKind::Ident)
            else {
                continue;
            };
            bindings.push((name.text().to_string(), name.span()));
            if let Some(body) = &body {
                bindings.push((name.text().to_string(), body.span()));
            }
        }
    }
    bindings
}

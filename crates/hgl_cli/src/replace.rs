//! What `hgl run` replaces in an application before it runs it.
//!
//! RUN-2: "What starts an application may replace an argument of its
//! entry or a field of its configuration with a constant checked
//! against the type it replaces, and what the application writes is the
//! default. It does not change the word of the run statement."
//!
//! `--arg upto=5` replaces an argument of the entry and `--config
//! length=10s` a field of the configuration. The value is HGL, one
//! expression. It is put in the text of the application's run
//! statement, in place of what the application wrote there or beside
//! it, and the program is then read and checked again as if the
//! application had written it. So a replacement is held to every rule
//! an argument written in the application is: its type, that it is a
//! constant, and the names it may use.
//!
//! ```text
//! eval(counted(upto: 3))                       the application
//! --arg upto=5 --config length=10s             what starts it
//! eval(counted(upto: (5)), RunConfig(length: (10s)))
//! ```

use hgl_analyser::hir::{Argument, Callee, ExprKind};
use hgl_analyser::{Detail, ParsedFile, Program};
use hgl_diagnostics::Diagnostics;
use hgl_lexer::{lex, literal};
use hgl_parser::{Entry, Limits, SyntaxKind, SyntaxNode};
use hgl_source::{FileId, SourceMap, Span};

/// One replacement, as the command line gives it: `upto=5`.
#[derive(Debug, Clone)]
pub(crate) struct Replacement {
    /// The parameter of the entry, or the field of the configuration.
    pub(crate) name: String,
    /// What to give it: HGL, one expression.
    pub(crate) value: String,
}

/// Reads `NAME=VALUE`. The command line's parser calls this for each
/// `--arg` and each `--config`, and prints the message of a failure.
pub(crate) fn replacement(written: &str) -> Result<Replacement, String> {
    // The first `=` ends the name: a value may hold one, `a == b`.
    let Some((name, value)) = written.split_once('=') else {
        return Err("a replacement is NAME=VALUE".to_string());
    };
    let (name, value) = (name.trim(), value.trim());
    let named = !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_');
    if !named {
        return Err(format!("`{name}` is no name of a parameter or a field"));
    }
    if value.is_empty() {
        return Err(format!("`{name}` is given no value"));
    }
    Ok(Replacement {
        name: name.to_string(),
        value: value.to_string(),
    })
}

/// Checks that `value` is one HGL expression on one line with no
/// comment: text that can stand between parentheses where an argument
/// is written and be that argument and nothing more.
fn one_expression(replacement: &Replacement, limits: Limits) -> Result<(), String> {
    let Replacement { name, value } = replacement;
    let mut sources = SourceMap::new();
    let file = sources
        .add(format!("the value of `{name}`"), value.as_str())
        .map_err(|error| error.to_string())?;
    let mut diagnostics = Diagnostics::new();
    let tokens = lex(&sources, file, &mut diagnostics);
    literal::validate_literals(&sources, file, &tokens, &mut diagnostics);
    let parse = hgl_parser::parse(
        &sources,
        file,
        &tokens,
        Entry::Statements,
        limits,
        &mut diagnostics,
    );
    if let Some(first) = diagnostics.as_slice().first() {
        return Err(format!(
            "the value of `{name}`, `{value}`, is not HGL: {}",
            first.message
        ));
    }
    let root = parse.syntax();
    let statements: Vec<SyntaxNode> = root.children().collect();
    let expression = statements.len() == 1 && statements[0].kind() == SyntaxKind::ExprStmt;
    // A line comment would take the rest of the line the value is put
    // on, and a line break could end a statement there.
    let one_line = root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .all(|token| {
            !matches!(
                token.kind(),
                SyntaxKind::Newline
                    | SyntaxKind::LineContinuation
                    | SyntaxKind::LineComment
                    | SyntaxKind::BlockComment
                    | SyntaxKind::DocComment
            )
        });
    if !expression || !one_line {
        return Err(format!(
            "the value of `{name}`, `{value}`, is not one expression"
        ));
    }
    Ok(())
}

/// What stands before an argument added at the `)` at `close`: nothing
/// after the `(` of an empty list or after a trailing comma, and a
/// comma otherwise.
fn separator(root: &SyntaxNode, close: u32) -> Result<&'static str, String> {
    let unread = || "the run statement could not be read for a replacement".to_string();
    let token = root
        .token_at_offset(close.into())
        .right_biased()
        .filter(|token| token.kind() == SyntaxKind::RParen)
        .ok_or_else(unread)?;
    let mut before = token.prev_token();
    while let Some(token) = &before {
        if !token.kind().is_trivia() {
            break;
        }
        before = token.prev_token();
    }
    match before.ok_or_else(unread)?.kind() {
        SyntaxKind::LParen | SyntaxKind::Comma => Ok(""),
        _ => Ok(", "),
    }
}

/// One list of arguments of the run statement that replacements are
/// made in: the entry's call, or the configuration's `RunConfig(...)`.
struct Written<'a> {
    /// What takes the arguments, to say in a message: "the entry
    /// `counted`".
    owner: String,
    /// What one of its names is: "parameter" or "field".
    what: &'static str,
    /// The names it has, in order: an [`Argument::parameter`] is a
    /// position among them.
    names: Vec<&'a str>,
    /// The arguments the application wrote.
    given: &'a [Argument],
    /// Where the `)` that ends the list is.
    close: u32,
}

impl Written<'_> {
    /// The place of `name` among the names, or the message that says
    /// there is none.
    fn place(&self, name: &str) -> Result<usize, String> {
        self.names
            .iter()
            .position(|known| *known == name)
            .ok_or_else(|| {
                let has = if self.names.is_empty() {
                    "none".to_string()
                } else {
                    self.names.join(", ")
                };
                format!("{} has no {} `{name}`: it has {has}", self.owner, self.what)
            })
    }

    /// The changes of text that make `replacements` in this list: the
    /// value of an argument the application wrote is replaced, and one
    /// it left out is added by name at the end.
    fn changes(
        &self,
        root: &SyntaxNode,
        replacements: &[Replacement],
        changes: &mut Vec<(Span, String)>,
    ) -> Result<(), String> {
        let mut added = Vec::new();
        for Replacement { name, value } in replacements {
            let place = self.place(name)?;
            let written = self
                .given
                .iter()
                .find(|argument| argument.parameter == Some(place));
            match written {
                Some(argument) => changes.push((argument.value.at.span, format!("({value})"))),
                None => added.push(format!("{name}: ({value})")),
            }
        }
        if !added.is_empty() {
            let separator = separator(root, self.close)?;
            changes.push((
                Span::empty(self.close),
                format!("{separator}{}", added.join(", ")),
            ));
        }
        Ok(())
    }
}

/// The text of the application `path` of `program` with its run
/// statement's arguments and configuration replaced, and the file the
/// text is of. On failure, the message to print: a name the entry or
/// the configuration has not, a name given twice, or a value that is
/// not one expression.
pub(crate) fn replaced(
    sources: &SourceMap,
    files: &[ParsedFile],
    program: &Program,
    path: &str,
    arguments: &[Replacement],
    config: &[Replacement],
    limits: Limits,
) -> Result<(FileId, String), String> {
    let unread = || format!("the run statement of `{path}` could not be read for a replacement");
    for replacements in [arguments, config] {
        for (position, replacement) in replacements.iter().enumerate() {
            one_expression(replacement, limits)?;
            if replacements[..position]
                .iter()
                .any(|earlier| earlier.name == replacement.name)
            {
                return Err(format!("`{}` is replaced twice", replacement.name));
            }
        }
    }
    let run = program
        .modules
        .iter()
        .find(|module| module.path == path)
        .and_then(|module| module.run_body.as_ref())
        .ok_or_else(unread)?;
    let file = run.at.file;
    let root = &files
        .iter()
        .find(|parsed| parsed.file == file)
        .ok_or_else(unread)?
        .root;
    let mut changes: Vec<(Span, String)> = Vec::new();

    // The entry's call: `counted(upto: 3)`.
    let ExprKind::Call {
        callee,
        arguments: given,
        ..
    } = &run.call.kind
    else {
        return Err(unread());
    };
    let (Callee::Function {
        module,
        declaration,
        ..
    }
    | Callee::Candidate {
        module,
        declaration,
        ..
    }) = callee
    else {
        return Err(unread());
    };
    let Some(Detail::Callable(signature)) = program.modules[*module].details.get(*declaration)
    else {
        return Err(unread());
    };
    let entry = Written {
        owner: format!(
            "the entry `{}`",
            program.modules[*module].declarations[*declaration].name
        ),
        what: "parameter",
        names: signature
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect(),
        given,
        close: run.call.at.span.end.saturating_sub(1),
    };
    entry.changes(root, arguments, &mut changes)?;

    // The configuration: `RunConfig(length: 10s)`, or none written.
    if !config.is_empty() {
        let fields = |module: usize, declaration: usize| -> Result<Vec<&str>, String> {
            match program.modules[module].details.get(declaration) {
                Some(Detail::Struct(of)) => Ok(of
                    .members
                    .iter()
                    .map(|member| member.name.as_str())
                    .collect()),
                _ => Err(unread()),
            }
        };
        match run.config.as_ref() {
            Some(written) => {
                let ExprKind::Construct {
                    module,
                    declaration,
                    arguments: given,
                } = &written.kind
                else {
                    return Err(format!(
                        "the configuration of `{path}` is not written as `RunConfig(...)`: \
                         a field of one that is named or computed is not replaced yet"
                    ));
                };
                let configuration = Written {
                    owner: "the configuration `RunConfig`".to_string(),
                    what: "field",
                    names: fields(*module, *declaration)?,
                    given,
                    close: written.at.span.end.saturating_sub(1),
                };
                configuration.changes(root, config, &mut changes)?;
            }
            // "or `RunConfig()` where none is written" (RUN-1): one is
            // written here, with the fields given.
            None => {
                let core = program.core().ok_or_else(unread)?;
                let declaration = program.modules[core]
                    .declarations
                    .iter()
                    .position(|declared| declared.name == "RunConfig")
                    .ok_or_else(unread)?;
                let close = run.at.span.end.saturating_sub(1);
                let configuration = Written {
                    owner: "the configuration `RunConfig`".to_string(),
                    what: "field",
                    names: fields(core, declaration)?,
                    given: &[],
                    close,
                };
                let mut given = Vec::new();
                for Replacement { name, value } in config {
                    configuration.place(name)?;
                    given.push(format!("{name}: ({value})"));
                }
                changes.push((
                    Span::empty(close),
                    format!("{}RunConfig({})", separator(root, close)?, given.join(", ")),
                ));
            }
        }
    }

    // The changes are made from the last to the first, so that each is
    // at the place the text was read with.
    changes.sort_by_key(|(span, _)| std::cmp::Reverse((span.start, span.end)));
    let mut text = sources.file(file).text().to_string();
    for (span, with) in changes {
        text.replace_range(span.range(), &with);
    }
    Ok((file, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A module with an entry that takes two arguments and one that
    /// takes none.
    const TICKS: &str = concat!(
        "module demo.ticks\n",
        "export fn counted(const upto: i64, const every: duration = 1s) -> i64 {\n",
        "    cache count: i64 = 0\n",
        "    while count < upto {\n",
        "        count += 1\n",
        "        yield every: count\n",
        "    }\n",
        "}\n",
        "export fn once() -> i64 {\n",
        "    yield 1s: 1\n",
        "}\n",
    );

    /// The run statement of an application that ends with the line
    /// `statement`, once `arguments` and `config` are replaced in it:
    /// its last line. Or the message that says why they are not.
    fn rewritten(statement: &str, arguments: &[&str], config: &[&str]) -> Result<String, String> {
        let application = format!(
            "application demo.app\nuse demo.ticks::{{counted, once}}\n\
             const standard: RunConfig = RunConfig(length: 1m)\n{statement}\n"
        );
        let mut sources = SourceMap::new();
        let mut diagnostics = Diagnostics::new();
        let mut files = Vec::new();
        let program_files = [("ticks.hgl", TICKS), ("app.hgl", application.as_str())];
        for (name, text) in program_files.into_iter().chain(hgl_analyser::CORE_FILES) {
            let file = sources.add(name, text).unwrap();
            let tokens = lex(&sources, file, &mut diagnostics);
            literal::validate_literals(&sources, file, &tokens, &mut diagnostics);
            let parse = hgl_parser::parse(
                &sources,
                file,
                &tokens,
                Entry::SourceFile,
                Limits::DEFAULT,
                &mut diagnostics,
            );
            files.push(ParsedFile::new(file, parse.syntax()));
        }
        let program = hgl_analyser::analyse(&files, &mut diagnostics);
        assert!(
            !diagnostics.has_errors(),
            "{:?}",
            diagnostics.as_slice().first().map(|first| &first.message)
        );
        let read = |given: &[&str]| -> Vec<Replacement> {
            given
                .iter()
                .map(|given| replacement(given).unwrap())
                .collect()
        };
        replaced(
            &sources,
            &files,
            &program,
            "demo.app",
            &read(arguments),
            &read(config),
            Limits::DEFAULT,
        )
        .map(|(_, text)| text.lines().last().unwrap().to_string())
    }

    /// RUN-2: "replace an argument of its entry": the value the
    /// application wrote, by name or by position, is replaced, and an
    /// argument it left out is added by name.
    #[test]
    fn an_argument_is_replaced_where_it_is_written_or_added() {
        assert_eq!(
            rewritten("eval(counted(upto: 3))", &["upto=5"], &[]).unwrap(),
            "eval(counted(upto: (5)))"
        );
        assert_eq!(
            rewritten("eval(counted(3, 2s))", &["every=1s", "upto = 5"], &[]).unwrap(),
            "eval(counted((5), (1s)))"
        );
        assert_eq!(
            rewritten("eval(counted(3))", &["every=2s"], &[]).unwrap(),
            "eval(counted(3, every: (2s)))"
        );
        // After a trailing comma no comma is added.
        assert_eq!(
            rewritten("eval(counted(3, ))", &["every=2s"], &[]).unwrap(),
            "eval(counted(3, every: (2s)))"
        );
        // A value may hold an `=`, and names the application may use.
        assert_eq!(
            rewritten(
                "eval(counted(3))",
                &["upto=if 1 == 1 { 2 } else { 3 }"],
                &[]
            )
            .unwrap(),
            "eval(counted((if 1 == 1 { 2 } else { 3 })))"
        );
        // With none given the text is as it was.
        assert_eq!(
            rewritten("eval(counted(3))", &[], &[]).unwrap(),
            "eval(counted(3))"
        );
    }

    /// RUN-2: "or a field of its configuration". RUN-1: the
    /// configuration is "`RunConfig()` where none is written", so one
    /// is written with the fields given.
    #[test]
    fn a_field_of_the_configuration_is_replaced_or_added() {
        assert_eq!(
            rewritten("eval(counted(3))", &[], &["length=10s"]).unwrap(),
            "eval(counted(3), RunConfig(length: (10s)))"
        );
        assert_eq!(
            rewritten(
                "run(counted(3), RunConfig(length: 1m))",
                &["upto=4"],
                &["length=10s", "start=@2026-01-01T00:00:00Z"]
            )
            .unwrap(),
            "run(counted((4)), RunConfig(length: (10s), start: (@2026-01-01T00:00:00Z)))"
        );
        assert_eq!(
            rewritten(
                "eval(once(), RunConfig())",
                &[],
                &["end=@2026-01-01T00:00:00Z"]
            )
            .unwrap(),
            "eval(once(), RunConfig(end: (@2026-01-01T00:00:00Z)))"
        );
        // A configuration that is named: an argument is replaced, and a
        // field of it is not yet.
        assert_eq!(
            rewritten("eval(counted(3), standard)", &["upto=4"], &[]).unwrap(),
            "eval(counted((4)), standard)"
        );
        assert_eq!(
            rewritten("eval(counted(3), standard)", &[], &["length=10s"]).unwrap_err(),
            "the configuration of `demo.app` is not written as `RunConfig(...)`: \
             a field of one that is named or computed is not replaced yet"
        );
    }

    /// A replacement names a parameter of the entry or a field of the
    /// configuration, once, and its value is one expression.
    #[test]
    fn a_replacement_that_cannot_be_made_says_why() {
        for (arguments, config, message) in [
            (
                &["limit=4"][..],
                &[][..],
                "the entry `counted` has no parameter `limit`: it has upto, every",
            ),
            (
                &[],
                &["until=4"],
                "the configuration `RunConfig` has no field `until`: it has start, end, length",
            ),
            (&["upto=1", "upto=2"], &[], "`upto` is replaced twice"),
            // Text that would add an argument of its own.
            (
                &["upto=3, every: 2s"],
                &[],
                "the value of `upto`, `3, every: 2s`, is not HGL: expected a line break, found `,`",
            ),
            (
                &["upto=3 # three"],
                &[],
                "the value of `upto`, `3 # three`, is not one expression",
            ),
            (
                &["upto=let x = 3"],
                &[],
                "the value of `upto`, `let x = 3`, is not one expression",
            ),
        ] {
            assert_eq!(
                rewritten("eval(counted(3))", arguments, config).unwrap_err(),
                message
            );
        }
        assert_eq!(
            rewritten("eval(once())", &["upto=3"], &[]).unwrap_err(),
            "the entry `once` has no parameter `upto`: it has none"
        );
    }

    /// `NAME=VALUE`: the first `=` ends the name.
    #[test]
    fn a_replacement_is_a_name_and_a_value() {
        let read = replacement("flag = a == b").unwrap();
        assert_eq!(
            (read.name.as_str(), read.value.as_str()),
            ("flag", "a == b")
        );
        assert_eq!(
            replacement("upto").unwrap_err(),
            "a replacement is NAME=VALUE"
        );
        assert_eq!(
            replacement("=3").unwrap_err(),
            "`` is no name of a parameter or a field"
        );
        assert_eq!(
            replacement("a.b=3").unwrap_err(),
            "`a.b` is no name of a parameter or a field"
        );
        assert_eq!(
            replacement("upto=").unwrap_err(),
            "`upto` is given no value"
        );
    }
}

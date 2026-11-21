//! What `hgl run` replaces in an application before it runs it, and
//! the configuration it supplies the application's run.
//!
//! RUN-2: "What starts an application supplies its run's configuration
//! (RUN-6), and may replace an argument of its entry with a constant
//! checked against the type it replaces: what the application writes is
//! the default. It does not change the word of the run statement."
//! RUN-6: "the application gives the fields that leaves out their
//! defaults, `inject run_config{length: 3s}` ... An `end` or a `length`
//! that what starts an application gives replaces a default of either."
//!
//! `--arg upto=5` replaces an argument of the entry, and `--config
//! length=10s` supplies a field of the configuration: `start`, `end` or
//! `length`, core's `RunConfig`'s. The value is HGL, one expression. An
//! argument is put in the text of the application's run statement, in
//! place of what the application wrote there or beside it. A field is
//! put in the braces of the application's `inject run_config`, in place
//! of the default it gives the field, beside the others, or in braces
//! of its own; an `end` takes out a default `length` and a `length` a
//! default `end`. Where the application injects no `run_config`, one is
//! written before its run statement. The program is then read and
//! checked again as if the application had written it, so a value is
//! held to every rule one written in the application is: its type, that
//! it is value code, and the names it may use. `run_config` then reads
//! the configuration as supplied.
//!
//! ```text
//! inject run_config{length: 3s}                the application
//! eval counted(upto: 3)
//! --arg upto=5 --config end=@2026-01-02T00:00:00Z
//! inject run_config{end: (@2026-01-02T00:00:00Z)}
//! eval counted(upto: (5))
//! ```

use hgl_analyser::hir::{Argument, ExprKind};
use hgl_analyser::{Argument as GenericArgument, Detail, ParsedFile, Program, RUN_CONFIG, Type};
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
///
/// What is read is the value as it is put in the run statement,
/// between its parentheses: there `.buy`, a member of the enum the
/// place expects (TYPE-7), is an expression, which it is not where a
/// statement begins.
fn one_expression(replacement: &Replacement, limits: Limits) -> Result<(), String> {
    let Replacement { name, value } = replacement;
    let mut sources = SourceMap::new();
    let file = sources
        .add(format!("the value of `{name}`"), format!("({value})"))
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
            "the value of `{name}`, `{value}`, is not one HGL expression: {}",
            first.message
        ));
    }
    let root = parse.syntax();
    let statements: Vec<SyntaxNode> = root.children().collect();
    // The statement is the parentheses and what they hold, and no
    // more: `3) + (4` between them is a sum of two, and `3, 4` a tuple
    // (written `(3, 4)`, it is one value, a tuple).
    let expression = match statements.as_slice() {
        [statement] if statement.kind() == SyntaxKind::ExprStmt => {
            let parts: Vec<SyntaxNode> = statement.children().collect();
            parts.len() == 1 && parts[0].kind() == SyntaxKind::ParenExpr
        }
        _ => false,
    };
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
            "the value of `{name}`, `{value}`, is not one HGL expression"
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

/// The list of arguments of the run statement that replacements are
/// made in: the entry's call.
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

/// What an application runs: the entry its run statement calls, and
/// the generic arguments the call comes to.
///
/// RUN-2: a replacement is "a constant checked against the type it
/// replaces". The types of an entry's parameters are fixed by which
/// function the entry is and by its generic arguments, so a
/// replacement that leaves this as it was is checked against those
/// types, and one that changes it, `"text"` where `echo(value: 3)`
/// made `T` an `i64`, is of another type than it replaces.
///
/// Two are equal where they are one declaration with the same generic
/// arguments, each a type by its own declaration and not by its name:
/// two modules may each have a `Side`. The program is read twice from
/// the same files in the same order, so a declaration has one place in
/// both readings.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct EntryCall {
    /// The position of the entry's module in `Program::modules`.
    module: usize,
    /// The position of the entry in that module's declarations. Two
    /// candidates of one operator have one name, and two places.
    declaration: usize,
    /// The generic arguments the call comes to.
    generics: Vec<GenericArgument>,
}

impl EntryCall {
    /// The call of the application `path` of `program`. `None` where
    /// the program has no such application, or its run statement calls
    /// no function.
    pub(crate) fn of(program: &Program, path: &str) -> Option<EntryCall> {
        let run = program
            .modules
            .iter()
            .find(|module| module.path == path)?
            .run_body
            .as_ref()?;
        let ExprKind::Call { callee, .. } = &run.call.kind else {
            return None;
        };
        let (module, declaration) = callee.declaration()?;
        Some(EntryCall {
            module,
            declaration,
            generics: callee.generics().to_vec(),
        })
    }

    /// The entry as a message names it: `demo.sides::echo<i64>`. Two
    /// that are not equal may be named alike, where their generic
    /// arguments are types of one name.
    pub(crate) fn shown(&self, program: &Program) -> String {
        let declared = &program.modules[self.module];
        let written: Vec<String> = self
            .generics
            .iter()
            .map(|given| given.to_string())
            .collect();
        let generics = if written.is_empty() {
            String::new()
        } else {
            format!("<{}>", written.join(", "))
        };
        format!(
            "{}::{}{generics}",
            declared.path, declared.declarations[self.declaration].name
        )
    }
}

/// The generic arguments of an entry's call that the application left
/// to be inferred, as text to write after the entry's name, and where
/// that is: `<i64>` for `echo(value: 3)`. `None` where the call infers
/// none: the entry has no generic parameter, or the application wrote
/// its arguments.
///
/// A replacement is read in place of an argument, and the entry's
/// generic arguments may have been inferred from that argument. With
/// them written the replacement is read as the argument was expected:
/// `.sell` is a member of the `Side` the call made `T`, and `3` the
/// `f64` it made it (EXPR-5).
///
/// The text is each argument as a message writes it, which the
/// application's file may have no name for, or another meaning for:
/// the caller checks that the application reads as it did with the
/// text written, before it relies on it.
pub(crate) fn inferred_generics(
    files: &[ParsedFile],
    program: &Program,
    path: &str,
) -> Option<(u32, String)> {
    let run = program
        .modules
        .iter()
        .find(|module| module.path == path)?
        .run_body
        .as_ref()?;
    let entry = EntryCall::of(program, path)?;
    if entry.generics.is_empty() {
        return None;
    }
    let root = &files
        .iter()
        .find(|parsed| parsed.file == run.call.at.file)?
        .root;
    // `echo(value: 3)` is a call of a name. One that writes its
    // generic arguments, `echo<i64>(value: 3)`, is another kind of
    // node, and is left as it is.
    let call = root.descendants().find(|node| {
        let range = node.text_range();
        node.kind() == SyntaxKind::CallExpr
            && u32::from(range.start()) == run.call.at.span.start
            && u32::from(range.end()) == run.call.at.span.end
    })?;
    let name = call
        .children()
        .find(|node| node.kind() == SyntaxKind::NameExpr)?;
    let written: Vec<String> = entry
        .generics
        .iter()
        .map(|given| given.to_string())
        .collect();
    Some((
        u32::from(name.text_range().end()),
        format!("<{}>", written.join(", ")),
    ))
}

/// The text of the application `path` of `program` with its run
/// statement's arguments and configuration replaced, and the file the
/// text is of. `generics` is what [`inferred_generics`] gave, where the
/// entry's generic arguments are to be written too. On failure, the
/// message to print: a name the entry or the configuration has not, a
/// name given twice, or a value that is not one expression.
#[allow(clippy::too_many_arguments)]
pub(crate) fn replaced(
    sources: &SourceMap,
    files: &[ParsedFile],
    program: &Program,
    path: &str,
    arguments: &[Replacement],
    config: &[Replacement],
    generics: Option<&(u32, String)>,
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
    if let Some((after_name, written)) = generics {
        changes.push((Span::empty(*after_name), written.clone()));
    }

    // The entry's call: `counted(upto: 3)`.
    let ExprKind::Call {
        callee,
        arguments: given,
        ..
    } = &run.call.kind
    else {
        return Err(unread());
    };
    let Some((module, declaration)) = callee.declaration() else {
        return Err(unread());
    };
    let Some(Detail::Callable(signature)) = program.modules[module].details.get(declaration) else {
        return Err(unread());
    };
    let entry = Written {
        owner: format!(
            "the entry `{}`",
            program.modules[module].declarations[declaration].name
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

    // RUN-6: the configuration what starts the application supplies,
    // over the defaults of its `inject run_config{...}`.
    if !config.is_empty() {
        let fields = run_config_fields(program).ok_or_else(unread)?;
        for Replacement { name, .. } in config {
            if !fields.contains(&name.as_str()) {
                return Err(format!(
                    "the configuration `RunConfig` has no field `{name}`: it has {}",
                    fields.join(", ")
                ));
            }
        }
        let module = program
            .modules
            .iter()
            .find(|module| module.path == path)
            .ok_or_else(unread)?;
        changes.push(configuration_change(module, run.at.span.start, config)?);
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

/// The fields of core's `RunConfig`, the type of the capability
/// `run_config` (RUN-6, core's CAP-7), in the order core declares them.
/// `None` where the program has no core.
fn run_config_fields(program: &Program) -> Option<Vec<&str>> {
    let Type::Named {
        module,
        declaration,
        ..
    } = program.run_config_type()?
    else {
        return None;
    };
    match program.modules[module].details.get(declaration) {
        Some(Detail::Native(native)) => Some(
            native
                .properties
                .iter()
                .map(|property| property.name.as_str())
                .collect(),
        ),
        _ => None,
    }
}

/// The change of text that supplies `config` to the application
/// `module`'s run, `before` being where its run statement starts.
///
/// RUN-6: "what starts the application supplies its run's
/// configuration, and the application gives the fields that leaves out
/// their defaults, `inject run_config{length: 3s}` ... An `end` or a
/// `length` that what starts an application gives replaces a default of
/// either." The braces of the application's `inject run_config` are
/// written again: each default in the order written, a field supplied
/// in place of its default or after them, and a default `end` or
/// `length` left out where the other is supplied. Where the application
/// injects no `run_config`, an `inject run_config{...}` of the fields
/// supplied is written before the run statement.
fn configuration_change(
    module: &hgl_analyser::Module,
    before: u32,
    config: &[Replacement],
) -> Result<(Span, String), String> {
    // The application's `inject run_config`, and the braces written
    // after the name, where it has them.
    let injected = module.store.iter().find_map(|store| {
        let inject = store
            .syntax
            .children()
            .find(|node| node.kind() == SyntaxKind::InjectDecl)?;
        let mut elements = inject.children_with_tokens();
        elements.find(|element| {
            element.as_token().is_some_and(|token| {
                token.kind() == SyntaxKind::Ident && token.text() == RUN_CONFIG
            })
        })?;
        let name_end = elements
            .clone()
            .next()
            .map_or(u32::from(inject.text_range().end()), |next| {
                u32::from(next.text_range().start())
            });
        let braces = elements
            .find(|element| !element.kind().is_trivia())
            .and_then(|element| element.into_node())
            .filter(|node| node.kind() == SyntaxKind::OperatorConfig);
        Some((name_end, braces))
    });
    // The defaults written, each its field and the text of its value.
    let mut entries: Vec<(String, String)> = injected
        .as_ref()
        .and_then(|(_, braces)| braces.as_ref())
        .map(|braces| {
            braces
                .children()
                .filter(|entry| entry.kind() == SyntaxKind::ConfigArg)
                .filter_map(|entry| {
                    let name = entry
                        .children_with_tokens()
                        .filter_map(|element| element.into_token())
                        .find(|token| !token.kind().is_trivia())?;
                    let value = entry.children().next()?;
                    Some((name.text().to_string(), value.text().to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    for Replacement { name, value } in config {
        let value = format!("({value})");
        match entries.iter_mut().find(|(field, _)| field == name) {
            Some(entry) => entry.1 = value,
            None => entries.push((name.clone(), value)),
        }
    }
    let supplied = |field: &str| config.iter().any(|replacement| replacement.name == field);
    for (given, other) in [("end", "length"), ("length", "end")] {
        if supplied(given) && !supplied(other) {
            entries.retain(|(field, _)| field != other);
        }
    }
    let written: Vec<String> = entries
        .iter()
        .map(|(field, value)| format!("{field}: {value}"))
        .collect();
    let braces = format!("{{{}}}", written.join(", "));
    Ok(match injected {
        Some((_, Some(written))) => {
            let range = written.text_range();
            (
                Span::new(u32::from(range.start()), u32::from(range.end())),
                braces,
            )
        }
        Some((name_end, None)) => (Span::empty(name_end), braces),
        None => (
            Span::empty(before),
            format!("inject {RUN_CONFIG}{braces}\n"),
        ),
    })
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
        "export fn echo<T>(const value: T) -> T {\n",
        "    yield 1s: value\n",
        "}\n",
    );

    /// The statements of an application that ends with the lines
    /// `statements`, once `arguments` and `config` are replaced in them:
    /// each line after its `use` and its constant, joined by ` / `. Or
    /// the message that says why they are not.
    fn rewritten(statements: &str, arguments: &[&str], config: &[&str]) -> Result<String, String> {
        let application = format!(
            "application demo.app\nuse demo.ticks::{{counted, once, echo}}\n\
             const opening: datetime = @2026-01-05T09:30:00Z\n{statements}\n"
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
            inferred_generics(&files, &program, "demo.app").as_ref(),
            Limits::DEFAULT,
        )
        .map(|(_, text)| text.lines().skip(3).collect::<Vec<_>>().join(" / "))
    }

    /// RUN-2: "replace an argument of its entry": the value the
    /// application wrote, by name or by position, is replaced, and an
    /// argument it left out is added by name.
    #[test]
    fn an_argument_is_replaced_where_it_is_written_or_added() {
        assert_eq!(
            rewritten("eval counted(upto: 3)", &["upto=5"], &[]).unwrap(),
            "eval counted(upto: (5))"
        );
        assert_eq!(
            rewritten("eval counted(3, 2s)", &["every=1s", "upto = 5"], &[]).unwrap(),
            "eval counted((5), (1s))"
        );
        assert_eq!(
            rewritten("eval counted(3)", &["every=2s"], &[]).unwrap(),
            "eval counted(3, every: (2s))"
        );
        // After a trailing comma no comma is added.
        assert_eq!(
            rewritten("eval counted(3, )", &["every=2s"], &[]).unwrap(),
            "eval counted(3, every: (2s))"
        );
        // A value may hold an `=`, and names the application may use.
        assert_eq!(
            rewritten("eval counted(3)", &["upto=if 1 == 1 { 2 } else { 3 }"], &[]).unwrap(),
            "eval counted((if 1 == 1 { 2 } else { 3 }))"
        );
        // A member of the enum the place expects, and a tuple.
        assert_eq!(
            rewritten("eval counted(3)", &["upto=.most"], &[]).unwrap(),
            "eval counted((.most))"
        );
        assert_eq!(
            rewritten("eval counted(3)", &["upto=(1, 2)"], &[]).unwrap(),
            "eval counted(((1, 2)))"
        );
        // With none given the text is as it was.
        assert_eq!(
            rewritten("eval counted(3)", &[], &[]).unwrap(),
            "eval counted(3)"
        );
    }

    /// RUN-6: "what starts the application supplies its run's
    /// configuration, and the application gives the fields that leaves
    /// out their defaults, `inject run_config{length: 3s}`". A field
    /// supplied takes the place of its default or is written beside
    /// them; where the application injects no `run_config`, one is
    /// written before its run statement.
    #[test]
    fn a_field_of_the_configuration_is_supplied_over_the_defaults() {
        assert_eq!(
            rewritten("eval counted(3)", &[], &["length=10s"]).unwrap(),
            "inject run_config{length: (10s)} / eval counted(3)"
        );
        assert_eq!(
            rewritten(
                "inject run_config{length: 1m}\nrun counted(3)",
                &["upto=4"],
                &["length=10s", "start=@2026-01-01T00:00:00Z"]
            )
            .unwrap(),
            "inject run_config{length: (10s), start: (@2026-01-01T00:00:00Z)} / run counted((4))"
        );
        // A default the application gives stands where nothing is
        // supplied for its field, and may name what the application
        // may.
        assert_eq!(
            rewritten(
                "inject run_config{start: opening}\neval once()",
                &[],
                &["length=1s"]
            )
            .unwrap(),
            "inject run_config{start: opening, length: (1s)} / eval once()"
        );
        // `inject run_config` with no braces is given them.
        assert_eq!(
            rewritten(
                "inject run_config, global_state\nput(global_state, \"k\", 1)\neval once()",
                &[],
                &["start=opening"]
            )
            .unwrap(),
            "inject run_config{start: (opening)}, global_state / put(global_state, \"k\", 1) / \
             eval once()"
        );
    }

    /// RUN-6: "An `end` or a `length` that what starts an application
    /// gives replaces a default of either."
    #[test]
    fn an_end_or_a_length_supplied_replaces_a_default_of_either() {
        assert_eq!(
            rewritten(
                "inject run_config{start: opening, length: 3s}\neval once()",
                &[],
                &["end=@2026-01-06T00:00:00Z"]
            )
            .unwrap(),
            "inject run_config{start: opening, end: (@2026-01-06T00:00:00Z)} / eval once()"
        );
        assert_eq!(
            rewritten(
                "inject run_config{end: @2026-01-06T00:00:00Z}\neval once()",
                &[],
                &["length=2s"]
            )
            .unwrap(),
            "inject run_config{length: (2s)} / eval once()"
        );
        // Both supplied are both the run's, which refuses them
        // (`run.end_and_length`, core's CRUN-2).
        assert_eq!(
            rewritten(
                "inject run_config{length: 3s}\neval once()",
                &[],
                &["end=@2026-01-06T00:00:00Z", "length=2s"]
            )
            .unwrap(),
            "inject run_config{length: (2s), end: (@2026-01-06T00:00:00Z)} / eval once()"
        );
    }

    /// The generic arguments an entry's call infers are written for
    /// the second read, after the entry's name: a replacement is then
    /// read as the argument it replaces was expected. Those the
    /// application wrote are left as they are.
    #[test]
    fn inferred_generic_arguments_are_written() {
        assert_eq!(
            rewritten("eval echo(value: 3)", &["value=4"], &[]).unwrap(),
            "eval echo<i64>(value: (4))"
        );
        assert_eq!(
            rewritten(
                "inject run_config{length: 1m}\neval echo(2.5)",
                &["value=3"],
                &[]
            )
            .unwrap(),
            "inject run_config{length: 1m} / eval echo<f64>((3))"
        );
        assert_eq!(
            rewritten("eval echo<i64>(value: 3)", &["value=4"], &[]).unwrap(),
            "eval echo<i64>(value: (4))"
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
            (
                &[],
                &["length=1s", "length=2s"],
                "`length` is replaced twice",
            ),
            // Text that would add an argument of its own.
            (
                &["upto=3, every: 2s"],
                &[],
                "the value of `upto`, `3, every: 2s`, is not one HGL expression: expected `,` or `)`, found `:`",
            ),
            // A tuple with no parentheses of its own, and two values.
            (
                &["upto=3, 4"],
                &[],
                "the value of `upto`, `3, 4`, is not one HGL expression",
            ),
            (
                &["upto=3) + (4"],
                &[],
                "the value of `upto`, `3) + (4`, is not one HGL expression",
            ),
            (
                &["upto=3 # three"],
                &[],
                "the value of `upto`, `3 # three`, is not one HGL expression: expected `)`, found end of file",
            ),
            (
                &["upto=let x = 3"],
                &[],
                "the value of `upto`, `let x = 3`, is not one HGL expression: expected an expression, found `let`",
            ),
        ] {
            assert_eq!(
                rewritten("eval counted(3)", arguments, config).unwrap_err(),
                message
            );
        }
        assert_eq!(
            rewritten("eval once()", &["upto=3"], &[]).unwrap_err(),
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

//! The `hgl` command.
//!
//! Each subcommand runs a prefix of the compiler pipeline and reports what
//! came out. Only the first stage exists so far:
//!
//! ```text
//! hgl tokens <file>     source text --[lex]--> tokens, printed one per line
//! ```
//!
//! This file contains no compiler logic. It reads arguments, calls the
//! stage crates, and prints. Argument parsing is delegated to the `clap`
//! crate: the structs and enums below *are* the command-line grammar, and
//! `clap` generates the parser and the `--help` text from them and from
//! their doc comments.
//!
//! # Exit status
//!
//! | Status | Meaning                                                    |
//! | ------ | ---------------------------------------------------------- |
//! | 0      | The source was accepted.                                   |
//! | 1      | The source was rejected; diagnostics are on standard error. |
//! | 2      | The command could not run: bad arguments, unreadable file. |
//!
//! A file that is not UTF-8 is source the language rejects (LEX-16), so it
//! exits with status 1 and a diagnostic like any other rejection.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use hgl_diagnostics::{Diagnostics, render};
use hgl_lexer::{Token, TokenKind, lex, literal};
use hgl_source::SourceMap;

/// The source was rejected.
const EXIT_REJECTED: u8 = 1;
/// The command could not run.
const EXIT_FAILED: u8 = 2;

/// The whole command line. `#[derive(Parser)]` makes `Cli::parse()`
/// available, which reads the process arguments into this struct or prints
/// a usage error and exits with status 2.
#[derive(Parser)]
#[command(
    name = "hgl",
    version,
    about = "HGL compiler (experimental; makes no conformance claim)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// The subcommands. Each variant's doc comment is its `--help` summary.
#[derive(Subcommand)]
enum Command {
    /// Print the tokens of a source file, one per line.
    Tokens(TokensArgs),
}

/// Arguments of `hgl tokens`.
#[derive(Args)]
struct TokensArgs {
    /// The source file to lex.
    file: PathBuf,

    /// Also print whitespace and comments.
    #[arg(long)]
    trivia: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Tokens(arguments) => tokens(&arguments),
    }
}

/// `hgl tokens`: lexes a file and prints each token.
///
/// The output has one line per token:
///
/// ```text
/// 1:1    KwLet      "let"
/// 1:5    Ident      "window"
/// 1:12   Eq         "="
/// 1:14   Duration   "1h30m"     5400000000 us
/// ```
///
/// The columns are the token's `line:column`, its kind, its text, and, for
/// a literal, the value the compiler read from it.
fn tokens(arguments: &TokensArgs) -> ExitCode {
    let name = arguments.file.display().to_string();
    // The file is read as bytes, not as text: whether the bytes are UTF-8
    // is the lexer's question (LEX-16), answered with a diagnostic.
    let bytes = match fs::read(&arguments.file) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("hgl: cannot read {name}: {error}");
            return ExitCode::from(EXIT_FAILED);
        }
    };

    let mut sources = SourceMap::new();
    let file = match sources.add_bytes(name, bytes) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("hgl: {error}");
            return ExitCode::from(EXIT_FAILED);
        }
    };

    let mut diagnostics = Diagnostics::new();
    let tokens = lex(&sources, file, &mut diagnostics);
    literal::validate_literals(&sources, file, &tokens, &mut diagnostics);

    let source = sources.file(file);
    for token in &tokens {
        if token.kind.is_trivia() && !arguments.trivia {
            continue;
        }
        let position = source.line_col(token.span.start).to_string();
        // `{:?}` prints the debug form: for a kind its name, for text a
        // quoted string with line breaks and tabs shown as escapes.
        let kind = format!("{:?}", token.kind);
        let text = format!("{:?}", source.slice(token.span));
        match literal_value(token, source.slice(token.span)) {
            Some(value) => println!("{position:<8} {kind:<12} {text:<24} {value}"),
            None => println!("{position:<8} {kind:<12} {text}"),
        }
    }

    // The two passes report in their own order; show them in source order.
    diagnostics.sort_by_location();
    if let Err(error) = render::to_stderr(&sources, &diagnostics) {
        eprintln!("hgl: cannot write diagnostics: {error}");
        return ExitCode::from(EXIT_FAILED);
    }
    if diagnostics.has_errors() {
        ExitCode::from(EXIT_REJECTED)
    } else {
        ExitCode::SUCCESS
    }
}

/// The value of a literal token as text, or `None` for other tokens and for
/// literals that have no value (those are reported as diagnostics instead).
fn literal_value(token: &Token, text: &str) -> Option<String> {
    if token.malformed {
        return None;
    }
    match token.kind {
        // `.ok()?` gives the value on success and returns `None` from this
        // function on failure.
        TokenKind::Int => Some(literal::decode_integer(text).ok()?.to_string()),
        TokenKind::Float => Some(format!("{:?}", literal::decode_float(text).ok()?)),
        TokenKind::Str => Some(format!("{:?}", literal::decode_string(text).ok()?)),
        TokenKind::Duration => Some(format!("{} us", literal::decode_duration(text).ok()?)),
        TokenKind::Temporal => Some(temporal_value(&literal::decode_temporal(text).ok()?)),
        _ => None,
    }
}

/// The fields of a temporal literal, for example
/// `date 2026-09-03, time 09:30:00.000000, offset +01:00, zone Europe/London`.
fn temporal_value(value: &literal::TemporalLiteral<'_>) -> String {
    let mut fields = Vec::new();
    if let Some(date) = value.date {
        fields.push(format!(
            "date {:04}-{:02}-{:02}",
            date.year, date.month, date.day
        ));
    }
    if let Some(time) = value.time {
        fields.push(format!(
            "time {:02}:{:02}:{:02}.{:06}",
            time.hour, time.minute, time.second, time.microsecond
        ));
    }
    if let Some(offset) = value.offset {
        let sign = if offset.minutes < 0 { '-' } else { '+' };
        let minutes = offset.minutes.abs();
        fields.push(format!(
            "offset {sign}{:02}:{:02}",
            minutes / 60,
            minutes % 60
        ));
    }
    if let Some(zone) = value.zone {
        fields.push(format!("zone {zone}"));
    }
    fields.join(", ")
}

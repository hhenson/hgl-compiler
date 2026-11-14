//! The `hgl` command.
//!
//! Each subcommand runs a prefix of the compiler pipeline and reports what
//! came out. Two stages exist so far:
//!
//! ```text
//! hgl tokens <file>     source text --[lex]--> tokens, printed one per line
//! hgl tree <file>       tokens --[parse]--> syntax tree, printed as an outline
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
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use hgl_diagnostics::{Diagnostics, render};
use hgl_lexer::{Token, TokenKind, lex, literal};
use hgl_parser::{Entry, Limits};
use hgl_source::{FileId, SourceMap};

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
    /// Print the syntax tree of a source file.
    Tree(TreeArgs),
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

/// Arguments of `hgl tree`.
#[derive(Args)]
struct TreeArgs {
    /// The source file to parse.
    file: PathBuf,

    /// Parse a fragment that has no `module` line, as the examples in a
    /// document often have.
    #[arg(long, value_enum)]
    fragment: Option<Fragment>,

    // `flatten` puts the options of `LimitArgs` among this command's own.
    #[command(flatten)]
    limits: LimitArgs,
}

/// The compiler's limits, for every command that parses. They are the
/// compiler's and not the language's: source past one is reported, and the
/// option raises it.
#[derive(Args, Clone, Copy)]
struct LimitArgs {
    /// Report nesting of expressions, types and blocks deeper than this.
    #[arg(long, value_name = "LEVELS", default_value_t = Limits::DEFAULT.nesting)]
    nesting_limit: usize,

    /// Report a syntax tree deeper than this. A long chain such as
    /// `a + b + ...` or an `else if` ladder adds a level for each link.
    #[arg(long, value_name = "LEVELS", default_value_t = Limits::DEFAULT.tree_depth)]
    tree_depth_limit: usize,
}

impl LimitArgs {
    /// The options as the parser takes them.
    fn limits(self) -> Limits {
        Limits {
            nesting: self.nesting_limit,
            tree_depth: self.tree_depth_limit,
        }
    }
}

/// What a file given to `hgl tree --fragment` holds. `ValueEnum` makes the
/// variant names, in lower case, the values the option accepts.
#[derive(Clone, Copy, ValueEnum)]
enum Fragment {
    /// Declarations: functions, structs, tests and so on.
    Declarations,
    /// Statements, as inside a function body.
    Statements,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Tokens(arguments) => tokens(&arguments),
        Command::Tree(arguments) => {
            let stack = arguments.limits.limits().stack_size();
            // `move` gives the closure the arguments to keep, since it
            // runs on another thread.
            on_stack_of(stack, move || tree(&arguments))
        }
    }
}

/// Runs `work` on a thread of its own with `stack` bytes of stack, and
/// gives the exit status it returns.
///
/// Parsing uses the call stack in proportion to the nesting it is allowed
/// to read. The stack of a program's first thread is fixed by the system,
/// and small on some (1 MiB on Windows), so a command that parses runs on
/// a thread whose stack is sized for the limits it was given. That is what
/// lets the limits be raised.
fn on_stack_of(stack: usize, work: impl FnOnce() -> ExitCode + Send + 'static) -> ExitCode {
    match std::thread::Builder::new().stack_size(stack).spawn(work) {
        // `join` waits for the thread. If the thread panicked, the panic
        // carries on here as if there had been no thread.
        Ok(thread) => thread
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
        Err(error) => {
            eprintln!("hgl: cannot reserve {stack} bytes of stack for these limits: {error}");
            ExitCode::from(EXIT_FAILED)
        }
    }
}

/// Reads the file at `path` into a new source map.
///
/// On failure the reason has been printed and the exit status to end with
/// is returned. `Result<A, B>` is "an `A` on success or a `B` on failure".
fn load(path: &Path) -> Result<(SourceMap, FileId), ExitCode> {
    let name = path.display().to_string();
    // The file is read as bytes, not as text: whether the bytes are UTF-8
    // is the lexer's question (LEX-16), answered with a diagnostic.
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("hgl: cannot read {name}: {error}");
            return Err(ExitCode::from(EXIT_FAILED));
        }
    };
    let mut sources = SourceMap::new();
    match sources.add_bytes(name, bytes) {
        Ok(file) => Ok((sources, file)),
        Err(error) => {
            eprintln!("hgl: {error}");
            Err(ExitCode::from(EXIT_FAILED))
        }
    }
}

/// Prints the diagnostics, in source order, and gives the exit status
/// they call for.
fn report(sources: &SourceMap, mut diagnostics: Diagnostics) -> ExitCode {
    // The stages report in their own order; show them in source order.
    diagnostics.sort_by_location();
    if let Err(error) = render::to_stderr(sources, &diagnostics) {
        eprintln!("hgl: cannot write diagnostics: {error}");
        return ExitCode::from(EXIT_FAILED);
    }
    if diagnostics.has_errors() {
        ExitCode::from(EXIT_REJECTED)
    } else {
        ExitCode::SUCCESS
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
    let (sources, file) = match load(&arguments.file) {
        Ok(loaded) => loaded,
        Err(status) => return status,
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
    report(&sources, diagnostics)
}

/// `hgl tree`: lexes and parses a file and prints its syntax tree.
///
/// The output has one line for each node and each token, indented under
/// the node that holds it, with the range of bytes it covers:
///
/// ```text
/// SourceFile@0..24
///   ModuleDecl@0..23
///     KwModule@0..6 "module"
///     Whitespace@6..7 " "
/// ```
///
/// A tree is printed whatever the source is: the parser reports what the
/// grammar does not allow and keeps going.
fn tree(arguments: &TreeArgs) -> ExitCode {
    let (sources, file) = match load(&arguments.file) {
        Ok(loaded) => loaded,
        Err(status) => return status,
    };

    let mut diagnostics = Diagnostics::new();
    let tokens = lex(&sources, file, &mut diagnostics);
    literal::validate_literals(&sources, file, &tokens, &mut diagnostics);
    let entry = match arguments.fragment {
        None => Entry::SourceFile,
        Some(Fragment::Declarations) => Entry::Declarations,
        Some(Fragment::Statements) => Entry::Statements,
    };
    let limits = arguments.limits.limits();
    let parse = hgl_parser::parse(&sources, file, &tokens, entry, limits, &mut diagnostics);
    // `dump` ends every line with a line break, so `print!`, not `println!`.
    print!("{}", hgl_parser::dump(&parse.syntax()));
    report(&sources, diagnostics)
}

/// The value of a literal token as text, or `None` for other tokens and for
/// literals that have no value (those are reported as diagnostics instead).
fn literal_value(token: &Token, text: &str) -> Option<String> {
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

//! The `hgl` command.
//!
//! Each subcommand runs a prefix of the compiler pipeline and reports what
//! came out. Three stages exist so far:
//!
//! ```text
//! hgl tokens <file>     source text --[lex]--> tokens, printed one per line
//! hgl tree <file>       tokens --[parse]--> syntax tree, printed as an outline
//! hgl check <roots>     syntax trees --[analyse]--> program; its errors reported
//! hgl test <roots>      the program's cases, each checked; the outcome of each
//! ```
//!
//! `tokens` and `tree` read one file. `check` and `test` read a program:
//! every `.hgl` file under the source roots they are given.
//!
//! This file contains no compiler logic. It reads arguments, finds and
//! reads files, calls the stage crates, and prints. Argument parsing is delegated to the `clap`
//! crate: the structs and enums below *are* the command-line grammar, and
//! `clap` generates the parser and the `--help` text from them and from
//! their doc comments.
//!
//! # Exit status
//!
//! | Status | Meaning                                                    |
//! | ------ | ---------------------------------------------------------- |
//! | 0      | The source was accepted; every selected case passed.       |
//! | 1      | The source was rejected, or a case failed; the reasons are on standard error. |
//! | 2      | The command could not run: bad arguments, unreadable file, a selected case that nothing can run yet. |
//!
//! A file that is not UTF-8 is source the language rejects (LEX-16), so it
//! exits with status 1 and a diagnostic like any other rejection.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use hgl_analyser::{DeclarationKind, ImportKind, ParsedFile, Program};
use hgl_diagnostics::{Diagnostics, Severity, render};
use hgl_lexer::{Token, TokenKind, lex, literal};
use hgl_parser::{Entry, Limits};
use hgl_runner::{Case, CaseKind, Outcome, Report, Selection};
use hgl_source::{FileId, SourceMap};
use walkdir::WalkDir;

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
    /// Check a program: every `.hgl` file under the source roots.
    Check(CheckArgs),
    /// Run the cases of a program: its tests and its `# expect-error`
    /// rejection cases.
    Test(TestArgs),
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

/// Arguments of `hgl check`.
#[derive(Args)]
struct CheckArgs {
    /// Source roots. A directory is searched for `.hgl` files, its
    /// subdirectories included; a file is taken as it is. File and
    /// directory names carry no meaning: a file belongs to the module its
    /// `module` line names.
    #[arg(required = true, value_name = "ROOT")]
    roots: Vec<PathBuf>,

    /// Also print each module found: its files, imports, declarations
    /// and tests.
    #[arg(long)]
    modules: bool,

    #[command(flatten)]
    limits: LimitArgs,
}

/// Arguments of `hgl test`.
#[derive(Args)]
struct TestArgs {
    /// Source roots, as for `hgl check`.
    #[arg(required = true, value_name = "ROOT")]
    roots: Vec<PathBuf>,

    /// Run the test with this name only, given as `name` or
    /// `module.path::name`. May be repeated. A rejection case owned by a
    /// declaration or an import always runs.
    #[arg(long = "test", value_name = "NAME")]
    tests: Vec<String>,

    /// Check the rejection cases and run no test.
    #[arg(long)]
    rejections_only: bool,

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
        Command::Check(arguments) => {
            let stack = arguments.limits.limits().stack_size();
            on_stack_of(stack, move || check(&arguments))
        }
        Command::Test(arguments) => {
            let stack = arguments.limits.limits().stack_size();
            on_stack_of(stack, move || test(&arguments))
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
    let mut sources = SourceMap::new();
    // `?` returns the failure from this function when there is one.
    let file = load_into(&mut sources, path)?;
    Ok((sources, file))
}

/// Reads the file at `path` and adds it to `sources`. Fails as [`load`]
/// does.
fn load_into(sources: &mut SourceMap, path: &Path) -> Result<FileId, ExitCode> {
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
    match sources.add_bytes(name, bytes) {
        Ok(file) => Ok(file),
        Err(error) => {
            eprintln!("hgl: {error}");
            Err(ExitCode::from(EXIT_FAILED))
        }
    }
}

/// The source files of a program: each root that is a file, and every
/// `.hgl` file under each root that is a directory.
///
/// The compiler finds a module by the `module` line of its files, so the
/// names and the layout of the files mean nothing to it (owner decision,
/// 2026-11-13). The files come back in a fixed order, by name within each
/// directory, so that a run prints the same thing on every machine; the
/// analyser's result does not depend on the order (UNIT-1).
fn find_sources(roots: &[PathBuf]) -> Result<Vec<PathBuf>, ExitCode> {
    let mut found = Vec::new();
    // A file reached twice, through two roots or a link, is read once.
    // `canonicalize` gives a file's one true path, whatever path led to it.
    let mut seen = HashSet::new();
    let mut add = |path: PathBuf, found: &mut Vec<PathBuf>| {
        let identity = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.insert(identity) {
            found.push(path);
        }
    };
    for root in roots {
        if root.is_file() {
            add(root.clone(), &mut found);
            continue;
        }
        let before = found.len();
        // `follow_links` enters linked directories; the walker reports a
        // link that leads back into its own path and does not follow it.
        for entry in WalkDir::new(root).follow_links(true).sort_by_file_name() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    eprintln!("hgl: cannot read {}: {error}", root.display());
                    return Err(ExitCode::from(EXIT_FAILED));
                }
            };
            let is_source = entry.file_type().is_file()
                && entry.path().extension().is_some_and(|found| found == "hgl");
            if is_source {
                add(entry.into_path(), &mut found);
            }
        }
        if found.len() == before && !seen_under(root, &found) {
            eprintln!("hgl: no .hgl file under {}", root.display());
            return Err(ExitCode::from(EXIT_FAILED));
        }
    }
    Ok(found)
}

/// True when a file already found is under `root`: the root added
/// nothing because another root had covered it.
fn seen_under(root: &Path, found: &[PathBuf]) -> bool {
    let Ok(root) = fs::canonicalize(root) else {
        return false;
    };
    found
        .iter()
        .any(|path| fs::canonicalize(path).is_ok_and(|path| path.starts_with(&root)))
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

/// Finds, reads, lexes and parses every source file under `roots`.
///
/// Gives the sources, the parsed files in the order found, and what the
/// lexer and parser reported.
fn parse_program(
    roots: &[PathBuf],
    limits: Limits,
) -> Result<(SourceMap, Vec<ParsedFile>, Diagnostics), ExitCode> {
    let paths = find_sources(roots)?;
    let mut sources = SourceMap::new();
    let mut diagnostics = Diagnostics::new();
    let mut parsed = Vec::new();
    for path in &paths {
        let file = load_into(&mut sources, path)?;
        let tokens = lex(&sources, file, &mut diagnostics);
        literal::validate_literals(&sources, file, &tokens, &mut diagnostics);
        let parse = hgl_parser::parse(
            &sources,
            file,
            &tokens,
            Entry::SourceFile,
            limits,
            &mut diagnostics,
        );
        parsed.push(ParsedFile::new(file, parse.syntax()));
    }
    Ok((sources, parsed, diagnostics))
}

/// `hgl check`: lexes and parses every source file of a program, analyses
/// the program, and reports what is wrong with it.
///
/// On standard output, after the modules when `--modules` asks for them:
///
/// ```text
/// checked 3 files in 2 modules: no errors
/// ```
///
/// The analyser reads modules, declarations and imports and resolves
/// names so far. Types, phases and constants are not checked yet, and the
/// output says so.
fn check(arguments: &CheckArgs) -> ExitCode {
    let (sources, parsed, mut diagnostics) =
        match parse_program(&arguments.roots, arguments.limits.limits()) {
            Ok(program) => program,
            Err(status) => return status,
        };
    let program = hgl_analyser::analyse(&parsed, &mut diagnostics);

    if arguments.modules {
        print!("{}", list_modules(&sources, &program));
    }
    let errors = diagnostics
        .as_slice()
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .count();
    println!(
        "checked {} in {}: {}",
        count(parsed.len(), "file"),
        count(program.modules.len(), "module"),
        match errors {
            0 => "no errors".to_string(),
            errors => count(errors, "error"),
        }
    );
    println!("modules, declarations, imports and names are checked; types are not yet");
    print!("{}", list_prelude_names(&sources, &program));
    report(&sources, diagnostics)
}

/// `hgl test`: checks the cases of a program and prints the outcome of
/// each (TST-15).
///
/// ```text
/// passed   rejection   cases/rolling.hgl:4 fn invalid
/// FAILED   rejection   examples.rejection::bad_default
///     expected type/rolling.size_kind on line 9 of cases/rolling.hgl: not reported
/// not run  executable  examples.rejection::survives
/// rejection cases: 1 passed, 1 failed
/// executable cases: 1 not run: this compiler has no target to run a test on yet
/// ```
///
/// A rejection case is a declaration, import or test marked with
/// `# expect-error` annotations (TST-12). An executable case is a named
/// test; none can be run until the compiler has a target.
fn test(arguments: &TestArgs) -> ExitCode {
    let (sources, parsed, front_end) =
        match parse_program(&arguments.roots, arguments.limits.limits()) {
            Ok(program) => program,
            Err(status) => return status,
        };
    let selection = Selection {
        tests: arguments.tests.clone(),
        rejections_only: arguments.rejections_only,
    };
    let report = hgl_runner::run(&sources, &parsed, &front_end, &selection);

    // TST-14: invalid metadata and errors in the surviving source stop
    // the run before any case.
    if report.prevented() {
        for problem in &report.problems {
            match problem.at {
                Some(at) => {
                    let source = sources.file(at.file);
                    eprintln!(
                        "{}:{}: {}",
                        source.name(),
                        source.line_col(at.span.start),
                        problem.message
                    );
                }
                None => eprintln!("hgl: {}", problem.message),
            }
        }
        if let Err(error) = render::to_stderr(&sources, &report.surviving) {
            eprintln!("hgl: cannot write diagnostics: {error}");
            return ExitCode::from(EXIT_FAILED);
        }
        println!("no case was run");
        return ExitCode::from(EXIT_REJECTED);
    }

    for case in &report.cases {
        print_case(&sources, case);
    }
    print_counts(&report);
    if report.succeeded() {
        ExitCode::SUCCESS
    } else if report
        .cases
        .iter()
        .any(|case| matches!(case.outcome, Outcome::Failed { .. }))
    {
        ExitCode::from(EXIT_REJECTED)
    } else {
        // Nothing failed, and something could not be run.
        ExitCode::from(EXIT_FAILED)
    }
}

/// Prints one case: its outcome, kind and identity, and under a failed
/// one each annotation no error met and each error no annotation
/// expects (TST-13).
fn print_case(sources: &SourceMap, case: &Case) {
    let outcome = match case.outcome {
        Outcome::Passed => "passed",
        Outcome::Failed { .. } => "FAILED",
        Outcome::NotRun(_) => "not run",
    };
    let kind = match case.kind {
        CaseKind::Executable => "executable",
        CaseKind::Rejection => "rejection",
    };
    println!("{outcome:<8} {kind:<11} {}", case.identity);
    let Outcome::Failed {
        missing,
        unexpected,
    } = &case.outcome
    else {
        return;
    };
    for expectation in missing {
        println!(
            "    expected {}/{} on line {} of {}: not reported",
            expectation.category.as_str(),
            expectation.code,
            expectation.line + 1,
            sources.file(expectation.at.file).name()
        );
    }
    for diagnostic in unexpected {
        let source = sources.file(diagnostic.primary.file);
        let local = if diagnostic.code.is_catalogued() {
            ""
        } else {
            " (a compiler-local code, which no annotation can expect)"
        };
        println!(
            "    reported {}/{} at {}:{}, not expected{local}: {}",
            diagnostic.category.as_str(),
            diagnostic.code.as_str(),
            source.name(),
            source.line_col(diagnostic.primary.span.start),
            diagnostic.message
        );
    }
}

/// Prints how many cases of each kind passed, failed and were not run,
/// the two kinds apart (TST-14).
fn print_counts(report: &Report) {
    if report.cases.is_empty() {
        // TST-15: "A run with no cases supplies no conformance evidence."
        println!("no case was selected: this run shows nothing");
        return;
    }
    let passed = |outcome: &Outcome| *outcome == Outcome::Passed;
    let failed = |outcome: &Outcome| matches!(outcome, Outcome::Failed { .. });
    let not_run = |outcome: &Outcome| matches!(outcome, Outcome::NotRun(_));
    println!(
        "rejection cases: {} passed, {} failed",
        report.count(CaseKind::Rejection, passed),
        report.count(CaseKind::Rejection, failed)
    );
    let waiting = report.count(CaseKind::Executable, not_run);
    if waiting > 0 {
        println!(
            "executable cases: {waiting} not run: {}",
            hgl_runner::NO_TARGET
        );
    } else {
        println!("executable cases: none selected");
    }
}

/// `1 file`, `2 files`.
fn count(number: usize, noun: &str) -> String {
    let plural = if number == 1 { "" } else { "s" };
    format!("{number} {noun}{plural}")
}

/// The modules of a program as text, for `hgl check --modules`:
///
/// ```text
/// module pricing
///   file src/pricing.hgl
///   file src/curves.hgl part curves
///   use lib::{shared}
///   export temporal function price
///   test struct Fixture
///   test smoke
/// ```
///
/// A declaration is listed as what it is and its name, with `export`
/// before one that is exported and `test` before one declared in a test
/// context. A named test is listed as `test` and its name.
fn list_modules(sources: &SourceMap, program: &Program) -> String {
    let mut out = String::new();
    for module in &program.modules {
        out.push_str(&format!("module {}\n", module.path));
        for file in &module.files {
            let name = sources.file(file.file).name();
            match &file.part {
                Some(part) => out.push_str(&format!("  file {name} part {part}\n")),
                None => out.push_str(&format!("  file {name}\n")),
            }
        }
        for import in module.files.iter().flat_map(|file| &file.imports) {
            match import.kind {
                ImportKind::Selective => {
                    out.push_str(&format!("  use {}::{{{}}}\n", import.from, import.name));
                }
                ImportKind::Alias => {
                    out.push_str(&format!("  use {} as {}\n", import.from, import.name));
                }
            }
        }
        for declaration in &module.declarations {
            let export = if declaration.exported { "export " } else { "" };
            let test = if declaration.test_only { "test " } else { "" };
            out.push_str(&format!(
                "  {export}{test}{} {}",
                declaration.kind.describe(),
                declaration.name
            ));
            // An implementation says which module's operator it found.
            if let DeclarationKind::Implementation(Some(operator)) = declaration.kind {
                out.push_str(&format!(" of {}", program.modules[operator.module].path));
            }
            out.push('\n');
        }
        for test in &module.tests {
            out.push_str(&format!("  test {}\n", test.name));
        }
    }
    out
}

/// The names left for the prelude, as text; empty when there are none.
///
/// Every module sees the public declarations of core's mandatory module
/// without importing them (UNIT-11), and core has not declared it yet. A
/// name nothing else binds is therefore listed here, not reported.
fn list_prelude_names(sources: &SourceMap, program: &Program) -> String {
    let mut out = String::new();
    for name in program
        .modules
        .iter()
        .flat_map(|module| &module.prelude_names)
    {
        if out.is_empty() {
            out.push_str("left for the prelude (UNIT-11), which core has not declared yet:\n");
        }
        let source = sources.file(name.at.file);
        out.push_str(&format!(
            "  {}:{} {}\n",
            source.name(),
            source.line_col(name.at.span.start),
            name.name
        ));
    }
    out
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

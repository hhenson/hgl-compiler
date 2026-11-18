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
//! hgl emit <roots>      program --[transpile]--> files for a target, written
//! hgl run <roots>       an application of the program, run on a target
//! ```
//!
//! `tokens` and `tree` read one file. `check`, `test`, `emit` and `run`
//! read a program: every `.hgl` file under the source roots they are
//! given. `test --target` also runs the program's tests on that target.
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
//! | 0      | The source was accepted; every selected case passed; the application's run ended. |
//! | 1      | The source was rejected, a case failed, or the application's run failed; the reasons are on standard error. |
//! | 2      | The command could not run: bad arguments, unreadable file, a selected test or an application that was not run. |
//!
//! A file that is not UTF-8 is source the language rejects (LEX-16), so it
//! exits with status 1 and a diagnostic like any other rejection.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use hgl_analyser::{Detail, ImportKind, ParsedFile, Program};
use hgl_diagnostics::{Diagnostics, Severity, render};
use hgl_hgraph::{Ran, RanApplication, Stop};
use hgl_lexer::{Token, TokenKind, lex, literal};
use hgl_parser::ast::{self, AstNode};
use hgl_parser::{Entry, Limits};
use hgl_runner::{Case, CaseKind, Executor, Outcome, Report, Selection};
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
    /// Write a program for a target, into a directory.
    Emit(EmitArgs),
    /// Run an application of a program on a target, and print its
    /// output.
    Run(RunArgs),
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

    /// Run the integration tests too. Without this an
    /// `integration_test` is left out, unless `--test` names it.
    #[arg(long)]
    integration: bool,

    /// The target to run the tests on. With none, the tests are listed
    /// and not run.
    #[arg(long, value_enum)]
    target: Option<Target>,

    /// For `--target hgraph`: the Python interpreter to run the
    /// program with, one that has hgraph installed. `python3` when not
    /// given, or `python` on Windows.
    #[arg(long, value_name = "PATH", requires = "target")]
    python: Option<PathBuf>,

    /// Keep what the target wrote for the run in this directory. With
    /// none, it is written to a temporary directory and removed after
    /// the run.
    #[arg(long, value_name = "DIR", requires = "target")]
    keep: Option<PathBuf>,

    #[command(flatten)]
    limits: LimitArgs,
}

/// Arguments of `hgl emit`.
#[derive(Args)]
struct EmitArgs {
    /// Source roots, as for `hgl check`.
    #[arg(required = true, value_name = "ROOT")]
    roots: Vec<PathBuf>,

    /// The target to write for.
    #[arg(long, value_enum, default_value_t = Target::Hgraph)]
    target: Target,

    /// The directory to write into. It is made when it is not there,
    /// and a file already in it with a written file's name is replaced.
    #[arg(long, value_name = "DIR")]
    out: PathBuf,

    #[command(flatten)]
    limits: LimitArgs,
}

/// Arguments of `hgl run`.
#[derive(Args)]
struct RunArgs {
    /// Source roots, as for `hgl check`.
    #[arg(required = true, value_name = "ROOT")]
    roots: Vec<PathBuf>,

    /// The application to run, by its path: `examples.entry.backtest`.
    /// It may be left out where the program has one application.
    #[arg(long, value_name = "PATH")]
    application: Option<String>,

    /// The target to run it on.
    #[arg(long, value_enum, default_value_t = Target::Hgraph)]
    target: Target,

    /// For `--target hgraph`: the Python interpreter to run the
    /// program with, one that has hgraph installed. `python3` when not
    /// given, or `python` on Windows.
    #[arg(long, value_name = "PATH")]
    python: Option<PathBuf>,

    /// Keep what the target wrote for the run in this directory. With
    /// none, it is written to a temporary directory and removed after
    /// the run.
    #[arg(long, value_name = "DIR")]
    keep: Option<PathBuf>,

    #[command(flatten)]
    limits: LimitArgs,
}

/// What `hgl emit` writes for, and `hgl test` and `hgl run` run on.
/// Each is a transpiler crate of its own.
#[derive(Clone, Copy, ValueEnum)]
enum Target {
    /// Python wiring for hgraph.
    Hgraph,
}

/// The path of core's module (UNIT-11).
const CORE_MODULE: &str = Program::CORE;

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
        // A command that analyses needs the analyser's stack too, which
        // is the larger.
        Command::Check(arguments) => {
            let stack = analysis_stack(arguments.limits.limits());
            on_stack_of(stack, move || check(&arguments))
        }
        Command::Test(arguments) => {
            let stack = analysis_stack(arguments.limits.limits());
            on_stack_of(stack, move || test(&arguments))
        }
        Command::Emit(arguments) => {
            let stack = analysis_stack(arguments.limits.limits());
            on_stack_of(stack, move || emit(&arguments))
        }
        Command::Run(arguments) => {
            let stack = analysis_stack(arguments.limits.limits());
            on_stack_of(stack, move || run(&arguments))
        }
    }
}

/// The stack a command that analyses needs: the parser's or the
/// analyser's, whichever is the larger, for trees as deep as `limits`
/// admit. Core's module, when the compiler supplies it, is read under
/// the default limits, so the stack is for those at least.
fn analysis_stack(limits: Limits) -> usize {
    let deepest = Limits {
        nesting: limits.nesting.max(Limits::DEFAULT.nesting),
        tree_depth: limits.tree_depth.max(Limits::DEFAULT.tree_depth),
    };
    deepest.stack_size().max(hgl_analyser::stack_size(deepest))
}

/// Runs `work` on a thread of its own with `stack` bytes of stack, and
/// gives the exit status it returns.
///
/// Parsing and analysing use the call stack in proportion to the nesting
/// they are allowed to read. The stack of a program's first thread is
/// fixed by the system, and small on some (1 MiB on Windows), so a
/// command that parses runs on a thread whose stack is sized for the
/// limits it was given. That is what lets the limits be raised.
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
/// 2:12   Tenor      "1Y6M"      18 months, 0 days
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

/// A program read for checking.
struct Parsed {
    sources: SourceMap,
    /// The parsed files: those found under the roots, in the order
    /// found, and then core's when the compiler supplied them.
    files: Vec<ParsedFile>,
    /// What the lexer and parser reported.
    diagnostics: Diagnostics,
    /// How many of `files` are core's, supplied by the compiler.
    supplied: usize,
}

/// Finds, reads, lexes and parses every source file under `roots`, and
/// core's module when no file under the roots belongs to `module core`.
fn parse_program(roots: &[PathBuf], limits: Limits) -> Result<Parsed, ExitCode> {
    let paths = find_sources(roots)?;
    let mut sources = SourceMap::new();
    let mut diagnostics = Diagnostics::new();
    let mut files = Vec::new();
    let parse_file =
        |sources: &SourceMap, file: FileId, limits: Limits, diagnostics: &mut Diagnostics| {
            let tokens = lex(sources, file, diagnostics);
            literal::validate_literals(sources, file, &tokens, diagnostics);
            let parse = hgl_parser::parse(
                sources,
                file,
                &tokens,
                Entry::SourceFile,
                limits,
                diagnostics,
            );
            ParsedFile::new(file, parse.syntax())
        };
    for path in &paths {
        let file = load_into(&mut sources, path)?;
        files.push(parse_file(&sources, file, limits, &mut diagnostics));
    }
    // A program that brings its own `module core` is checked with that
    // one: this is how core itself is checked.
    let has_core = files.iter().any(|file| {
        ast::SourceFile::cast(file.root.clone())
            .and_then(|source| source.module_decl())
            .and_then(|module| module.path())
            .is_some_and(|path| path.dotted() == CORE_MODULE)
    });
    let mut supplied = 0;
    if !has_core {
        for (name, text) in hgl_analyser::CORE_FILES {
            let file = match sources.add(name, text) {
                Ok(file) => file,
                Err(error) => {
                    eprintln!("hgl: {error}");
                    return Err(ExitCode::from(EXIT_FAILED));
                }
            };
            // The limits given are for the program's own source. Core's
            // files are the compiler's, and are read under the default
            // ones whatever the program is held to.
            files.push(parse_file(
                &sources,
                file,
                Limits::DEFAULT,
                &mut diagnostics,
            ));
            supplied += 1;
        }
    }
    Ok(Parsed {
        sources,
        files,
        diagnostics,
        supplied,
    })
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
/// An application is counted apart: `in 2 modules and 1 application`.
///
/// The analyser reads modules, declarations and imports, resolves names
/// and reads the types written so far. What an expression is, phases and
/// constants are not checked yet, and the output says so.
fn check(arguments: &CheckArgs) -> ExitCode {
    let Parsed {
        sources,
        files: parsed,
        mut diagnostics,
        supplied,
    } = match parse_program(&arguments.roots, arguments.limits.limits()) {
        Ok(program) => program,
        Err(status) => return status,
    };
    let program = hgl_analyser::analyse(&parsed, &mut diagnostics);
    // The module the compiler supplied is not counted or listed: the
    // counts and the listing are of what the source roots hold. It is
    // in the program only when its files were read whole: under a
    // parser limit too small for them, no module is made of them.
    let supplied_modules = usize::from(supplied > 0 && program.module(CORE_MODULE).is_some());

    if arguments.modules {
        print!("{}", list_modules(&sources, &program, supplied > 0));
    }
    let of = |severity: Severity| {
        diagnostics
            .as_slice()
            .iter()
            .filter(|diagnostic| diagnostic.severity == severity)
            .count()
    };
    let (errors, warnings) = (of(Severity::Error), of(Severity::Warning));
    // A warning does not reject the source (ERR-1): it is counted
    // after the errors, where there is one.
    // UNIT-14: an application is counted apart from the modules.
    let applications = program
        .modules
        .iter()
        .filter(|module| module.application)
        .count();
    let modules = count(
        program
            .modules
            .len()
            .saturating_sub(supplied_modules + applications),
        "module",
    );
    println!(
        "checked {} in {}: {}{}",
        count(parsed.len().saturating_sub(supplied), "file"),
        match applications {
            0 => modules,
            applications => format!("{modules} and {}", count(applications, "application")),
        },
        match errors {
            0 => "no errors".to_string(),
            errors => count(errors, "error"),
        },
        match warnings {
            0 => String::new(),
            warnings => format!(", {}", count(warnings, "warning")),
        }
    );
    println!(
        "modules, declarations, imports, names, written types and the bodies of value functions, nodes, sources, graphs and tests are checked; no test is run"
    );
    report(&sources, diagnostics)
}

/// `hgl emit`: checks the program, and writes it for the target when
/// it is accepted and the target can write all of it. Nothing is
/// written otherwise: half a program is of no use to run.
fn emit(arguments: &EmitArgs) -> ExitCode {
    let Parsed {
        sources,
        files: parsed,
        mut diagnostics,
        ..
    } = match parse_program(&arguments.roots, arguments.limits.limits()) {
        Ok(program) => program,
        Err(status) => return status,
    };
    let program = hgl_analyser::analyse(&parsed, &mut diagnostics);
    if diagnostics.has_errors() {
        println!("nothing written: the program is rejected");
        return report(&sources, diagnostics);
    }
    let (name, files) = match arguments.target {
        Target::Hgraph => (
            hgl_hgraph::TARGET,
            hgl_hgraph::transpile(&program, &mut diagnostics).files,
        ),
    };
    if diagnostics.has_errors() {
        println!("nothing written: the {name} target does not write all of the program yet");
        return report(&sources, diagnostics);
    }
    if let Err(error) = hgl_hgraph::write(&files, &arguments.out) {
        eprintln!("hgl: cannot write to {}: {error}", arguments.out.display());
        return ExitCode::from(EXIT_FAILED);
    }
    println!(
        "wrote {} for {name} to {}",
        count(files.len(), "file"),
        arguments.out.display()
    );
    report(&sources, diagnostics)
}

/// `hgl run`: runs an application of a program on a target (UNIT-14,
/// RUN-1), and prints the run's output: a line for each publication of
/// the entry's result, its instant and then what was published.
///
/// ```text
/// 1970-01-01T00:00:00.000001Z  1970-01-01T00:00:00.000001Z
/// 1970-01-01T00:00:00.500001Z  1970-01-01T00:00:00.500001Z
/// ran examples.entry.backtest on hgraph: 2 publications
/// ```
///
/// What the program logs and what the target prints goes to standard
/// error, as the run goes. The output is printed to standard output
/// when the run has stopped: core's CRUN-8 has it as the recording
/// "once the run has stopped", and CRUN-6 gives a run that failed
/// none.
///
/// RUN-3: `eval` is a simulation, and `run` is in real time "until the
/// configuration's end or until its host stops it". An interrupt is
/// the host's stop: the run ends as it does at its end, and what it
/// recorded up to then is printed.
///
/// RUN-2's replacement of an argument or of a field of the
/// configuration by what starts the application is not written: the
/// application runs as it is written.
fn run(arguments: &RunArgs) -> ExitCode {
    let Parsed {
        sources,
        files: parsed,
        mut diagnostics,
        ..
    } = match parse_program(&arguments.roots, arguments.limits.limits()) {
        Ok(program) => program,
        Err(status) => return status,
    };
    let program = hgl_analyser::analyse(&parsed, &mut diagnostics);
    if diagnostics.has_errors() {
        println!("not run: the program is rejected");
        return report(&sources, diagnostics);
    }
    let Target::Hgraph = arguments.target;
    // What the target does not write yet is no error of the program:
    // it matters where the application needs it.
    let mut unwritten = Diagnostics::new();
    let output = hgl_hgraph::transpile(&program, &mut unwritten);
    let paths = || {
        let paths: Vec<&str> = output
            .applications
            .iter()
            .map(|application| application.path.as_str())
            .collect();
        paths.join(", ")
    };
    let chosen = match (&arguments.application, output.applications.as_slice()) {
        (_, []) => {
            eprintln!("hgl: the program has no application to run");
            return ExitCode::from(EXIT_FAILED);
        }
        (None, [_]) => 0,
        (None, several) => {
            eprintln!(
                "hgl: the program has {}; name one with `--application`: {}",
                count(several.len(), "application"),
                paths()
            );
            return ExitCode::from(EXIT_FAILED);
        }
        (Some(path), applications) => {
            match applications
                .iter()
                .position(|application| application.path == *path)
            {
                Some(found) => found,
                None => {
                    eprintln!(
                        "hgl: the program has no application `{path}`; it has: {}",
                        paths()
                    );
                    return ExitCode::from(EXIT_FAILED);
                }
            }
        }
    };
    let application = &output.applications[chosen];
    let name = hgl_hgraph::TARGET;
    if let Some(diagnostic) = &application.blocked {
        let source = sources.file(diagnostic.primary.file);
        eprintln!(
            "{}:{}: {}",
            source.name(),
            source.line_col(diagnostic.primary.span.start),
            diagnostic.message
        );
        println!(
            "not run: the {name} target does not write all that {} needs yet",
            application.path
        );
        return ExitCode::from(EXIT_FAILED);
    }

    // The directory the program is written to: the one to keep, or a
    // temporary one that is removed when `temporary` is dropped at the
    // end of this function.
    let temporary;
    let directory = match &arguments.keep {
        Some(directory) => directory.as_path(),
        None => match tempfile::Builder::new().prefix("hgl-run-").tempdir() {
            Ok(made) => {
                temporary = made;
                temporary.path()
            }
            Err(error) => {
                eprintln!("hgl: a temporary directory could not be made: {error}");
                return ExitCode::from(EXIT_FAILED);
            }
        },
    };
    let python = arguments
        .python
        .clone()
        .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "python" } else { "python3" }));
    // An interrupt or a request to end, of this process, is the host's
    // stop of the run (RUN-3). It is passed to the interpreter, which
    // one sent to this process alone does not reach, and this process
    // outlives it, to print what the run recorded. Where the handler
    // cannot be set the run goes on all the same, and an interrupt
    // then ends this process as it always did.
    let stop = Stop::new();
    let asked = stop.clone();
    let _ = ctrlc::set_handler(move || asked.ask());
    match hgl_hgraph::run_application(&python, directory, &output, chosen, &stop) {
        RanApplication::Stopped { output, by_host } => {
            let how = if by_host { " until it was stopped" } else { "" };
            match output {
                Some(published) => {
                    for (at, value) in &published {
                        println!("{at}  {value}");
                    }
                    println!(
                        "ran {} on {name}{how}: {}",
                        application.path,
                        count(published.len(), "publication")
                    );
                }
                None => println!(
                    "ran {} on {name}{how}: its entry has no result",
                    application.path
                ),
            }
            ExitCode::SUCCESS
        }
        RanApplication::Failed(message) => {
            eprintln!("hgl: {} failed: {message}", application.path);
            ExitCode::from(EXIT_REJECTED)
        }
        RanApplication::NotRun(reason) => {
            eprintln!("hgl: {} was not run: {reason}", application.path);
            ExitCode::from(EXIT_FAILED)
        }
    }
}

/// `hgl test`: checks the cases of a program and prints the outcome of
/// each (TST-15).
///
/// ```text
/// passed   rejection   cases/rolling.hgl:4 fn invalid
/// FAILED   rejection   examples.rejection::bad_default
///     expected type/rolling_window.size_kind on line 9 of cases/rolling.hgl: not reported
/// passed   executable  examples.rejection::survives
/// FAILED   executable  examples.rejection::wrong
///     cases/rolling.hgl:31:5: the assertion does not hold
/// not run  executable  examples.rejection::later
///     cases/rolling.hgl:40:18: the hgraph target does not write the type `i64?` yet
/// rejection cases: 1 passed, 1 failed
/// executable cases: 1 passed, 1 failed, 1 not run
/// ```
///
/// A rejection case is a declaration, import or test marked with
/// `# expect-error` annotations (TST-12). An executable case is a named
/// test. It is run on the target `--target` names, and listed as not
/// run when none is named:
///
/// ```text
/// not run  executable  examples.rejection::survives
/// executable cases: 1 not run: no target was given to run a test on
/// ```
fn test(arguments: &TestArgs) -> ExitCode {
    let Parsed {
        sources,
        files: parsed,
        diagnostics: front_end,
        ..
    } = match parse_program(&arguments.roots, arguments.limits.limits()) {
        Ok(program) => program,
        Err(status) => return status,
    };
    let selection = Selection {
        tests: arguments.tests.clone(),
        rejections_only: arguments.rejections_only,
        integration: arguments.integration,
    };
    // The target the tests run on, and where it writes the program.
    let mut on_hgraph = arguments.target.map(|Target::Hgraph| OnHgraph {
        sources: &sources,
        python: arguments
            .python
            .clone()
            .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "python" } else { "python3" })),
        keep: arguments.keep.as_deref(),
    });
    let executor = on_hgraph
        .as_mut()
        .map(|executor| executor as &mut dyn Executor);
    let report = hgl_runner::run(&sources, &parsed, &front_end, &selection, executor);

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
        .any(|case| matches!(case.outcome, Outcome::Failed { .. } | Outcome::FailedRun(_)))
    {
        ExitCode::from(EXIT_REJECTED)
    } else {
        // Nothing failed, and something could not be run.
        ExitCode::from(EXIT_FAILED)
    }
}

/// `hgl test --target hgraph`: what runs the executable cases of a
/// program on hgraph.
struct OnHgraph<'a> {
    /// The program's files, to say where a failure is.
    sources: &'a SourceMap,
    /// The Python interpreter to start.
    python: PathBuf,
    /// Where to keep what the target writes, when it is kept.
    keep: Option<&'a Path>,
}

impl Executor for OnHgraph<'_> {
    fn execute(&mut self, program: &Program, tests: &[String]) -> Vec<Outcome> {
        // What the target does not write yet is no error of the
        // program: it is why a test that needs it is not run.
        let mut unwritten = Diagnostics::new();
        let output = hgl_hgraph::transpile(program, &mut unwritten);
        let place = |at: hgl_diagnostics::Location| {
            let source = self.sources.file(at.file);
            format!("{}:{}", source.name(), source.line_col(at.span.start))
        };

        // Each test asked for, as its position among the target's
        // tests, and those of them that can be run.
        let found: Vec<Option<usize>> = tests
            .iter()
            .map(|identity| {
                output
                    .tests
                    .iter()
                    .position(|test| test.identity() == *identity)
            })
            .collect();
        let runnable: Vec<usize> = found
            .iter()
            .flatten()
            .copied()
            .filter(|&test| output.tests[test].blocked.is_none())
            .collect();

        // The directory the program is written to: the one to keep, or
        // a temporary one that is removed when `temporary` is dropped
        // at the end of this function.
        let temporary;
        let directory = match self.keep {
            Some(directory) => Ok(directory),
            None => match tempfile::Builder::new().prefix("hgl-test-").tempdir() {
                Ok(made) => {
                    temporary = made;
                    Ok(temporary.path())
                }
                Err(error) => Err(format!("a temporary directory could not be made: {error}")),
            },
        };
        let mut ran = match directory {
            Ok(directory) => hgl_hgraph::run(&self.python, directory, &output, &runnable),
            Err(reason) => vec![Ran::NotRun(reason); runnable.len()],
        }
        .into_iter();

        found
            .iter()
            .map(|test| {
                let Some(test) = test.map(|test| &output.tests[test]) else {
                    return Outcome::NotRun("the target wrote no such test".to_string());
                };
                if let Some(diagnostic) = &test.blocked {
                    return Outcome::NotRun(format!(
                        "{}: {}",
                        place(diagnostic.primary),
                        diagnostic.message
                    ));
                }
                match ran.next() {
                    Some(Ran::Passed) => Outcome::Passed,
                    Some(Ran::Failed { at, message }) => Outcome::FailedRun(match at {
                        Some(at) => format!("{}: {message}", place(at)),
                        None => message,
                    }),
                    Some(Ran::NotRun(reason)) => Outcome::NotRun(reason),
                    None => Outcome::NotRun("the target gave it no outcome".to_string()),
                }
            })
            .collect()
    }
}

/// Prints one case: its outcome, kind and identity. Under a failed
/// rejection case is each annotation no error met and each error no
/// annotation expects (TST-13); under an executable case that failed or
/// was not run on a target is what the target said.
fn print_case(sources: &SourceMap, case: &Case) {
    let outcome = match case.outcome {
        Outcome::Passed => "passed",
        Outcome::Failed { .. } | Outcome::FailedRun(_) => "FAILED",
        Outcome::NotRun(_) => "not run",
    };
    let kind = match case.kind {
        CaseKind::Executable => "executable",
        CaseKind::Rejection => "rejection",
    };
    println!("{outcome:<8} {kind:<11} {}", case.identity);
    let (missing, unexpected) = match &case.outcome {
        Outcome::Failed {
            missing,
            unexpected,
        } => (missing, unexpected),
        // With no target every test has the one reason, which the
        // counts give once.
        Outcome::FailedRun(said) | Outcome::NotRun(said) => {
            if said != hgl_runner::NO_TARGET {
                for line in said.lines() {
                    println!("    {line}");
                }
            }
            return;
        }
        Outcome::Passed => return,
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
    let ran = report.count(CaseKind::Executable, passed);
    let broke = report.count(CaseKind::Executable, |outcome| {
        matches!(outcome, Outcome::FailedRun(_))
    });
    let waiting = report.count(CaseKind::Executable, not_run);
    let untargeted = report.count(
        CaseKind::Executable,
        |outcome| matches!(outcome, Outcome::NotRun(reason) if reason == hgl_runner::NO_TARGET),
    );
    if ran + broke + waiting == 0 {
        println!("executable cases: none selected");
    } else if untargeted == waiting && ran + broke == 0 {
        println!(
            "executable cases: {waiting} not run: {}",
            hgl_runner::NO_TARGET
        );
    } else {
        // TST-14: "Report executed/rejected counts separately".
        println!("executable cases: {ran} passed, {broke} failed, {waiting} not run");
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
///   export temporal function price(quote: Quote, const depth: i64 = ...) -> f64
///   test struct Fixture
///   test smoke
/// ```
///
/// A declaration is listed as what it is and its name, with `export`
/// before one that is exported and `test` before one declared in a test
/// context. A function, operator or native function has its signature
/// after its name, with each type as the analyser read it: `_` is a type
/// that is not known. A named test is listed as `test` and its name.
fn list_modules(sources: &SourceMap, program: &Program, core_supplied: bool) -> String {
    let mut out = String::new();
    for module in &program.modules {
        // Core's module is listed when the program brings it, and not
        // when the compiler supplied it.
        if core_supplied && module.path == CORE_MODULE {
            continue;
        }
        // UNIT-14: an application is a unit of its own, headed by its
        // own word.
        let unit = if module.application {
            "application"
        } else {
            "module"
        };
        out.push_str(&format!("{unit} {}\n", module.path));
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
        // `zip` pairs each declaration with what was read of its types.
        for (declaration, detail) in module.declarations.iter().zip(&module.details) {
            let export = if declaration.exported { "export " } else { "" };
            let test = if declaration.test_only { "test " } else { "" };
            out.push_str(&format!(
                "  {export}{test}{} {}",
                declaration.kind.describe(),
                declaration.name
            ));
            if let Detail::Callable(signature) = detail {
                out.push_str(&signature.to_string());
            }
            // An implementation says which module's operator it found.
            if let Some((_, Some(operator))) = declaration.kind.candidate() {
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
        // LEX-28: the counts a tenor denotes.
        TokenKind::Tenor => Some(match literal::decode_tenor(text).ok()? {
            literal::Tenor::Moves { months, days } => format!("{months} months, {days} days"),
            literal::Tenor::BusinessDays(count) => format!("{count} business days"),
        }),
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

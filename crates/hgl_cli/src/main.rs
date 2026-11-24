//! The `hgl` command.
//!
//! Each subcommand runs a prefix of the compiler pipeline and reports what
//! came out. Four stages exist so far:
//!
//! ```text
//! hgl tokens <file>     source text --[lex]--> tokens, printed one per line
//! hgl tree <file>       tokens --[parse]--> syntax tree, printed as an outline
//! hgl check <roots>     syntax trees --[analyse]--> program; its errors reported
//! hgl doc <roots>       program --[document]--> JSON API model
//! hgl test <roots>      the program's cases, each checked; the outcome of each
//! hgl emit <roots>      program --[transpile]--> files for a target, written
//! hgl build <roots>     program --[transpile and build]--> native artifacts
//! hgl run <roots>       an application of the program, run on a target
//! ```
//!
//! `tokens` and `tree` read one file. `check`, `doc`, `test`, `emit` and `run`
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
//! On the C++ target, a real-time application run that stops cleanly after
//! an interrupt exits with status 0 and prints the publications recorded
//! before it stopped.
//!
//! A file that is not UTF-8 is source the language rejects (LEX-16), so it
//! exits with status 1 and a diagnostic like any other rejection.

mod check_json;
mod cpp;
mod doc;
mod replace;
#[cfg(all(test, unix))]
#[path = "../tests/common/executable.rs"]
mod test_executable;
mod test_paths;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use cpp::CaseOutcome;
use hgl_analyser::{Detail, ImportKind, ParsedFile, Program};
use hgl_diagnostics::{Diagnostics, Severity, render};
use hgl_lexer::{Token, TokenKind, lex, literal};
use hgl_parser::ast::{self, AstNode};
use hgl_parser::{Entry, HasSpan, Limits};
use hgl_runner::{CaseKind, Problem, Report, Selection};
use hgl_source::{FileId, SourceMap};
use replace::Replacement;
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
    version = concat!(env!("CARGO_PKG_VERSION"), " (hgraph ", env!("HGL_HGRAPH_TAG"), ")"),
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
    /// Write the API documentation of a checked program.
    Doc(doc::DocArgs),
    /// Run the cases of a program: its tests and its `# expect-error`
    /// rejection cases.
    Test(TestArgs),
    /// Write a program for a target, into a directory.
    Emit(EmitArgs),
    /// Build a program's units and executables without running them.
    Build(BuildArgs),
    /// Run an application of a program on a target, and print its
    /// output.
    Run(RunArgs),
    /// Internal process entry that passes the build lock to native jobs.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[command(hide = true)]
    NativeBuildChild {
        parent: u32,
        program: PathBuf,
        #[arg(last = true)]
        arguments: Vec<std::ffi::OsString>,
    },
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

    /// Write versioned JSON diagnostics with UTF-16 ranges to stdout.
    #[arg(long, conflicts_with = "modules")]
    json: bool,

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
    /// `module.path::name`. May be repeated. Declaration and import
    /// rejection cases are independent of this name filter.
    #[arg(long = "test", value_name = "NAME")]
    tests: Vec<String>,

    /// Select cases written in this file or directory, recursively.
    /// May be repeated. With --test, restrict those names to these paths.
    #[arg(long = "test-path", value_name = "PATH")]
    test_paths: Vec<PathBuf>,

    /// Check only the selected rejection cases.
    #[arg(long)]
    rejections_only: bool,

    /// Run the integration tests too. Without this an
    /// `integration_test` is left out, unless `--test` names it.
    #[arg(long, conflicts_with = "integration_only")]
    integration: bool,

    /// Run only integration tests and the rejection cases they own.
    #[arg(long)]
    integration_only: bool,

    /// The target to run the tests on. With none, the tests are listed
    /// and not run.
    #[arg(long, value_enum)]
    target: Option<Target>,

    /// Build the selected tests with native debug symbols. Print a JSON
    /// launch plan for the modules without starting any test. Reuse DIR
    /// on later calls.
    #[arg(long, value_name = "DIR", requires = "target", conflicts_with = "keep")]
    debug_build: Option<PathBuf>,

    /// Build the program in this directory, which keeps what the target
    /// wrote. With none, it is built in a directory under the cache
    /// for its source roots, which is kept for the next run.
    #[arg(long, visible_alias = "out", value_name = "DIR", requires = "target")]
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
    #[arg(long, value_enum, default_value_t = Target::Cpp)]
    target: Target,

    /// The directory to write into. It is made when it is not there,
    /// and a file already in it with a written file's name is replaced.
    #[arg(long, value_name = "DIR")]
    out: PathBuf,

    #[command(flatten)]
    limits: LimitArgs,
}

/// Arguments of `hgl build`.
#[derive(Args)]
struct BuildArgs {
    /// Source roots, as for `hgl check`.
    #[arg(required = true, value_name = "ROOT")]
    roots: Vec<PathBuf>,

    /// The target to build for.
    #[arg(long, value_enum, default_value_t = Target::Cpp)]
    target: Target,

    /// Build the test executables as well.
    #[arg(long)]
    tests: bool,

    /// Write compile_commands.json for the generated C++ in the workspace.
    /// Includes test entry units with --tests.
    #[arg(long)]
    compile_commands: bool,

    /// The build workspace, or the program's workspace under the cache.
    #[arg(long, visible_alias = "keep", value_name = "DIR")]
    out: Option<PathBuf>,

    /// Build with native debug symbols and the original HGL source paths.
    #[arg(long)]
    debug: bool,

    /// How to report the build.
    #[arg(long, value_enum, default_value_t = BuildFormat::Text)]
    format: BuildFormat,

    /// An application to describe or apply replacements to.
    #[arg(long, value_name = "PATH")]
    application: Option<String>,

    /// Replace an entry argument, as for `hgl run` (RUN-2).
    #[arg(long = "arg", value_name = "NAME=VALUE", value_parser = replace::replacement)]
    arguments: Vec<Replacement>,

    /// Supply run configuration, as for `hgl run` (RUN-6).
    #[arg(long = "config", value_name = "FIELD=VALUE", value_parser = replace::replacement)]
    config: Vec<Replacement>,

    #[command(flatten)]
    limits: LimitArgs,
}

#[derive(Clone, Copy, ValueEnum)]
enum BuildFormat {
    Text,
    Json,
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
    #[arg(long, value_enum, default_value_t = Target::Cpp)]
    target: Target,

    /// Replace an argument of the application's entry: `--arg upto=5`.
    /// The value is HGL, a constant of the parameter's type, and what
    /// the application writes is the default (RUN-2). It may be given
    /// more than once.
    #[arg(long = "arg", value_name = "NAME=VALUE", value_parser = replace::replacement)]
    arguments: Vec<Replacement>,

    /// Supply a field of the run's configuration, over the default the
    /// application's `inject run_config{...}` gives it (RUN-6):
    /// `--config length=10s`. The fields are `start`, `end` and
    /// `length`, core's `RunConfig`'s; the value is HGL, of the field's
    /// type, as an argument's is. An `end` or a `length` given replaces
    /// a default of either. It may be given more than once.
    #[arg(long = "config", value_name = "FIELD=VALUE", value_parser = replace::replacement)]
    config: Vec<Replacement>,

    /// Build with native debug symbols and print a JSON launch description.
    /// A fresh session directory under DIR holds the executable and generated C++.
    /// Debug symbols refer to the original HGL application files.
    /// The application is started by the debugger.
    #[arg(long, value_name = "DIR", conflicts_with = "keep")]
    debug_build: Option<PathBuf>,

    /// Build the application in this directory, which keeps what the
    /// target wrote. With none, it is built in a directory under the
    /// cache for its source roots, which is kept for the next run.
    #[arg(long, visible_alias = "out", value_name = "DIR")]
    keep: Option<PathBuf>,

    #[command(flatten)]
    limits: LimitArgs,
}

/// The application `hgl run` runs, by its path: the one `named`, or the
/// program's only one. On failure the reason has been printed, and the
/// exit status to end with is returned.
fn chosen_application(paths: &[&str], named: Option<&str>) -> Result<String, ExitCode> {
    match (named, paths) {
        (_, []) => {
            eprintln!("hgl: the program has no application to run");
            Err(ExitCode::from(EXIT_FAILED))
        }
        (None, [only]) => Ok(only.to_string()),
        (None, several) => {
            eprintln!(
                "hgl: the program has {}; name one with `--application`: {}",
                count(several.len(), "application"),
                paths.join(", ")
            );
            Err(ExitCode::from(EXIT_FAILED))
        }
        (Some(path), applications) => {
            if applications.contains(&path) {
                Ok(path.to_string())
            } else {
                eprintln!(
                    "hgl: the program has no application `{path}`; it has: {}",
                    paths.join(", ")
                );
                Err(ExitCode::from(EXIT_FAILED))
            }
        }
    }
}

/// What `hgl emit` writes for, and `hgl test` and `hgl run` run on:
/// a transpiler crate, `hgl_cpp`. `ValueEnum` makes the variant's name,
/// in lower case, the one value `--target` accepts; any other is
/// refused as a usage error.
#[derive(Clone, Copy, ValueEnum)]
enum Target {
    /// C++ against hgraph's C++ API, built with the SDK that
    /// the bundle provides or `HGL_HGRAPH_SDK` and `HGL_HGRAPH_DEPS` name.
    Cpp,
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
        Command::Doc(arguments) => {
            let stack = analysis_stack(arguments.limits.limits());
            on_stack_of(stack, move || doc::run(&arguments))
        }
        Command::Test(arguments) => {
            let stack = analysis_stack(arguments.limits.limits());
            on_stack_of(stack, move || test(&arguments))
        }
        Command::Emit(arguments) => {
            let stack = analysis_stack(arguments.limits.limits());
            on_stack_of(stack, move || emit(&arguments))
        }
        Command::Build(arguments) => {
            let stack = analysis_stack(arguments.limits.limits());
            on_stack_of(stack, move || match arguments.target {
                Target::Cpp => cpp::build(&arguments),
            })
        }
        Command::Run(arguments) => {
            let stack = analysis_stack(arguments.limits.limits());
            on_stack_of(stack, move || match arguments.target {
                Target::Cpp => cpp::run(&arguments),
            })
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Command::NativeBuildChild {
            parent,
            program,
            arguments,
        } => cpp::native_build_child(parent, &program, &arguments),
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
    let bytes = read_source(path)?;
    match sources.add_bytes(name, bytes) {
        Ok(file) => Ok(file),
        Err(error) => {
            eprintln!("hgl: {error}");
            Err(ExitCode::from(EXIT_FAILED))
        }
    }
}

/// Read the original bytes. The lexer diagnoses non-UTF-8 source (LEX-16).
fn read_source(path: &Path) -> Result<Vec<u8>, ExitCode> {
    fs::read(path).map_err(|error| {
        eprintln!("hgl: cannot read {}: {error}", path.display());
        ExitCode::from(EXIT_FAILED)
    })
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
    discover_sources(roots, |_, _, _| {})
}

/// Visits every way a file is reached and returns each source once. The
/// visitor gets the written path, canonical identity and first path used
/// for that source, so a path selector can match an alternate spelling.
fn discover_sources(
    roots: &[PathBuf],
    mut visit: impl FnMut(&Path, &Path, &Path),
) -> Result<Vec<PathBuf>, ExitCode> {
    let mut found = Vec::new();
    // A file reached twice, through two roots or a link, is read once.
    // `canonicalize` gives a file's one true path, whatever path led to it.
    let mut seen = HashMap::new();
    let mut add = |path: PathBuf, found: &mut Vec<PathBuf>| {
        let identity = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let index = if let Some(&index) = seen.get(&identity) {
            index
        } else {
            let index = found.len();
            seen.insert(identity.clone(), index);
            found.push(path.clone());
            index
        };
        visit(&path, &identity, &found[index]);
    };
    for root in roots {
        if root.is_file() {
            add(root.clone(), &mut found);
            continue;
        }
        let mut contains_source = false;
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
                contains_source = true;
                add(entry.into_path(), &mut found);
            }
        }
        if !contains_source {
            eprintln!("hgl: no .hgl file under {}", root.display());
            return Err(ExitCode::from(EXIT_FAILED));
        }
    }
    Ok(found)
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

/// The source inputs, read once so a build key and the parser see the
/// same bytes even when a file changes during the build.
struct ProgramSources {
    files: Vec<(String, Vec<u8>)>,
}

impl ProgramSources {
    fn read(roots: &[PathBuf]) -> Result<Self, ExitCode> {
        Self::read_paths(&find_sources(roots)?)
    }

    /// Reads a source list that has already been discovered and ordered.
    fn read_paths(paths: &[PathBuf]) -> Result<Self, ExitCode> {
        let files = paths
            .iter()
            .map(|path| Ok((path.display().to_string(), read_source(path)?)))
            .collect::<Result<_, ExitCode>>()?;
        Ok(Self { files })
    }
}

/// Finds, reads, lexes and parses the source files and the supplied core.
fn parse_program(
    roots: &[PathBuf],
    limits: Limits,
    documentation_examples: bool,
) -> Result<Parsed, ExitCode> {
    parse_sources(
        &ProgramSources::read(roots)?,
        limits,
        None,
        documentation_examples,
    )
}

/// Parses the snapshot, with supplied core if the sources have no core
/// module. RUN-2 may substitute the new text of one named file. SDOC-9
/// examples are parsed only from the explicit inputs and for test analysis.
fn parse_sources(
    inputs: &ProgramSources,
    limits: Limits,
    replaced: Option<(&str, &str)>,
    documentation_examples: bool,
) -> Result<Parsed, ExitCode> {
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
    for (name, bytes) in &inputs.files {
        let file = match replaced {
            Some((replaced, text)) if replaced == name => sources.add(name, text),
            _ => sources.add_bytes(name, bytes.clone()),
        }
        .map_err(|error| {
            eprintln!("hgl: {error}");
            ExitCode::from(EXIT_FAILED)
        })?;
        let mut parsed = parse_file(&sources, file, limits, &mut diagnostics);
        if documentation_examples {
            parsed.examples = hgl_parser::documentation::parse_examples(
                &sources,
                file,
                &parsed.root,
                limits,
                &mut diagnostics,
            );
        }
        files.push(parsed);
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
            let mut supplied_diagnostics = Diagnostics::new();
            let mut parsed = parse_file(&sources, file, Limits::DEFAULT, &mut supplied_diagnostics);
            // TST-14: only explicit source contributes named tests and
            // their diagnostics. Core's ordinary declarations still check.
            parsed.compiler_supplied = true;
            parsed.omit.extend(
                parsed
                    .root
                    .descendants()
                    .filter_map(ast::TestDecl::cast)
                    .map(|test| test.syntax().span()),
            );
            for diagnostic in supplied_diagnostics.as_slice() {
                let span = diagnostic.primary.span;
                if !parsed
                    .omit
                    .iter()
                    .any(|omitted| omitted.start <= span.start && span.end <= omitted.end)
                {
                    diagnostics.emit(diagnostic.clone());
                }
            }
            files.push(parsed);
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
/// The analyser reads modules, declarations and imports, resolves names,
/// reads the types written, and types the bodies of value functions,
/// nodes, sources, graphs and tests. The output says what is checked,
/// and that no test is run.
fn check(arguments: &CheckArgs) -> ExitCode {
    let Parsed {
        sources,
        files: parsed,
        mut diagnostics,
        supplied,
    } = match parse_program(&arguments.roots, arguments.limits.limits(), true) {
        Ok(program) => program,
        Err(status) => return status,
    };
    let program = hgl_analyser::analyse(&parsed, &mut diagnostics);
    if arguments.json {
        diagnostics.sort_by_location();
        if let Err(error) = check_json::write(std::io::stdout().lock(), &sources, &diagnostics) {
            eprintln!("hgl: cannot write diagnostics: {error}");
            return ExitCode::from(EXIT_FAILED);
        }
        return if diagnostics.has_errors() {
            ExitCode::from(EXIT_REJECTED)
        } else {
            ExitCode::SUCCESS
        };
    }
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
    } = match parse_program(&arguments.roots, arguments.limits.limits(), false) {
        Ok(program) => program,
        Err(status) => return status,
    };
    let program = hgl_analyser::analyse(&parsed, &mut diagnostics);
    if diagnostics.has_errors() {
        println!("nothing written: the program is rejected");
        return report(&sources, diagnostics);
    }
    let (name, files) = match arguments.target {
        Target::Cpp => {
            let file_name =
                |at: hgl_diagnostics::Location| sources.file(at.file).name().to_string();
            let files = hgl_cpp::transpile(&program, &file_name, &mut diagnostics).files;
            (hgl_cpp::TARGET, files)
        }
    };
    if diagnostics.has_errors() {
        println!("nothing written: the {name} target does not write all of the program yet");
        return report(&sources, diagnostics);
    }
    match cpp::write(&files, &arguments.out) {
        Ok(()) => println!(
            "wrote {} for {name} to {}",
            count(files.len(), "file"),
            arguments.out.display()
        ),
        Err(error) => {
            eprintln!("hgl: cannot write to {}: {error}", arguments.out.display());
            return ExitCode::from(EXIT_FAILED);
        }
    }
    report(&sources, diagnostics)
}

/// The checked program and the source positions for the target to write.
struct PreparedProgram {
    sources: SourceMap,
    parsed: Vec<ParsedFile>,
    front_end: Diagnostics,
    program: Program,
    chosen: Option<String>,
    original_source: Option<(hgl_source::SourceFile, replace::SourceEdits)>,
}

/// Checks a program and, for a run or a named application, applies
/// the requested overrides before writing the program (RUN-2, RUN-6).
fn prepare_program(
    inputs: &ProgramSources,
    limits: Limits,
    replacements: &cpp::Replacements<'_>,
    running: bool,
    debug: bool,
    documentation_examples: bool,
) -> Result<PreparedProgram, ExitCode> {
    let Parsed {
        mut sources,
        files: mut parsed,
        diagnostics: mut front_end,
        ..
    } = parse_sources(inputs, limits, None, documentation_examples)?;
    let mut diagnostics = front_end.clone();
    let mut program = hgl_analyser::analyse(&parsed, &mut diagnostics);
    if diagnostics.has_errors() {
        if running && !debug {
            println!("not run: the program is rejected");
        }
        return Err(report(&sources, diagnostics));
    }
    let chosen = if running
        || replacements.application.is_some()
        || !replacements.arguments.is_empty()
        || !replacements.configuration.is_empty()
    {
        let paths: Vec<_> = program
            .modules
            .iter()
            .filter(|module| module.run_body.is_some())
            .map(|module| module.path.as_str())
            .collect();
        Some(chosen_application(&paths, replacements.application)?)
    } else {
        None
    };
    // RUN-2: what starts the application replaces an argument of its
    // entry, and supplies its run's configuration (RUN-6). The
    // application is read again with each written in it, and the
    // program checked again: each is held to what the application
    // writes is.
    let mut original_source = None;
    if (!replacements.arguments.is_empty() || !replacements.configuration.is_empty())
        && let Some(chosen) = chosen.as_deref()
    {
        let written = replace::EntryCall::of(&program, chosen);
        let written_as = written
            .as_ref()
            .map_or("no entry".to_string(), |entry| entry.shown(&program));
        // The entry's generic arguments, where the application left
        // them to be inferred from the arguments now replaced. They are
        // written out for the second read, so that a replacement is
        // read as the argument was expected: but only where the
        // application, with them written and nothing replaced, still
        // reads as it did. Its file may have no name for one of them.
        let inferred = replace::inferred_generics(&parsed, &program, chosen);
        let explicit = inferred.as_ref().filter(|generics| {
            let Ok((file, text, _)) = replace::replaced(
                &sources,
                &parsed,
                &program,
                chosen,
                &[],
                &[],
                Some(generics),
                limits,
            ) else {
                return false;
            };
            let name = sources.file(file).name().to_string();
            let Ok(Parsed {
                files,
                diagnostics: mut found,
                ..
            }) = parse_sources(inputs, limits, Some((&name, &text)), documentation_examples)
            else {
                return false;
            };
            let read = hgl_analyser::analyse(&files, &mut found);
            !found.has_errors() && replace::EntryCall::of(&read, chosen) == written
        });
        let (file, text, edits) = match replace::replaced(
            &sources,
            &parsed,
            &program,
            chosen,
            replacements.arguments,
            replacements.configuration,
            explicit,
            limits,
        ) {
            Ok(replaced) => replaced,
            Err(message) => {
                eprintln!("hgl: {message}");
                return Err(ExitCode::from(EXIT_FAILED));
            }
        };
        let name = sources.file(file).name().to_string();
        let Parsed {
            sources: read_again,
            files: read_files,
            diagnostics: read_diagnostics,
            ..
        } = parse_sources(inputs, limits, Some((&name, &text)), documentation_examples)?;
        let mut found = read_diagnostics.clone();
        program = hgl_analyser::analyse(&read_files, &mut found);
        parsed = read_files;
        front_end = read_diagnostics;
        if debug {
            original_source = Some((sources.file(file).clone(), edits));
        }
        sources = read_again;
        if found.has_errors() {
            if running && !debug {
                println!("not run: the program is rejected with the replacements given");
            }
            return Err(report(&sources, found));
        }
        // "a constant checked against the type it replaces": the types
        // of the entry's parameters are those of the entry the
        // application calls, with the generic arguments its call comes
        // to. A replacement that makes the call another entry, or the
        // same one with other generic arguments, is of another type.
        let replaced = replace::EntryCall::of(&program, chosen);
        if replaced != written {
            if running && !debug {
                println!("not run: the program is rejected with the replacements given");
            }
            let replaced_as = replaced
                .as_ref()
                .map_or("no entry".to_string(), |entry| entry.shown(&program));
            // Two types of one name, each of its own module, are
            // written alike.
            let alike = if replaced_as == written_as {
                ", whose generic arguments are other types of the same names"
            } else {
                ""
            };
            eprintln!(
                "hgl: a replacement is of the type it replaces: `{chosen}` runs `{written_as}`, \
                 and with the replacements given it would run `{replaced_as}`{alike}"
            );
            return Err(ExitCode::from(EXIT_REJECTED));
        }
    }
    Ok(PreparedProgram {
        sources,
        parsed,
        front_end,
        program,
        chosen,
        original_source,
    })
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
///     cases/rolling.hgl:40:18: the cpp target does not write a temporal value of `set<list<bool>>` yet
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
    let (paths, source_files) = match test_paths::sources(&arguments.roots, &arguments.test_paths) {
        Ok(Some(selected)) => (Some(selected.paths), Some(selected.names)),
        Ok(None) => (None, None),
        Err(status) => return status,
    };
    let selection = Selection {
        tests: arguments.tests.clone(),
        rejections_only: arguments.rejections_only,
        integration: arguments.integration,
        integration_only: arguments.integration_only,
        source_files,
    };
    if matches!(arguments.target, Some(Target::Cpp)) {
        return cpp::test(arguments, &selection, paths.as_deref());
    }
    let parsed = match paths {
        Some(paths) => ProgramSources::read_paths(&paths)
            .and_then(|inputs| parse_sources(&inputs, arguments.limits.limits(), None, true)),
        None => parse_program(&arguments.roots, arguments.limits.limits(), true),
    };
    let Parsed {
        sources,
        files,
        diagnostics,
        ..
    } = match parsed {
        Ok(program) => program,
        Err(status) => return status,
    };
    let report = hgl_runner::run(&sources, &files, &diagnostics, &selection, None);
    if selection.source_files.is_some() && report.cases.is_empty() && !report.prevented() {
        return print_empty_test_selection(true);
    }
    print_report(&sources, &report)
}

fn write_problems(sources: &SourceMap, problems: &[Problem]) {
    for problem in problems {
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
}

fn print_problems(sources: &SourceMap, problems: &[Problem]) -> ExitCode {
    write_problems(sources, problems);
    println!("no case was run");
    ExitCode::from(EXIT_REJECTED)
}

/// Reports what stopped test preparation before any case (TST-14).
fn print_test_prevention(sources: &SourceMap, report: &Report, summary: bool) -> ExitCode {
    write_problems(sources, &report.problems);
    if let Err(error) = render::to_stderr(sources, &report.surviving) {
        eprintln!("hgl: cannot write diagnostics: {error}");
        return ExitCode::from(EXIT_FAILED);
    }
    if summary {
        println!("no case was run");
    }
    ExitCode::from(EXIT_REJECTED)
}

fn print_report(sources: &SourceMap, report: &Report) -> ExitCode {
    if report.prevented() {
        return print_test_prevention(sources, report, true);
    }
    let outcomes: Vec<_> = report
        .cases
        .iter()
        .map(|case| {
            (
                case.identity.as_str(),
                case.kind,
                CaseOutcome::from_runner(&case.outcome, sources),
            )
        })
        .collect();
    print_outcomes(&outcomes)
}

/// Reports an empty selection (TST-15). A path selection also gives the
/// zero counts consumed by callers that run a file or directory.
fn print_empty_test_selection(path_selected: bool) -> ExitCode {
    println!("no case was selected: this run shows nothing");
    if path_selected {
        println!("rejection cases: 0 passed, 0 failed");
        println!("executable cases: 0 passed, 0 failed, 0 not run");
    }
    ExitCode::SUCCESS
}

/// Prints current or cached case details and their separate totals (TST-14).
fn print_outcomes(cases: &[(&str, CaseKind, CaseOutcome)]) -> ExitCode {
    if cases.is_empty() {
        return print_empty_test_selection(false);
    }
    let mut rejected = 0;
    let mut rejection_failures = 0;
    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;
    let mut untargeted = 0;
    for (identity, kind, outcome) in cases {
        let label = match outcome {
            CaseOutcome::Passed => "passed",
            CaseOutcome::Failed(_) => "FAILED",
            CaseOutcome::NotRun(_) => "not run",
        };
        let kind_label = match kind {
            CaseKind::Rejection => "rejection",
            CaseKind::Executable => "executable",
        };
        println!("{label:<8} {kind_label:<11} {identity}");
        match (kind, outcome) {
            (CaseKind::Rejection, CaseOutcome::Passed) => rejected += 1,
            (CaseKind::Rejection, CaseOutcome::Failed(_)) => rejection_failures += 1,
            (CaseKind::Executable, CaseOutcome::Passed) => passed += 1,
            (CaseKind::Executable, CaseOutcome::Failed(_)) => failed += 1,
            (_, CaseOutcome::NotRun(reason)) => {
                skipped += 1;
                untargeted += usize::from(reason == hgl_runner::NO_TARGET);
            }
        }
        if let CaseOutcome::Failed(details) | CaseOutcome::NotRun(details) = outcome
            && details != hgl_runner::NO_TARGET
        {
            for line in details.lines() {
                println!("    {line}");
            }
        }
    }
    println!("rejection cases: {rejected} passed, {rejection_failures} failed");
    if passed + failed + skipped == 0 {
        println!("executable cases: none selected");
    } else if untargeted == skipped && passed + failed == 0 {
        println!(
            "executable cases: {skipped} not run: {}",
            hgl_runner::NO_TARGET
        );
    } else {
        println!("executable cases: {passed} passed, {failed} failed, {skipped} not run");
    }
    if rejection_failures + failed > 0 {
        ExitCode::from(EXIT_REJECTED)
    } else if skipped > 0 {
        ExitCode::from(EXIT_FAILED)
    } else {
        ExitCode::SUCCESS
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

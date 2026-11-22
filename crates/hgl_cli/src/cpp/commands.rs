//! The build command and the application and debugger paths that use it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hgl_analyser::Program;
use hgl_cpp::Output;
use hgl_diagnostics::{Diagnostics, Location};
use hgl_runner::Selection;
use hgl_source::SourceMap;

use super::application::{self, RanApplication};
use super::build::Workspace;
use super::plan::{self, Action};
use super::{BuildSession, BuiltProgram, Replacements, sdk};
use crate::{
    BuildArgs, BuildFormat, EXIT_FAILED, EXIT_REJECTED, LimitArgs, ProgramSources, RunArgs, count,
};

/// Inputs the build, run and debugger commands give to the same builder.
struct BuildRequest<'a> {
    roots: &'a [PathBuf],
    keep: Option<&'a Path>,
    debug: bool,
    tests: bool,
    running: bool,
    limits: LimitArgs,
    replacements: Replacements<'a>,
}

impl BuildRequest<'_> {
    fn build(&self) -> Result<(BuildSession, BuiltProgram), ExitCode> {
        let session = BuildSession::open(self.roots, self.keep, self.debug);
        let inputs = match ProgramSources::read(self.roots) {
            Ok(inputs) => inputs,
            Err(status) => {
                if let Ok(session) = session {
                    let _ = session.begin();
                }
                return Err(status);
            }
        };
        let session = session.and_then(|session| {
            let key = session.key(&inputs, self.limits, &self.replacements)?;
            Ok((session, key))
        });
        let session = match session {
            Ok((session, key)) => {
                if let Some(built) = session.reused(&key, self.tests) {
                    return Ok((session, built));
                }
                session.begin().map(|()| (session, key))
            }
            Err(reason) => Err(reason),
        };
        let (output, program, chosen) = self.prepare(&inputs)?;
        if (self.running || self.debug)
            && let Some(path) = &chosen
        {
            runnable_application(&program, path, self.debug)?;
        }
        let (session, key) = session.map_err(|reason| self.failed(chosen.as_deref(), reason))?;
        let description = session
            .finish_build(&key, &output, &program)
            .map_err(|reason| self.failed(chosen.as_deref(), reason))?;
        Ok((
            session,
            BuiltProgram {
                program,
                description,
                reused: false,
            },
        ))
    }

    fn prepare(
        &self,
        inputs: &ProgramSources,
    ) -> Result<(Output, plan::Program, Option<String>), ExitCode> {
        let limits = self.limits.limits();
        let has_replacements =
            !self.replacements.arguments.is_empty() || !self.replacements.configuration.is_empty();
        if self.tests && !has_replacements {
            let parsed = crate::parse_sources(inputs, limits, None)?;
            return self.prepare_tests(parsed.sources, &parsed.files, &parsed.diagnostics, None);
        }
        let checked =
            crate::prepare_program(inputs, limits, &self.replacements, self.running, self.debug)?;
        if self.tests {
            return self.prepare_tests(
                checked.sources,
                &checked.parsed,
                &checked.front_end,
                checked.original_source.as_ref(),
            );
        }
        let output = self.lower(
            &checked.program,
            &checked.sources,
            checked.original_source.as_ref(),
        );
        let program = plan::Program::new(&output, None, &checked.sources);
        Ok((output, program, checked.chosen))
    }

    fn prepare_tests(
        &self,
        sources: SourceMap,
        files: &[hgl_analyser::ParsedFile],
        front_end: &Diagnostics,
        original_source: Option<&(hgl_source::SourceFile, crate::replace::SourceEdits)>,
    ) -> Result<(Output, plan::Program, Option<String>), ExitCode> {
        let checked = hgl_runner::prepare_all(&sources, files, front_end, &Selection::default());
        let Some(program) = checked.program else {
            return Err(crate::print_report(&sources, &checked.report));
        };
        let output = self.lower(&program, &sources, original_source);
        let program = plan::Program::new(&output, Some(&checked.report), &sources);
        let chosen = self
            .replacements
            .application
            .map(|path| choose_application(&program, Some(path)))
            .transpose()?;
        Ok((output, program, chosen))
    }

    fn lower(
        &self,
        program: &Program,
        sources: &SourceMap,
        original_source: Option<&(hgl_source::SourceFile, crate::replace::SourceEdits)>,
    ) -> Output {
        let position = |at: Location, line: usize| {
            let name = sources.file(at.file).name();
            let line = match original_source {
                Some((original, edits)) if original.name() == name => {
                    original.line_index(edits.original_offset(at.span.start)?) + 1
                }
                _ => line,
            };
            let path = if self.debug && !name.starts_with('<') {
                std::path::absolute(name)
                    .map_or_else(|_| name.to_string(), |p| p.display().to_string())
            } else {
                name.to_string()
            };
            Some((path, line))
        };
        hgl_cpp::transpile_with_positions(program, &position, &mut Diagnostics::new())
    }

    fn failed(&self, path: Option<&str>, reason: String) -> ExitCode {
        if self.running
            && !self.debug
            && let Some(path) = path
        {
            return ran(path, hgl_cpp::TARGET, RanApplication::NotRun(reason));
        }
        build_error(reason)
    }
}

fn build_error(reason: String) -> ExitCode {
    eprintln!("hgl: {reason}");
    ExitCode::from(EXIT_FAILED)
}

fn runnable_application<'a>(
    program: &'a plan::Program,
    path: &str,
    debug: bool,
) -> Result<&'a plan::Application, ExitCode> {
    let Some(application) = program.applications.iter().find(|app| app.path == path) else {
        eprintln!("hgl: the target wrote no application `{path}`");
        return Err(ExitCode::from(EXIT_FAILED));
    };
    if let Some(reason) = &application.blocked {
        eprintln!("{reason}");
        if !debug {
            println!(
                "not run: the {} target does not write all that {path} needs yet",
                hgl_cpp::TARGET
            );
        }
        return Err(ExitCode::from(EXIT_FAILED));
    }
    Ok(application)
}

fn choose_application(program: &plan::Program, named: Option<&str>) -> Result<String, ExitCode> {
    let paths: Vec<_> = program
        .applications
        .iter()
        .map(|app| app.path.as_str())
        .collect();
    crate::chosen_application(&paths, named)
}

/// Builds or reuses the application's artifacts, then runs its host.
pub(crate) fn run(arguments: &RunArgs) -> ExitCode {
    let request = BuildRequest {
        roots: &arguments.roots,
        keep: arguments.keep.as_deref(),
        debug: arguments.debug_build.is_some(),
        tests: false,
        running: true,
        limits: arguments.limits,
        replacements: Replacements {
            application: arguments.application.as_deref(),
            arguments: &arguments.arguments,
            configuration: &arguments.config,
        },
    };
    if let Some(directory) = arguments.debug_build.as_deref() {
        return match legacy_debug(&request, directory) {
            Ok(launch) => {
                println!("{launch}");
                ExitCode::SUCCESS
            }
            Err(status) => status,
        };
    }
    let stop = application::Stop::new();
    let asked = stop.clone();
    let _ = ctrlc::set_handler(move || asked.ask());
    let (build, built) = match request.build() {
        Ok(built) => built,
        Err(status) => return status,
    };
    let chosen = match choose_application(&built.program, arguments.application.as_deref()) {
        Ok(path) => path,
        Err(status) => return status,
    };
    let app = match runnable_application(&built.program, &chosen, request.debug) {
        Ok(app) => app,
        Err(status) => return status,
    };
    let libraries = sdk::library_path(&built.description);
    ran(
        &app.path,
        hgl_cpp::TARGET,
        application::launch(&build.workspace, &app.path, &libraries, &stop),
    )
}

/// Validates the legacy debugger request before creating its session
/// directory. Each launch has its own executable until the caller ends it.
fn legacy_debug(
    request: &BuildRequest<'_>,
    directory: &Path,
) -> Result<serde_json::Value, ExitCode> {
    let inputs = ProgramSources::read(request.roots)?;
    let (output, program, chosen) = request.prepare(&inputs)?;
    let chosen =
        chosen.ok_or_else(|| build_error("the program has no application to run".into()))?;
    let app = runnable_application(&program, &chosen, true)?;
    let directory = debug_session(directory).map_err(build_error)?;
    let session =
        BuildSession::open(request.roots, Some(directory.path()), true).map_err(build_error)?;
    let key = session
        .key(&inputs, request.limits, &request.replacements)
        .map_err(build_error)?;
    session.begin().map_err(build_error)?;
    let description = session
        .finish_build(&key, &output, &program)
        .map_err(build_error)?;
    let launch = application::debug_launch(
        &session.workspace,
        &app.path,
        sdk::library_path(&description),
    )
    .map_err(build_error)?;
    let _ = directory.keep();
    Ok(launch)
}

fn debug_session(directory: &Path) -> Result<tempfile::TempDir, String> {
    fs::create_dir_all(directory).map_err(|error| format!("{}: {error}", directory.display()))?;
    tempfile::Builder::new()
        .prefix("session-")
        .tempdir_in(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))
}

/// Builds or reuses a program without starting any executable.
pub(crate) fn build(arguments: &BuildArgs) -> ExitCode {
    let request = BuildRequest {
        roots: &arguments.roots,
        keep: arguments.out.as_deref(),
        debug: arguments.debug,
        tests: arguments.tests,
        running: false,
        limits: arguments.limits,
        replacements: Replacements {
            application: arguments.application.as_deref(),
            arguments: &arguments.arguments,
            configuration: &arguments.config,
        },
    };
    let (session, built) = match request.build() {
        Ok(built) => built,
        Err(status) => return status,
    };
    if let Some(path) = arguments.application.as_deref() {
        if let Err(status) = choose_application(&built.program, Some(path)) {
            return status;
        }
        if arguments.debug
            && let Err(status) = runnable_application(&built.program, path, true)
        {
            return status;
        }
    }
    match arguments.format {
        BuildFormat::Text => {
            let action = if built.reused { "reused" } else { "built" };
            println!(
                "{action} {}, {} and {} on {} in {}",
                count(built.program.units.len(), "unit"),
                count(
                    built
                        .program
                        .applications
                        .iter()
                        .filter(|app| app.blocked.is_none())
                        .count(),
                    "application"
                ),
                count(test_executables(&built.program).len(), "test executable"),
                hgl_cpp::TARGET,
                session.workspace.directory.display(),
            );
        }
        BuildFormat::Json => match build_description(&session.workspace, &built, arguments) {
            Ok(description) => println!("{description}"),
            Err(reason) => return build_error(reason),
        },
    }
    ExitCode::SUCCESS
}

fn test_executables(program: &plan::Program) -> BTreeSet<&str> {
    program
        .tests
        .iter()
        .flatten()
        .filter_map(|test| match &test.action {
            Action::Run(test) => Some(test.stem.as_str()),
            Action::Checked(_) => None,
        })
        .collect()
}

fn build_description(
    workspace: &Workspace,
    built: &BuiltProgram,
    arguments: &BuildArgs,
) -> Result<serde_json::Value, String> {
    let directory = workspace
        .directory
        .to_str()
        .ok_or_else(|| "the build workspace path is not UTF-8".to_string())?;
    let applications: Vec<_> = built
        .program
        .applications
        .iter()
        .map(|app| {
            serde_json::json!({
                "application": app.path,
                "executable": app.blocked.is_none().then(|| workspace.application(&app.path)),
                "blocked": app.blocked,
            })
        })
        .collect();
    let tests: Vec<_> = test_executables(&built.program)
        .into_iter()
        .map(|stem| workspace.test_executable(stem))
        .collect();
    let mut description = serde_json::json!({
        "version": 1,
        "target": hgl_cpp::TARGET,
        "workspace": directory,
        "reused": built.reused,
        "units": built.program.units,
        "applications": applications,
        "tests": tests,
    });
    let debug_app =
        arguments
            .application
            .as_deref()
            .or_else(|| match built.program.applications.as_slice() {
                [only] if only.blocked.is_none() => Some(only.path.as_str()),
                _ => None,
            });
    if arguments.debug
        && let Some(path) = debug_app
    {
        let launch =
            application::debug_launch(workspace, path, sdk::library_path(&built.description))?;
        if let (Some(description), serde_json::Value::Object(launch)) =
            (description.as_object_mut(), launch)
        {
            description.extend(launch);
        }
    }
    Ok(description)
}

/// What `hgl run` prints of what became of application `path` on the
/// target `name`, and the status it ends with: the run's output once the
/// run has stopped, a line for each publication (core's CRUN-8), or why
/// it failed (CRUN-6: "A run that failed has no output") or was not run.
fn ran(path: &str, name: &str, ran: RanApplication) -> ExitCode {
    match ran {
        RanApplication::Stopped { output, by_host } => {
            let how = if by_host { " until it was stopped" } else { "" };
            match output {
                Some(published) => {
                    for (at, value) in &published {
                        println!("{at}  {value}");
                    }
                    println!(
                        "ran {path} on {name}{how}: {}",
                        count(published.len(), "publication")
                    );
                }
                None => println!("ran {path} on {name}{how}: its entry has no result"),
            }
            ExitCode::SUCCESS
        }
        RanApplication::Failed(message) => {
            eprintln!("hgl: {path} failed: {message}");
            ExitCode::from(EXIT_REJECTED)
        }
        RanApplication::NotRun(reason) => {
            eprintln!("hgl: {path} was not run: {reason}");
            ExitCode::from(EXIT_FAILED)
        }
    }
}

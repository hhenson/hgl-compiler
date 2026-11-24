//! Build-time commands for core conformance, release verification and packaging.

use std::error::Error;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Output};

use clap::{Parser, Subcommand, ValueEnum};

mod core_gate;
mod launch;
mod release;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Clone, Copy, ValueEnum)]
enum Target {
    #[value(name = "aarch64-apple-darwin")]
    Macos,
    #[value(name = "x86_64-unknown-linux-gnu")]
    Linux,
}

impl Target {
    fn triple(self) -> &'static str {
        match self {
            Self::Macos => "aarch64-apple-darwin",
            Self::Linux => "x86_64-unknown-linux-gnu",
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum CoreTarget {
    Cpp,
}

#[derive(Parser)]
#[command(about = "Run the HGL workspace's build and release checks")]
struct Arguments {
    #[command(subcommand)]
    mode: Mode,
}

#[derive(Subcommand)]
enum Mode {
    /// Run both pinned core suites and compare them with the recorded gaps.
    CheckCore {
        hgl: PathBuf,
        core: PathBuf,
        #[arg(long, value_enum, default_value = "cpp")]
        target: CoreTarget,
        #[arg(long)]
        write_list: bool,
    },
    /// Print the workspace version.
    Version,
    /// Hash the macOS release's native build recipe.
    NativeKey,
    /// Restore the native libraries and notices from a verified cache.
    CopyNative {
        source: PathBuf,
        destination: PathBuf,
        #[arg(long)]
        debug: Option<PathBuf>,
    },
    /// Publish a verified native cache atomically.
    SaveNative {
        source: PathBuf,
        destination: PathBuf,
        #[arg(long)]
        debug: Option<PathBuf>,
    },
    /// Find live artifacts from a verified push to main.
    FindVerifiedRun {
        repository: String,
        commit: String,
        current_run: String,
    },
    /// Collect the Rust toolchain and compiler dependencies' license notices.
    Licenses {
        destination: PathBuf,
        #[arg(long, value_enum, default_value = "aarch64-apple-darwin")]
        target: Target,
    },
    /// Check a release's layout, architecture, load paths and notices.
    Verify {
        bundle: PathBuf,
        #[arg(long, value_enum, default_value = "aarch64-apple-darwin")]
        target: Target,
        #[arg(long)]
        forbid: Vec<PathBuf>,
    },
    /// Verify debug libraries and their association with the release.
    VerifyDebug {
        tree: PathBuf,
        #[arg(long)]
        release: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "aarch64-apple-darwin")]
        target: Target,
        #[arg(long)]
        forbid: Vec<PathBuf>,
    },
    /// Read a string field of a debugger launch description.
    LaunchField {
        description: PathBuf,
        keys: Vec<String>,
    },
    /// Print the described command, one argument per line.
    LaunchCommand { description: PathBuf },
    /// Run the debugger fixture's command outside the debugger.
    RunDescribed { description: PathBuf },
}

fn run(mode: Mode) -> Result<bool> {
    match mode {
        Mode::CheckCore {
            hgl,
            core,
            target: CoreTarget::Cpp,
            write_list,
        } => {
            return core_gate::check(&hgl, &core, write_list);
        }
        Mode::Version => println!("{}", release::version()?),
        Mode::NativeKey => println!("{}", release::native_key()?),
        Mode::CopyNative {
            source,
            destination,
            debug,
        } => {
            release::copy_native(&source, &destination, debug.as_deref())?;
        }
        Mode::SaveNative {
            source,
            destination,
            debug,
        } => {
            release::save_native(&source, &destination, debug.as_deref())?;
        }
        Mode::FindVerifiedRun {
            repository,
            commit,
            current_run,
        } => {
            release::find_verified_run(&repository, &commit, &current_run)?;
        }
        Mode::Licenses {
            destination,
            target,
        } => release::licenses(&destination, target)?,
        Mode::Verify {
            bundle,
            target,
            forbid,
        } => release::verify(&bundle, &forbid, target)?,
        Mode::VerifyDebug {
            tree,
            release: bundle,
            target,
            forbid,
        } => {
            release::verify_debug(&tree, bundle.as_deref().unwrap_or(&tree), &forbid, target)?;
        }
        Mode::LaunchField { description, keys } => launch::field(&description, &keys)?,
        Mode::LaunchCommand { description } => launch::command(&description)?,
        Mode::RunDescribed { description } => launch::run(&description)?,
    }
    Ok(true)
}

fn main() -> ExitCode {
    match run(Arguments::parse().mode) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("hgl_build_tool: {error}");
            ExitCode::FAILURE
        }
    }
}

fn output(command: &mut Command) -> Result<Output> {
    let result = command.output().map_err(|error| {
        format!(
            "could not start {}: {error}",
            command.get_program().to_string_lossy()
        )
    })?;
    if result.status.success() {
        Ok(result)
    } else {
        Err(format!(
            "{}: {}\n{}",
            command.get_program().to_string_lossy(),
            result.status,
            String::from_utf8_lossy(&result.stderr),
        )
        .into())
    }
}

fn text(command: &mut Command) -> Result<String> {
    Ok(String::from_utf8(output(command)?.stdout)?)
}

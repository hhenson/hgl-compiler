//! Connects `hgl doc` to analysis and the independent documentation stage.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, ValueEnum};

use crate::{EXIT_FAILED, LimitArgs, parse_program, report};

/// Arguments of `hgl doc`.
#[derive(Args)]
pub(crate) struct DocArgs {
    /// Source roots, as for `hgl check`.
    #[arg(required = true, value_name = "ROOT")]
    roots: Vec<PathBuf>,

    /// The output format. HTML is the default.
    #[arg(long, value_enum, default_value = "html")]
    format: Format,

    /// Output directory. HTML defaults to hgl-doc; JSON defaults to stdout.
    #[arg(long, value_name = "DIR")]
    out: Option<PathBuf>,

    /// Include private declarations and named tests.
    #[arg(long)]
    private: bool,

    #[command(flatten)]
    pub(crate) limits: LimitArgs,
}

/// The documentation formats this command writes.
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Html,
    Json,
}

/// Analyses a program, then writes its API documentation when the source is accepted.
pub(crate) fn run(arguments: &DocArgs) -> ExitCode {
    let limits = arguments.limits.limits();
    let mut parsed = match parse_program(&arguments.roots, limits) {
        Ok(program) => program,
        Err(status) => return status,
    };
    let program = hgl_analyser::analyse(&parsed.files, &mut parsed.diagnostics);
    let status = report(&parsed.sources, parsed.diagnostics);
    if status != ExitCode::SUCCESS {
        return status;
    }

    let input_count = parsed.files.len() - parsed.supplied;
    let roots: Vec<_> = arguments
        .roots
        .iter()
        .filter(|root| root.is_dir())
        .filter_map(|root| {
            Some(DirectoryRoot {
                absolute: std::path::absolute(root).ok()?,
                canonical: fs::canonicalize(root).ok(),
            })
        })
        .collect();
    let paths: Vec<_> = parsed
        .sources
        .files()
        .iter()
        .enumerate()
        .map(|(index, source)| {
            if index < input_count {
                portable_path(Path::new(source.name()), &roots)
            } else {
                // Compiled-in core names already describe their logical location.
                source.name().to_string()
            }
        })
        .collect();
    let path = |file: hgl_source::FileId| paths[file.index()].clone();
    let result = match arguments.format {
        Format::Html => write_site(
            arguments
                .out
                .as_deref()
                .unwrap_or_else(|| Path::new("hgl-doc")),
            hgl_doc::build_site(&program, &parsed.sources, path, arguments.private),
        ),
        Format::Json => write_json(arguments.out.as_deref(), |output| {
            hgl_doc::write_json(output, &program, &parsed.sources, path, arguments.private)
        }),
    };
    if let Err(error) = result {
        eprintln!("hgl: cannot write documentation: {error}");
        return ExitCode::from(EXIT_FAILED);
    }
    ExitCode::SUCCESS
}

/// Writes the generated files under the chosen directory.
fn write_site(directory: &Path, files: Vec<hgl_doc::SiteFile>) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    for file in files {
        let destination = directory.join(file.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, file.contents)?;
    }
    Ok(())
}

fn write_json(
    out: Option<&Path>,
    document: impl FnOnce(&mut dyn Write) -> io::Result<()>,
) -> io::Result<()> {
    if let Some(out) = out {
        fs::create_dir_all(out)?;
        let mut output = BufWriter::new(File::create(out.join("documentation.json"))?);
        document(&mut output)?;
        output.flush()
    } else {
        let mut output = BufWriter::new(io::stdout().lock());
        document(&mut output)?;
        output.flush()
    }
}

/// A directory root as supplied and as identified by the filesystem.
struct DirectoryRoot {
    absolute: PathBuf,
    canonical: Option<PathBuf>,
}

/// Use the innermost supplied directory that contains the file. A
/// separately supplied file has its basename. Join components explicitly,
/// so neither the checkout location nor the platform separator is emitted.
fn portable_path(path: &Path, roots: &[DirectoryRoot]) -> String {
    let absolute = std::path::absolute(path).ok();
    let canonical = fs::canonicalize(path).ok();
    let relative = roots
        .iter()
        .flat_map(|root| {
            // The written path can retain a link inside a source root that
            // leads outside it. The canonical path handles equivalent input
            // spellings (including `..`) independent of which root found it.
            [
                beneath(absolute.as_deref(), Some(&root.absolute)),
                beneath(canonical.as_deref(), root.canonical.as_deref()),
            ]
            .into_iter()
            .flatten()
        })
        .map(|file| {
            let parts: Vec<_> = file
                .iter()
                .map(|component| component.to_string_lossy())
                .collect();
            (parts.len(), parts.join("/"))
        })
        .min();
    relative.map_or_else(
        || {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        },
        |(_, file)| file,
    )
}

fn beneath<'a>(file: Option<&'a Path>, root: Option<&Path>) -> Option<&'a Path> {
    let relative = file?.strip_prefix(root?).ok()?;
    relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
        .then_some(relative)
}

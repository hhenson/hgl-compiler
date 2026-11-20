//! Native build/run dispatch. Cargo tracks the generated application, node
//! library and provider independently. Hold one workspace lock through launch.
use super::*;
use hgraph_native::Entry;
use std::io::Write as _;
use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io,
    process::Command,
};

fn unsupported(sources: &SourceMap, err: hgl_lower::Unsupported) -> String {
    let src = sources.file(err.0.file);
    format!(
        "{}:{}: target.unsupported: hgraph-native does not yet support {}",
        src.name(),
        src.line_col(err.0.span.start),
        err.1
    )
}
fn io_error(e: io::Error) -> String {
    e.to_string()
}
struct Workspace {
    dir: PathBuf,
    _lock: fs::File,
}
impl Workspace {
    fn open(dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&dir).map_err(io_error)?;
        let lock = fs::File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join(".hgl-build.lock"))
            .map_err(io_error)?;
        lock.lock().map_err(io_error)?;
        Ok(Self { dir, _lock: lock })
    }
    fn cached(keep: Option<&Path>, roots: &[PathBuf]) -> Result<Self, String> {
        if let Some(dir) = keep {
            return Self::open(dir.into());
        }
        let base = std::env::var_os("HGL_NATIVE_CACHE")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_CACHE_HOME").map(|p| PathBuf::from(p).join("hgl/native"))
            })
            .or_else(|| {
                std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache/hgl/native"))
            })
            .unwrap_or_else(|| std::env::temp_dir().join("hgl-native"));
        let mut hash = DefaultHasher::new();
        for path in roots {
            fs::canonicalize(path).map_err(io_error)?.hash(&mut hash);
        }
        Self::open(base.join(format!("{:016x}", hash.finish())))
    }
    fn write(&self, entries: &[Entry]) -> Result<(), String> {
        for (name, text) in hgraph_native::emit(entries) {
            let file = self.dir.join(name);
            let parent = file.parent().unwrap();
            if fs::read(&file).ok().as_deref() == Some(text.as_bytes()) {
                continue;
            }
            fs::create_dir_all(parent).map_err(io_error)?;
            let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
            temp.write_all(text.as_bytes()).map_err(io_error)?;
            temp.persist(file).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    fn build(&self, entries: &[Entry]) -> Result<PathBuf, String> {
        let emit_started = std::time::Instant::now();
        self.write(entries)?;
        let emit_ms = emit_started.elapsed().as_secs_f64() * 1000.0;
        let cargo_started = std::time::Instant::now();
        let result = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&self.dir)
            .args([
                "build",
                "--offline",
                "--message-format=json-render-diagnostics",
                "--bin",
                "hgl_app",
            ])
            .output()
            .map_err(io_error)?;
        if !result.status.success() {
            return Err(format!(
                "native build failed\n{}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        let messages = String::from_utf8_lossy(&result.stdout);
        if timing_enabled() {
            let rebuilt: Vec<_> = messages
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter(|msg| msg["reason"] == "compiler-artifact" && msg["fresh"] == false)
                .map(|msg| msg["target"]["name"].clone())
                .collect();
            timings(
                serde_json::json!({"emit_ms":emit_ms,"cargo_ms":cargo_started.elapsed().as_secs_f64()*1000.0,"rebuilt":rebuilt}),
            );
        }
        messages
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find_map(|v| {
                (v["target"]["name"] == "hgl_app")
                    .then(|| v["executable"].as_str().map(PathBuf::from))
                    .flatten()
            })
            .ok_or_else(|| "Cargo reported no application executable".into())
    }
}
pub(super) fn emit(p: &Program, sources: &SourceMap, args: &EmitArgs) -> Result<(), String> {
    let mut entries = vec![];
    let mut matched = std::collections::BTreeSet::new();
    for m in &p.modules {
        if m.application && args.tests.is_empty() {
            entries.push(Entry::Application(
                m.path.clone(),
                hgl_lower::application(p, m).map_err(|e| unsupported(sources, e))?,
            ));
        }
        for (i, t) in m.tests.iter().enumerate() {
            let name = format!("{}::{}", m.path, t.name);
            let mut wanted = args.tests.is_empty();
            for selector in &args.tests {
                if selector == &name || selector == &t.name {
                    matched.insert(selector);
                    wanted = true;
                }
            }
            if wanted {
                entries.push(Entry::Test(
                    name,
                    hgl_lower::test(p, m, i).map_err(|e| unsupported(sources, e))?,
                ));
            }
        }
    }
    if let Some(missing) = args.tests.iter().find(|s| !matched.contains(s)) {
        return Err(format!("unknown test {missing}"));
    }
    if entries.is_empty() {
        return Err("no executable application or test to emit".into());
    }
    Workspace::open(args.out.clone())?.write(&entries)?;
    println!("wrote Rust workspace to {}", args.out.display());
    Ok(())
}
pub(super) struct Native<'a> {
    pub sources: &'a SourceMap,
    pub keep: Option<&'a Path>,
    pub roots: &'a [PathBuf],
}
impl Executor for Native<'_> {
    fn execute(&mut self, p: &Program, tests: &[String]) -> Vec<Outcome> {
        let mut outcomes = vec![Outcome::NotRun("test not found".into()); tests.len()];
        let mut entries = vec![];
        let mut indices = vec![];
        for (slot, name) in tests.iter().enumerate() {
            let Some((module, test)) = name.rsplit_once("::") else {
                continue;
            };
            let Some(m) = p.modules.iter().find(|m| m.path == module) else {
                continue;
            };
            let Some(i) = m.tests.iter().position(|t| t.name == test) else {
                continue;
            };
            match hgl_lower::test(p, m, i) {
                Ok(cases) => {
                    entries.push(Entry::Test(name.clone(), cases));
                    indices.push(slot);
                }
                Err(e) => outcomes[slot] = Outcome::NotRun(unsupported(self.sources, e)),
            }
        }
        if !entries.is_empty() {
            let mut run = || -> Result<(), String> {
                let w = Workspace::cached(self.keep, self.roots)?;
                let exe = w.build(&entries)?;
                for &i in &indices {
                    let ran = Command::new(&exe)
                        .arg(&tests[i])
                        .output()
                        .map_err(io_error)?;
                    outcomes[i] = if ran.status.success() {
                        Outcome::Passed
                    } else {
                        Outcome::FailedRun(format!(
                            "{} {}",
                            ran.status,
                            String::from_utf8_lossy(&ran.stderr).trim()
                        ))
                    };
                }
                Ok(())
            };
            if let Err(e) = run() {
                for i in indices {
                    outcomes[i] = Outcome::NotRun(e.clone());
                }
            }
        }
        outcomes
    }
}
pub(super) fn application(
    p: &Program,
    sources: &SourceMap,
    args: &RunArgs,
    name: &str,
    started: std::time::Instant,
) -> Result<(), String> {
    let m = p
        .modules
        .iter()
        .find(|m| m.path == name && m.application)
        .ok_or("application missing")?;
    let lower_started = std::time::Instant::now();
    let graph = hgl_lower::application(p, m).map_err(|e| unsupported(sources, e))?;
    if timing_enabled() {
        timings(
            serde_json::json!({"frontend_ms":lower_started.duration_since(started).as_secs_f64()*1000.0,"lower_ms":lower_started.elapsed().as_secs_f64()*1000.0}),
        );
    }
    let w = Workspace::cached(args.keep.as_deref(), &args.roots)?;
    let exe = w.build(&[Entry::Application(name.to_string(), graph)])?;
    let launch = std::time::Instant::now();
    let status = Command::new(exe).arg(name).status().map_err(io_error)?;
    if timing_enabled() {
        timings(serde_json::json!({"launch_ms":launch.elapsed().as_secs_f64()*1000.0}));
    }
    if status.success() {
        Ok(())
    } else {
        Err(format!("native application failed ({status})"))
    }
}
pub(super) fn status(result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("hgl: {message}");
            ExitCode::from(EXIT_FAILED)
        }
    }
}

fn timing_enabled() -> bool {
    std::env::var_os("HGL_NATIVE_TIMINGS").is_some()
}
fn timings(fields: serde_json::Value) {
    eprintln!("hgl-native-timing {fields}");
}

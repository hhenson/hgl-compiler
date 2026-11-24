//! Account for every case reported by the C++ core conformance runs (TST-14, TST-15).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::Result;

const SUITES: [&str; 2] = ["conformance", "core_conformance"];

static CASE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?m)^(passed|FAILED|not run) +(rejection|executable) +([^\n]+)(?:\n    ([^\n]+))?$",
    )
    .expect("case row regex")
});
static UNSUPPORTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^.+\.hgl:\d+:\d+: the cpp target does not write (.+) yet$")
        .expect("target gap regex")
});
static REJECTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:.*/)?tests/([^/]+)/(.+?):\d+ (?:\S+ )+(\S+)$")
        .expect("rejection identity regex")
});
static NOT_REJECTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^expected \S+ on line \d+ of .+: not reported$").expect("rejection failure regex")
});

#[derive(Deserialize)]
struct KnownGaps {
    #[serde(default)]
    rejections: BTreeMap<String, Vec<String>>,
    #[serde(flatten)]
    executable: BTreeMap<String, Vec<String>>,
}

type Gaps<'a> = BTreeMap<&'a str, &'a str>;

struct Run {
    status: i32,
    stdout: String,
}

#[derive(Clone, Copy)]
struct Case<'a> {
    status: &'a str,
    kind: &'a str,
    name: &'a str,
    detail: &'a str,
}

impl Case<'_> {
    fn key(&self) -> (&str, &str) {
        (self.kind, self.name)
    }
}

fn case_rows(output: &str) -> Result<Vec<Case<'_>>> {
    let mut rows = Vec::new();
    let mut names = HashSet::new();
    for captured in CASE.captures_iter(output) {
        let row = Case {
            status: captured.get(1).expect("status").as_str(),
            kind: captured.get(2).expect("kind").as_str(),
            name: captured.get(3).expect("name").as_str(),
            detail: captured.get(4).map_or("", |detail| detail.as_str()),
        };
        if !names.insert((row.kind, row.name)) {
            return Err(format!("duplicate case in runner output: {}", row.name).into());
        }
        rows.push(row);
    }
    Ok(rows)
}

fn listed_cases(list: &KnownGaps) -> Result<(Gaps<'_>, BTreeSet<&str>)> {
    let mut gaps = BTreeMap::new();
    let mut rejections = BTreeSet::new();
    for (reason, names) in list.executable.iter().chain(&list.rejections) {
        for name in names {
            let suite = name
                .split_once("::")
                .map_or(name.as_str(), |(suite, _)| suite);
            if !SUITES.contains(&suite) || gaps.insert(name.as_str(), reason.as_str()).is_some() {
                return Err(
                    format!("duplicate or unknown case in unsupported_cases.toml: {name}").into(),
                );
            }
        }
    }
    rejections.extend(list.rejections.values().flatten().map(String::as_str));
    Ok((gaps, rejections))
}

fn rejection_key(name: &str) -> Option<String> {
    let found = REJECTION.captures(name)?;
    Some(format!("{}::{}::{}", &found[1], &found[2], &found[3]))
}

fn gap(detail: &str) -> Option<&str> {
    Some(UNSUPPORTED.captures(detail)?.get(1)?.as_str())
}

/// Require the same cases in discovery and execution. A listed rejection
/// must fail by missing its expected diagnostic, never for another reason.
fn verify(
    discovered: &Run,
    result: &Run,
    expected: &Gaps<'_>,
    unrejected: &BTreeSet<&str>,
) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    let inventory = case_rows(&discovered.stdout)?;
    let actual = case_rows(&result.stdout)?;
    let failed_rejection = inventory
        .iter()
        .any(|case| case.kind == "rejection" && case.status == "FAILED");
    let listing = if failed_rejection { 1 } else { 2 };
    if discovered.status != listing || !inventory.iter().any(|case| case.kind == "executable") {
        problems.push("the runner could not list the suite's executable cases".into());
    }

    let before: BTreeSet<_> = inventory.iter().map(Case::key).collect();
    let after: BTreeSet<_> = actual.iter().map(Case::key).collect();
    for (kind, name) in before.symmetric_difference(&after) {
        problems.push(format!("discovery and execution disagree on {kind} {name}"));
    }

    let mut counts = BTreeMap::<_, usize>::new();
    let mut blocked = BTreeSet::new();
    for Case {
        kind,
        name,
        status,
        detail,
    } in actual.iter().copied()
    {
        *counts.entry((kind, status)).or_default() += 1;
        if kind == "rejection"
            && let Some(listed) = rejection_key(name)
            && unrejected.contains(listed.as_str())
        {
            if status != "FAILED" || !NOT_REJECTED.is_match(detail) {
                problems.push(format!("listed rejection case is now {status}: {name}"));
            }
            blocked.insert(listed);
            continue;
        }
        if status == "FAILED" {
            problems.push(format!("{kind} failed: {name}"));
        }
        if status == "not run" {
            if kind != "executable"
                || gap(detail).is_none()
                || expected.get(name).copied() != gap(detail)
            {
                problems.push(format!(
                    "unexpected reason or case not run: {name}: {detail}"
                ));
            }
            blocked.insert(name.to_string());
        }
    }

    for &name in expected.keys().filter(|&&name| !blocked.contains(name)) {
        if unrejected.contains(name) {
            problems.push(format!("listed rejection case is now missing: {name}"));
        } else {
            let status = actual
                .iter()
                .find(|case| case.kind == "executable" && case.name == name)
                .map_or("missing", |case| case.status);
            problems.push(format!("listed target gap is now {status}: {name}"));
        }
    }

    let count = |kind, status| counts.get(&(kind, status)).copied().unwrap_or(0);
    let passed = count("executable", "passed");
    let failed = count("executable", "FAILED");
    let waiting = count("executable", "not run");
    let rejected = count("rejection", "passed");
    let unrejected = count("rejection", "FAILED");
    let summaries = [
        format!("executable cases: {passed} passed, {failed} failed, {waiting} not run"),
        format!("rejection cases: {rejected} passed, {unrejected} failed"),
    ];
    if !result
        .stdout
        .lines()
        .rev()
        .take(2)
        .eq(summaries.iter().map(String::as_str))
        || result.stdout.ends_with("\n\n")
    {
        problems.push("the runner's final counts do not match its case rows".into());
    }
    // Discovery and execution have different nonzero statuses; verify both.
    let status = if unrejected > 0 || failed > 0 {
        1
    } else if waiting > 0 {
        2
    } else {
        0
    };
    if result.status != status {
        problems.push(format!("unexpected runner exit status {}", result.status));
    }
    Ok(problems)
}

fn gaps_of(result: &Run) -> Result<Gaps<'_>> {
    Ok(case_rows(&result.stdout)?
        .into_iter()
        .filter_map(|case| {
            (case.kind == "executable" && case.status == "not run")
                .then(|| Some((case.name, gap(case.detail)?)))
                .flatten()
        })
        .collect())
}

fn toml_string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn write_list(path: &Path, text: &str, gaps: &Gaps<'_>, list: &KnownGaps) -> Result<()> {
    let lines: Vec<_> = text.lines().collect();
    let header = lines
        .iter()
        .take_while(|line| line.starts_with('#'))
        .count();
    let table = lines
        .iter()
        .position(|line| *line == "[rejections]")
        .unwrap_or(lines.len());
    let above = lines[header..table]
        .iter()
        .rev()
        .take_while(|line| line.starts_with('#'))
        .count();

    let mut by_reason = BTreeMap::<_, Vec<_>>::new();
    for (&name, &reason) in gaps {
        by_reason.entry(reason).or_default().push(name);
    }
    let mut written: Vec<_> = lines[..header].iter().map(ToString::to_string).collect();
    written.push(String::new());
    for (reason, names) in by_reason {
        written.push(format!("{} = [", toml_string(reason)));
        written.extend(
            names
                .into_iter()
                .map(|name| format!("    {},", toml_string(name))),
        );
        written.push("]".into());
    }
    written.push(String::new());
    written.extend(lines[table - above..table].iter().map(ToString::to_string));
    written.push("[rejections]".into());
    for (reason, names) in &list.rejections {
        let mut names: Vec<_> = names.iter().map(String::as_str).collect();
        names.sort_unstable();
        let names: Vec<_> = names.into_iter().map(toml_string).collect();
        written.push(format!("{} = [{}]", toml_string(reason), names.join(", ")));
    }
    fs::write(path, written.join("\n") + "\n")?;
    Ok(())
}

fn suite_args(core: &Path, suite: &str, execute: bool) -> Vec<OsString> {
    let root = core.join("tests").join(suite);
    let mut args = vec!["test".into(), root.into_os_string(), "--integration".into()];
    if execute {
        args.extend(["--target".into(), "cpp".into()]);
    }
    args
}

fn run(hgl: &Path, args: &[OsString]) -> Result<Run> {
    let result = Command::new(hgl)
        .args(args)
        .stderr(Stdio::inherit())
        .output()?;
    let status = result.status.code().unwrap_or_else(|| {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            -result.status.signal().unwrap_or(1)
        }
        #[cfg(not(unix))]
        {
            -1
        }
    });
    let stdout = String::from_utf8(result.stdout)?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    Ok(Run { status, stdout })
}

/// Run and check the pinned core suites without making core a source root.
pub(crate) fn check(hgl: &Path, core: &Path, regenerate_list: bool) -> Result<bool> {
    let hgl = hgl.canonicalize().or_else(|_| std::path::absolute(hgl))?;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../hgl_cpp/ci/unsupported_cases.toml");
    let list_text = fs::read_to_string(&path)?;
    let list: KnownGaps = toml::from_str(&list_text)?;
    let (gaps, unrejected) = listed_cases(&list)?;
    let mut problems = Vec::new();
    let mut found = BTreeMap::new();
    for suite in SUITES {
        println!("Running {suite}");
        io::stdout().flush()?;
        let discovery = run(&hgl, &suite_args(core, suite, false))?;
        let result = run(&hgl, &suite_args(core, suite, true))?;
        print!("{}", result.stdout);
        found.extend(
            gaps_of(&result)?
                .into_iter()
                .map(|(name, reason)| (name.to_string(), reason.to_string())),
        );
        let expected = gaps
            .iter()
            .filter(|(name, _)| name.starts_with(&format!("{suite}::")))
            .map(|(&name, &reason)| (name, reason))
            .collect();
        problems.extend(verify(&discovery, &result, &expected, &unrejected)?);
    }
    if regenerate_list {
        let references = found
            .iter()
            .map(|(name, reason)| (name.as_str(), reason.as_str()))
            .collect();
        write_list(&path, &list_text, &references, &list)?;
        println!("wrote {} cases to {}", found.len(), path.display());
        return Ok(true);
    }
    for problem in &problems {
        eprintln!("{problem}");
    }
    Ok(problems.is_empty())
}

#[cfg(test)]
mod tests;

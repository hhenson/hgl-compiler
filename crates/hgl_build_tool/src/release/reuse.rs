//! Find live release artifacts verified by the same attempt of a main push.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

use crate::{Result, text};

const REQUIRED: [&str; 5] = [
    "verify",
    "verify-debug",
    "hosted-check",
    "verify-linux",
    "verify-debug-linux",
];

#[derive(Deserialize)]
struct WorkflowRun {
    id: u64,
    head_sha: String,
    head_branch: String,
    event: String,
    status: String,
    run_attempt: u64,
}

#[derive(Deserialize)]
struct Job {
    name: String,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
struct Artifact {
    id: u64,
    name: String,
    expired: bool,
}

fn github_items(endpoint: &str, key: &str, fields: &[(&str, &str)]) -> Result<Vec<Value>> {
    let mut command = Command::new("gh");
    command.args([
        "api",
        "--method",
        "GET",
        "--paginate",
        "--jq",
        &format!(".{key}[] | @json"),
        endpoint,
    ]);
    for (key, value) in fields {
        command.args(["--raw-field", &format!("{key}={value}")]);
    }
    Ok(text(&mut command)?
        .lines()
        .map(serde_json::from_str)
        .collect::<serde_json::Result<_>>()?)
}

fn verified_run(
    repository: &str,
    commit: &str,
    current_run: &str,
    mut items: impl FnMut(&str, &str, &[(&str, &str)]) -> Result<Vec<Value>>,
) -> Result<Option<(u64, u64, u64)>> {
    let base = format!("repos/{repository}/actions");
    let runs = items(
        &format!("{base}/workflows/release.yml/runs"),
        "workflow_runs",
        &[
            ("branch", "main"),
            ("event", "push"),
            ("head_sha", commit),
            ("per_page", "100"),
        ],
    )?;
    for run in runs {
        let run: WorkflowRun = serde_json::from_value(run)?;
        if !matches!(run.status.as_str(), "in_progress" | "completed")
            || run.head_sha != commit
            || run.head_branch != "main"
            || run.event != "push"
            || run.id.to_string() == current_run
        {
            continue;
        }
        let WorkflowRun {
            id, run_attempt, ..
        } = run;
        let jobs = items(
            &format!("{base}/runs/{id}/attempts/{run_attempt}/jobs"),
            "jobs",
            &[("per_page", "100")],
        )?;
        let mut passed = BTreeSet::new();
        for job in jobs {
            let job: Job = serde_json::from_value(job)?;
            if job.conclusion.as_deref() == Some("success") {
                passed.insert(job.name);
            }
        }
        if !REQUIRED.iter().all(|job| passed.contains(*job)) {
            continue;
        }
        let artifacts = items(
            &format!("{base}/runs/{id}/artifacts"),
            "artifacts",
            &[("per_page", "100")],
        )?;
        let mut live = BTreeMap::new();
        for artifact in artifacts {
            let artifact: Artifact = serde_json::from_value(artifact)?;
            if !artifact.expired {
                live.insert(artifact.name, artifact.id);
            }
        }
        if let (Some(macos), Some(linux)) = (
            live.get(&format!("hgl-macos-arm64-{run_attempt}")),
            live.get(&format!("hgl-linux-x86_64-{run_attempt}")),
        ) {
            return Ok(Some((id, *macos, *linux)));
        }
    }
    Ok(None)
}

/// Print the reuse workflow outputs if both verified bundles are still live.
pub(crate) fn find_verified_run(repository: &str, commit: &str, current_run: &str) -> Result<()> {
    if let Some((run, macos, linux)) = verified_run(repository, commit, current_run, github_items)?
    {
        println!("run-id={run}\nartifact-id={macos}\nlinux-artifact-id={linux}");
    }
    Ok(())
}

#[cfg(test)]
mod tests;

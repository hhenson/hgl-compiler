//! Reuse requires live artifacts from the particular verified attempt.

use serde_json::json;

use super::*;

struct Responses {
    run: Value,
    jobs: Vec<Value>,
    artifacts: Vec<Value>,
}

impl Default for Responses {
    fn default() -> Self {
        Self {
            run: json!({ "id": 2, "head_sha": "same", "head_branch": "main", "event": "push",
                "status": "in_progress", "run_attempt": 3 }),
            jobs: REQUIRED
                .into_iter()
                .map(|name| json!({ "name": name, "conclusion": "success" }))
                .collect(),
            artifacts: vec![
                json!({ "id": 10, "name": "hgl-macos-arm64-3", "expired": false }),
                json!({ "id": 11, "name": "hgl-linux-x86_64-3", "expired": false }),
            ],
        }
    }
}

impl Responses {
    fn verified_run(&self) -> Option<(u64, u64, u64)> {
        verified_run("test/project", "same", "1", |endpoint, key, fields| {
            assert_eq!(fields.last(), Some(&("per_page", "100")));
            let relative = endpoint
                .strip_prefix("repos/test/project/actions/")
                .unwrap();
            Ok(match (relative, key) {
                ("workflows/release.yml/runs", "workflow_runs") => {
                    assert_eq!(
                        fields,
                        [
                            ("branch", "main"),
                            ("event", "push"),
                            ("head_sha", "same"),
                            ("per_page", "100")
                        ]
                    );
                    vec![self.run.clone()]
                }
                ("runs/2/attempts/3/jobs", "jobs") => self.jobs.clone(),
                ("runs/2/artifacts", "artifacts") => self.artifacts.clone(),
                _ => panic!("unexpected list: {endpoint}/{key}"),
            })
        })
        .unwrap()
    }
}

#[test]
fn workspace_test_does_not_gate_verification_reuse() {
    let mut fixture = Responses::default();
    for (status, outcome) in [
        ("in_progress", Value::Null),
        ("completed", json!("failure")),
        ("completed", json!("success")),
    ] {
        fixture.run["status"] = json!(status);
        fixture
            .jobs
            .push(json!({ "name": "cpp-macos", "conclusion": outcome }));
        assert_eq!(fixture.verified_run(), Some((2, 10, 11)));
        fixture.jobs.pop();
    }
}

#[test]
fn every_verifier_has_to_succeed() {
    let mut fixture = Responses::default();
    for index in 0..fixture.jobs.len() {
        for outcome in [Value::Null, json!("failure"), json!("skipped")] {
            fixture.jobs[index]["conclusion"] = outcome;
            assert_eq!(fixture.verified_run(), None);
        }
        fixture.jobs[index]["conclusion"] = json!("success");
    }
    fixture.jobs.pop();
    assert_eq!(fixture.verified_run(), None);
}

#[test]
fn verifiers_match_publish_dependencies() {
    let source = include_str!("../../../../../.github/workflows/release.yml");
    let published = source.split_once("\n  publish:\n").unwrap().1;
    let needs = published
        .lines()
        .find_map(|line| line.strip_prefix("    needs: ["))
        .unwrap();
    let needed: BTreeSet<_> = needs
        .strip_suffix(']')
        .unwrap()
        .split(',')
        .map(str::trim)
        .filter(|job| !["reuse-build", "bundle", "bundle-linux"].contains(job))
        .collect();
    assert_eq!(needed, REQUIRED.into_iter().collect());
}

#[test]
fn artifacts_must_be_live_from_the_verified_attempt() {
    let mut fixture = Responses::default();
    fixture.artifacts[1]["expired"] = json!(true);
    assert_eq!(fixture.verified_run(), None);
    fixture.artifacts[1]["expired"] = json!(false);
    fixture.artifacts[1]["name"] = json!("hgl-linux-x86_64-2");
    assert_eq!(fixture.verified_run(), None);
}

#[test]
fn the_run_must_match_source_and_exclude_the_current_run() {
    let mut fixture = Responses::default();
    for (field, wrong) in [
        ("head_sha", json!("different")),
        ("head_branch", json!("branch")),
        ("event", json!("workflow_dispatch")),
        ("id", json!(1)),
        ("status", json!("queued")),
    ] {
        let previous = std::mem::replace(&mut fixture.run[field], wrong);
        assert_eq!(fixture.verified_run(), None);
        fixture.run[field] = previous;
    }
}

//! Wiring contracts for the release gate.
//!
//! A green CI workflow protects a release only when the tag workflow invokes
//! it and every artifact-producing or publishing path depends on that result.

use std::collections::{BTreeMap, BTreeSet};

const CI_WORKFLOW: &str = include_str!("../.github/workflows/ci.yml");
const RELEASE_WORKFLOW: &str = include_str!("../.github/workflows/release.yml");

/// Normalize CRLF -> LF. `include_str!` embeds the working-tree bytes at compile
/// time, and the GitHub Windows runner checks the repo out with autocrlf=true,
/// so ci.yml/release.yml arrive with `\r\n`. These contracts assert on the
/// wiring (embedded `\n` substrings), not on line-ending style, so normalize
/// first. The `had_crlf` witnesses below make a CRLF cause visible rather than
/// inferred if an assertion ever fails.
fn lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[test]
fn ci_is_callable_at_the_tagged_commit() {
    let ci = lf(CI_WORKFLOW);
    let release = lf(RELEASE_WORKFLOW);
    assert!(
        ci.contains("on:\n  workflow_call:\n"),
        "release.yml can gate on CI only if ci.yml is a reusable workflow; ci.yml had_crlf={}",
        CI_WORKFLOW.contains("\r\n")
    );
    assert!(
        release.contains("  ci:\n    name: CI\n    uses: ./.github/workflows/ci.yml\n"),
        "the release workflow must invoke ci.yml at the tagged commit; release.yml had_crlf={}",
        RELEASE_WORKFLOW.contains("\r\n")
    );
}

/// Steps in a job produce an artifact or publish a release when they carry one
/// of these substrings — in a `run:` (cargo publish) or a `uses:` (the release
/// and PyPI actions, or any artifact upload). Detecting the job by what it DOES
/// keeps the contract true across needs-graph edits: the literal edges may
/// change freely, and the invariant "every such job reaches ci" still holds.
const PUBLISH_MARKERS: &[&str] = &[
    "cargo publish",
    "softprops/action-gh-release",
    "pypa/gh-action-pypi-publish",
    "actions/upload-artifact",
];

/// Parse a workflow's `jobs` into (needs graph, publishing-job set). `needs`
/// accepts YAML's scalar (`needs: ci`) and sequence (`needs: [ci, build]`)
/// forms; a job is a publisher when any step's `run` or `uses` carries a
/// PUBLISH_MARKERS substring.
fn parse_jobs(yaml: &str) -> (BTreeMap<String, BTreeSet<String>>, BTreeSet<String>) {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml).expect("workflow parses as YAML");
    let jobs = doc
        .get("jobs")
        .and_then(|j| j.as_mapping())
        .expect("workflow has a jobs mapping");

    let mut needs_graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut publishers: BTreeSet<String> = BTreeSet::new();

    for (name, spec) in jobs {
        let name = name.as_str().expect("job name is a string").to_string();

        let mut needs = BTreeSet::new();
        match spec.get("needs") {
            Some(serde_yaml::Value::String(s)) => {
                needs.insert(s.clone());
            }
            Some(serde_yaml::Value::Sequence(seq)) => {
                for item in seq {
                    if let Some(s) = item.as_str() {
                        needs.insert(s.to_string());
                    }
                }
            }
            _ => {}
        }
        needs_graph.insert(name.clone(), needs);

        if let Some(steps) = spec.get("steps").and_then(|s| s.as_sequence()) {
            for step in steps {
                let run = step.get("run").and_then(|v| v.as_str()).unwrap_or("");
                let uses = step.get("uses").and_then(|v| v.as_str()).unwrap_or("");
                if PUBLISH_MARKERS
                    .iter()
                    .any(|m| run.contains(m) || uses.contains(m))
                {
                    publishers.insert(name.clone());
                    break;
                }
            }
        }
    }

    (needs_graph, publishers)
}

/// Does `job` reach `ci` through its transitive `needs` closure?
fn reaches_ci(job: &str, needs_graph: &BTreeMap<String, BTreeSet<String>>) -> bool {
    let mut stack = vec![job.to_string()];
    let mut seen: BTreeSet<String> = BTreeSet::new();
    while let Some(cur) = stack.pop() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        if let Some(deps) = needs_graph.get(&cur) {
            for dep in deps {
                if dep == "ci" {
                    return true;
                }
                stack.push(dep.clone());
            }
        }
    }
    false
}

/// Publishing/artifact jobs whose transitive `needs` closure omits `ci`.
/// Panics if the workflow declares no publishing job at all: without that
/// guard the invariant would pass vacuously the day a refactor renames every
/// marker, which is the same silent-stale failure this rewrite exists to end.
fn publishing_jobs_missing_ci(yaml: &str) -> Vec<String> {
    let (needs_graph, publishers) = parse_jobs(yaml);
    assert!(
        !publishers.is_empty(),
        "no publishing/artifact job detected — PUBLISH_MARKERS are stale, not the \
         graph; this check would otherwise pass vacuously"
    );
    publishers
        .iter()
        .filter(|j| !reaches_ci(j, &needs_graph))
        .cloned()
        .collect()
}

#[test]
fn every_publishing_job_reaches_ci_in_its_needs_closure() {
    let release = lf(RELEASE_WORKFLOW);
    let missing = publishing_jobs_missing_ci(&release);
    assert!(
        missing.is_empty(),
        "every artifact-producing or publishing job must reach ci through its needs \
         closure, so a red CI stops the tag; jobs not reaching ci: {missing:?}; \
         release.yml had_crlf={}",
        RELEASE_WORKFLOW.contains("\r\n")
    );
}

#[test]
fn the_closure_check_reds_when_a_needs_edge_is_removed() {
    // Control: the real workflow flags nothing, so the mutation below is what
    // makes the checker speak. If this control ever reds, the negative fixture
    // proves nothing and must be re-derived.
    let release = lf(RELEASE_WORKFLOW);
    assert!(
        publishing_jobs_missing_ci(&release).is_empty(),
        "control: real release.yml must pass before the mutation can prove anything"
    );
    // Mutation: sever build's only edge to ci. build uploads artifacts, so it is
    // a publishing job and must now be flagged as no longer reaching ci.
    let severed = release.replacen(
        "  build:\n    name: Build (${{ matrix.target }})\n    needs: ci\n",
        "  build:\n    name: Build (${{ matrix.target }})\n    needs: []\n",
        1,
    );
    assert_ne!(
        severed, release,
        "the mutation must actually edit the yaml (the anchored build block moved?)"
    );
    let flagged = publishing_jobs_missing_ci(&severed);
    assert!(
        flagged.contains(&"build".to_string()),
        "severing build->ci must flag build; flagged={flagged:?}"
    );
}

#[test]
#[should_panic(expected = "no publishing/artifact job detected")]
fn publisher_detection_finding_nothing_panics_instead_of_passing_vacuously() {
    // Witness for the vacuous-pass guard itself: a workflow with jobs but no
    // publishing/artifact step must make publishing_jobs_missing_ci PANIC, not
    // return an empty "all clear". Without this, deleting the guard's assert
    // leaves every other test green while the invariant silently checks nothing
    // the day a refactor renames every marker.
    let no_publisher = "\
jobs:
  ci:
    name: CI
    uses: ./.github/workflows/ci.yml
  lint:
    name: Lint
    needs: ci
    steps:
      - run: cargo clippy --all-targets
";
    let _ = publishing_jobs_missing_ci(no_publisher);
}

#[test]
fn crates_publish_runs_package_verification() {
    let release = lf(RELEASE_WORKFLOW);
    assert!(
        release.contains("run: cargo publish --locked"),
        "cargo publish must run with --locked so a Cargo.lock drift fails the \
         publish loudly instead of silently regenerating the lock (changed from \
         --allow-dirty in 8ca60c2; publish-crates is a fresh checkout with no \
         build step, so the tree is clean at publish time)"
    );
    assert!(
        !release.contains("cargo publish --no-verify"),
        "release publication must never bypass package verification"
    );
}

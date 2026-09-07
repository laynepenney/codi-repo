"""`review run <lane-dir>`: the review-owned in-lane test run — the last raw-shell
exit point (venv + install + pytest by hand) folded into one verb.

It runs ONLY inside an `open-gr --enter` reconstruction lane (it reads the
`.grip-open-gr-reconstruct.json` marker), so a green is always about a bound tree.
Two structural bindings make the green mean something:

  * THE TREE COMPARISON — the lane's current working tree must equal the marker's
    bound head-tree. One comparison catches both a wrong reconstruction and a lane
    drifted after open (a touched tracked file changes the tree). Asserted BEFORE
    the venv is created, so the venv never pollutes the hash.
  * IMPORT UNDER THE LANE — after install, the package's resolved `__file__` must
    live under the lane, or a second checkout shadowing it on `PYTHONPATH` would let
    a pass be about someone else's tree (the stale editable-install trap).

Counts come from pytest's SUMMARY LINE, never the exit code (a run that collected
zero tests exits 0). A run that collects/selects zero tests, or whose summary line
cannot be parsed, is a REFUSAL, not a green.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

_MARKER_NAME = ".grip-open-gr-reconstruct.json"
_RECEIPT_NAME = ".grip-review-run.json"
_VENV_DIRNAME = ".venv"


class ReviewRunRefused(Exception):
    """A structural refusal: the run cannot yield a trustworthy green."""

    def __init__(self, code: str, detail: str) -> None:
        self.code = code
        self.detail = detail
        super().__init__(f"{code}: {detail}")


def _git(repo_dir: Path, *args: str, env: dict | None = None) -> str:
    proc = subprocess.run(
        ["git", "-C", str(repo_dir), *args],
        text=True,
        capture_output=True,
        env=env,
    )
    if proc.returncode != 0:
        raise ReviewRunRefused(
            "git_failed",
            f"git {' '.join(args)} in {repo_dir} exited {proc.returncode}: "
            f"{proc.stderr.strip()}",
        )
    return proc.stdout.strip()


# ---- the tree comparison (drift + reconstruction, ONE check) ----------------

def compute_working_tree(repo_dir: Path) -> str:
    """The tree hash of the current TRACKED content of repo_dir, computed in a
    throwaway index so the real index is untouched. `add -u` stages modifications
    and deletions of tracked files but NOT untracked additions, so the open-gr
    marker, the lane `.venv`, and the run receipt do not read as drift — only a
    change to a reconstructed (tracked) file does."""
    with tempfile.TemporaryDirectory() as td:
        idx = str(Path(td) / "index")
        env = {**os.environ, "GIT_INDEX_FILE": idx}
        _git(repo_dir, "read-tree", "HEAD", env=env)
        _git(repo_dir, "add", "-u", env=env)
        return _git(repo_dir, "write-tree", env=env)


def assert_lane_tree_bound(repo_dir: Path, bound_head_tree: str) -> str:
    """THE tracked-tree comparison. Refuse unless the lane's current tracked tree
    equals the bound head-tree recorded at open. Returns the computed tree. Dropping
    this comparison lets a MODIFIED tracked file pass as a green."""
    if not bound_head_tree:
        raise ReviewRunRefused(
            "no_bound_tree",
            f"the open-gr marker for {repo_dir} records no bound_head_tree; "
            "reopen the lane with a build that records it",
        )
    current = compute_working_tree(repo_dir)
    if current != bound_head_tree:
        raise ReviewRunRefused(
            "tree_drift",
            f"lane tree {current} != bound head-tree {bound_head_tree} "
            f"({repo_dir}); the run would not be about the pinned head",
        )
    return current


# Untracked paths the run itself is expected to create; everything else untracked in
# the lane is drift, because an injected conftest.py or module can change what the
# tests do WITHOUT touching the tracked tree (which `assert_lane_tree_bound` sees).
_UNTRACKED_ALLOW_NAMES = frozenset({_MARKER_NAME, _RECEIPT_NAME})
_UNTRACKED_ALLOW_TOP = (_VENV_DIRNAME + "/",)
_UNTRACKED_ALLOW_SEGMENTS = frozenset({"__pycache__", ".pytest_cache", ".mypy_cache"})


def _is_allowlisted_untracked(rel_path: str) -> bool:
    if rel_path in _UNTRACKED_ALLOW_NAMES:
        return True
    if any(rel_path == pre.rstrip("/") or rel_path.startswith(pre) for pre in _UNTRACKED_ALLOW_TOP):
        return True
    segments = rel_path.strip("/").split("/")
    if any(seg in _UNTRACKED_ALLOW_SEGMENTS or seg.endswith(".egg-info") for seg in segments):
        return True
    return False


def assert_no_untracked_drift(repo_dir: Path) -> None:
    """Refuse if the lane holds any untracked path the run did not create. Without
    this an untracked `conftest.py` (or shadow module) that patches the package turns
    a failing tree green while the tracked-tree comparison passes. The complement of
    `assert_lane_tree_bound`: dropping either reds only its own drift witness."""
    out = _git(repo_dir, "status", "--porcelain")
    offending = []
    for line in out.splitlines():
        if line.startswith("?? "):
            rel = line[3:].strip().strip('"')
            if not _is_allowlisted_untracked(rel):
                offending.append(rel)
    if offending:
        raise ReviewRunRefused(
            "untracked_drift",
            f"untracked path(s) in the lane the run did not create: "
            f"{', '.join(offending[:5])}; an injected conftest/module can change test "
            "behavior without touching the tracked tree",
        )


# ---- import resolves under the lane -----------------------------------------

def resolve_import_file(venv_python: Path, package: str, env: dict) -> str:
    """Import `package` in the venv python (under `env`) and return its __file__."""
    proc = subprocess.run(
        [str(venv_python), "-c", f"import {package} as _m; print(_m.__file__ or '')"],
        text=True,
        capture_output=True,
        env=env,
    )
    if proc.returncode != 0:
        raise ReviewRunRefused(
            "import_failed",
            f"could not import {package!r} in the lane venv: {proc.stderr.strip()}",
        )
    path = proc.stdout.strip()
    if not path:
        raise ReviewRunRefused(
            "import_no_file",
            f"{package!r} has no __file__ (namespace package?); cannot bind the "
            "install to the lane",
        )
    return path


def assert_import_under_lane(resolved_file: str, lane_dir: Path) -> None:
    """Refuse unless the resolved import path lives under the lane. A second checkout
    on PYTHONPATH would otherwise let a pass be about a different tree."""
    p = Path(resolved_file).resolve()
    root = lane_dir.resolve()
    if root != p and root not in p.parents:
        raise ReviewRunRefused(
            "import_escapes_lane",
            f"{p} does not resolve under the lane {root}; a checkout outside the "
            "lane is shadowing the reconstruction (stale editable install)",
        )


# ---- pytest summary parsing (counts from the summary line, not exit code) ----

_COLLECTED_RE = re.compile(r"collected (\d+) item")
_SELECTED_RE = re.compile(r"(\d+) selected")
# A summary line ends with "in <time>s" (barred in normal mode, bare in -q), or is
# the "no tests ran in <time>s" line; leading/trailing "=" bars are optional.
_SUMMARY_LINE_RE = re.compile(r"(?:in \d+\.\d+s|no tests ran)")
_TIME_TAIL_RE = re.compile(r"\bin \d+\.\d+s\b|\bno tests ran\b")
_COUNT_RE = re.compile(
    r"(\d+) (passed|failed|error|errors|skipped|xfailed|xpassed|deselected|warning|warnings)"
)


def parse_pytest_summary(stdout: str) -> dict | None:
    """Counts from pytest's SUMMARY LINE, never the exit code. Handles both the
    barred normal-mode line (`===== 3 passed in 0.01s =====`) and the bare `-q` line
    (`1 passed in 0.00s`, `1 deselected in 0.00s`). Returns a dict with
    collected/selected/deselected/passed/failed/skipped/xfailed/errors, or None if
    no summary line exists (unparseable output -> the caller refuses). A run where no
    test ran yields selected==0 (the caller refuses)."""
    lines = stdout.splitlines()
    summary_body: str | None = None
    for line in reversed(lines):
        if _SUMMARY_LINE_RE.search(line):
            summary_body = line.strip().strip("=").strip()
            break
    if summary_body is None:
        return None

    counts = {k: 0 for k in ("passed", "failed", "errors", "skipped", "xfailed", "xpassed")}
    deselected = 0
    for n, word in _COUNT_RE.findall(summary_body):
        if word in ("error", "errors"):
            counts["errors"] = int(n)
        elif word in ("warning", "warnings"):
            continue
        elif word == "deselected":
            deselected = int(n)
        else:
            counts[word] = int(n)

    collected = None
    selected = None
    for line in lines:
        cm = _COLLECTED_RE.search(line)
        if cm:
            collected = int(cm.group(1))
        sm = _SELECTED_RE.search(line)
        if sm:
            selected = int(sm.group(1))
        dm = re.search(r"(\d+) deselected", line)
        if dm:
            deselected = max(deselected, int(dm.group(1)))

    ran = (
        counts["passed"] + counts["failed"] + counts["errors"]
        + counts["skipped"] + counts["xfailed"] + counts["xpassed"]
    )
    if "no tests ran" in summary_body:
        selected = 0
    elif selected is None:
        selected = ran
    return {
        "collected": collected,
        "deselected": deselected,
        "selected": selected,
        **counts,
    }


# ---- the verb ---------------------------------------------------------------

def _read_marker(lane_dir: Path) -> dict:
    marker_path = lane_dir / _MARKER_NAME
    if not marker_path.exists():
        raise ReviewRunRefused(
            "no_marker",
            f"no open-gr marker at {marker_path}; `review run` only runs inside a "
            "lane opened by `review open-gr --enter`",
        )
    marker = json.loads(marker_path.read_text())
    if marker.get("kind") != "open-gr-reconstruct":
        raise ReviewRunRefused(
            "not_open_gr", f"{marker_path} is not an open-gr reconstruction marker"
        )
    return marker


def run_review_lane(
    lane_dir: Path,
    *,
    package: str,
    pytest_args: list[str],
    python: str | None = None,
    install: list[str] | None = None,
    system_site_packages: bool = False,
) -> dict:
    """Create `<lane>/.venv`, install the reconstructed tree, and run pytest — but
    only after the lane's tree is proven to equal the bound head-tree and the import
    is proven to resolve under the lane. Returns a receipt. Raises ReviewRunRefused
    for any structural problem (no marker, tree drift, import escape, zero collected,
    unparseable summary)."""
    lane_dir = Path(lane_dir).resolve()
    marker = _read_marker(lane_dir)
    repos = marker.get("repos", [])
    if len(repos) != 1:
        raise ReviewRunRefused(
            "multi_repo_lane",
            f"v1 review run handles a single-repo lane; marker binds {len(repos)} "
            "repos (multi-repo is a follow-on)",
        )
    repo = repos[0]
    bound_tree = repo.get("bound_head_tree", "")
    repo_dir = lane_dir  # single-repo lane: the clone IS the lane

    # (1) THE TREE COMPARISON — before the venv exists, so it never pollutes the hash.
    #     Two halves: tracked content equals the bound tree, AND no untracked path the
    #     run did not create (an injected conftest changes behavior invisibly to the
    #     tracked-tree hash).
    assert_lane_tree_bound(repo_dir, bound_tree)
    assert_no_untracked_drift(repo_dir)

    # (2) venv in the lane, so close-gr reclaims it.
    interpreter = python or sys.executable
    venv_dir = lane_dir / _VENV_DIRNAME
    venv_cmd = [interpreter, "-m", "venv"]
    if system_site_packages:
        venv_cmd.append("--system-site-packages")
    venv_cmd.append(str(venv_dir))
    proc = subprocess.run(venv_cmd, text=True, capture_output=True)
    if proc.returncode != 0:
        raise ReviewRunRefused("venv_failed", f"venv create failed: {proc.stderr.strip()}")
    venv_python = venv_dir / "bin" / "python"

    # (3) install the reconstructed tree editable.
    install_cmd = install or [str(venv_python), "-m", "pip", "install", "-e", str(repo_dir)]
    proc = subprocess.run(install_cmd, text=True, capture_output=True, cwd=str(repo_dir))
    if proc.returncode != 0:
        raise ReviewRunRefused(
            "install_failed",
            f"install `{' '.join(install_cmd)}` failed: {proc.stderr.strip()[-800:]}",
        )

    # (4) IMPORT UNDER THE LANE — in the same env pytest will use.
    run_env = {**os.environ}
    resolved_file = resolve_import_file(venv_python, package, run_env)
    assert_import_under_lane(resolved_file, lane_dir)

    # (5) run pytest; counts from the summary line, never the exit code.
    test_cmd = [str(venv_python), "-m", "pytest", *pytest_args]
    proc = subprocess.run(
        test_cmd, text=True, capture_output=True, cwd=str(repo_dir), env=run_env
    )
    summary = parse_pytest_summary(proc.stdout + "\n" + proc.stderr)
    if summary is None:
        raise ReviewRunRefused(
            "unparseable_summary",
            "no pytest summary line found; refusing to call this a green "
            f"(pytest exit was {proc.returncode})",
        )
    if not summary.get("selected"):
        raise ReviewRunRefused(
            "zero_collected",
            f"pytest selected 0 tests (collected={summary.get('collected')}, "
            f"deselected={summary.get('deselected')}); a zero-test run is not a green",
        )

    version = subprocess.run(
        [str(venv_python), "--version"], text=True, capture_output=True
    ).stdout.strip() or subprocess.run(
        [str(venv_python), "-V"], text=True, capture_output=True
    ).stderr.strip()

    # A green requires at least one PASS: an all-skipped or all-deselected run has no
    # failure but proves nothing, so it is not a green.
    result = (
        "green"
        if (summary["passed"] >= 1 and summary["failed"] == 0 and summary["errors"] == 0)
        else "red"
    )
    receipt = {
        "kind": "review-run",
        "gr_commit": marker.get("gr_commit", ""),
        "bound_head": repo.get("bound_head", ""),
        "bound_head_tree": bound_tree,
        "interpreter": {"path": str(venv_python), "version": version},
        "resolved_install_path": resolved_file,
        "test_command": test_cmd,
        "collected": summary["collected"],
        "deselected": summary["deselected"],
        "selected": summary["selected"],
        "passed": summary["passed"],
        "failed": summary["failed"],
        "skipped": summary["skipped"],
        "xfailed": summary["xfailed"],
        "errors": summary["errors"],
        "result": result,
    }
    (lane_dir / _RECEIPT_NAME).write_text(json.dumps(receipt, indent=2) + "\n")
    return receipt

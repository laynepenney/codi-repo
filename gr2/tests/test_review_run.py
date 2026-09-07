"""`review run <lane-dir>`: the review-owned in-lane test run (venv + install +
pytest folded into one verb). A green is trustworthy only because it is bound two
ways — the lane tree equals the bound head-tree, and the import resolves under the
lane — and because counts come from pytest's summary line, never the exit code.

Fast unit witnesses cover the two bindings, the summary parser, and the refusals;
an integration witness runs a real venv end-to-end (offline: the install command
seeds a `.pth`, and the current sys.path is added so host pytest is importable).
"""
from __future__ import annotations

import json
import site
import subprocess
import sys
from pathlib import Path

import pytest

from gr2.python_cli import review_run as rr


def _git(cwd: Path, *a: str) -> str:
    return subprocess.run(["git", *a], cwd=cwd, text=True, capture_output=True, check=True).stdout.strip()


def _pkg_repo(tmp_path: Path, *, test_body: str) -> tuple[Path, str]:
    """A git repo holding a trivial installable package `demo_pkg` and a test file.
    Returns (repo_dir, head_tree)."""
    repo = tmp_path / "lane"
    (repo / "src" / "demo_pkg").mkdir(parents=True)
    (repo / "tests").mkdir()
    (repo / "src" / "demo_pkg" / "__init__.py").write_text("VALUE = 1\n")
    (repo / "pyproject.toml").write_text(
        "[build-system]\nrequires=['setuptools']\nbuild-backend='setuptools.build_meta'\n"
        "[project]\nname='demo_pkg'\nversion='0.0.0'\n"
        "[tool.setuptools.packages.find]\nwhere=['src']\n"
    )
    (repo / "tests" / "test_demo.py").write_text(test_body)
    _git(repo, "init", "-q", "-b", "main")
    _git(repo, "config", "user.email", "a@e.invalid")
    _git(repo, "config", "user.name", "a")
    _git(repo, "add", ".")
    _git(repo, "commit", "-q", "-m", "pkg")
    head = _git(repo, "rev-parse", "HEAD")
    head_tree = _git(repo, "rev-parse", "HEAD^{tree}")
    return repo, head_tree


def _write_marker(lane_dir: Path, head: str, head_tree: str, *, key: str = "alpha") -> None:
    marker = {
        "kind": "open-gr-reconstruct",
        "gr_commit": "deadbeef" * 5,
        "repos": [{
            "key": key,
            "reconstructed_head": head,
            "bound_head": head,
            "bound_head_tree": head_tree,
            "reconstructed_tree": head_tree,
            "tree_match": True,
        }],
    }
    (lane_dir / rr._MARKER_NAME).write_text(json.dumps(marker, indent=2) + "\n")


# offline install: seed a .pth so demo_pkg resolves under the lane AND host pytest
# (on the current sys.path) is importable in the lane venv — no network.
def _offline_install(lane: Path) -> list[str]:
    vpy = lane / rr._VENV_DIRNAME / "bin" / "python"
    paths = [str(lane / "src"), *[p for p in sys.path if p]]
    script = (
        "import site,sys,pathlib;"
        "sp=pathlib.Path(site.getsitepackages()[0]);"
        "sp.mkdir(parents=True,exist_ok=True);"
        "(sp/'zz_lane.pth').write_text('\\n'.join(sys.argv[1:])+'\\n')"
    )
    return [str(vpy), "-c", script, *paths]


PASS_TEST = "from demo_pkg import VALUE\n\ndef test_ok():\n    assert VALUE == 1\n"


# ---------------------------------------------------------------- summary parse

def test_parse_summary_normal_green():
    s = rr.parse_pytest_summary("collected 3 items\n\n===== 3 passed in 0.01s =====\n")
    assert s is not None and s["selected"] == 3 and s["passed"] == 3 and s["failed"] == 0


def test_parse_summary_counts_from_the_line_not_the_exit_code():
    s = rr.parse_pytest_summary("collected 3 items\n\n=== 1 failed, 2 passed in 0.1s ===\n")
    assert s["passed"] == 2 and s["failed"] == 1


def test_parse_summary_no_tests_ran_is_zero_selected():
    s = rr.parse_pytest_summary(
        "collected 3800 items / 3800 deselected / 0 selected\n\n"
        "===== no tests ran in 0.20s =====\n"
    )
    assert s is not None and s["selected"] == 0 and s["deselected"] == 3800


def test_parse_summary_records_the_selection_visibly():
    s = rr.parse_pytest_summary(
        "collected 3800 items / 3620 deselected / 180 selected\n\n"
        "===== 180 passed, 3620 deselected in 2.0s =====\n"
    )
    assert s["collected"] == 3800 and s["deselected"] == 3620 and s["selected"] == 180


def test_parse_summary_unparseable_is_none():
    assert rr.parse_pytest_summary("Traceback...\nImportError: boom\n") is None


# ---------------------------------------------------- THE tree comparison + drift

def test_tree_bound_passes_on_a_pristine_reconstruction(tmp_path: Path):
    repo, head_tree = _pkg_repo(tmp_path, test_body=PASS_TEST)
    assert rr.assert_lane_tree_bound(repo, head_tree) == head_tree


def test_tree_bound_refuses_a_lane_drifted_after_open(tmp_path: Path):
    # Drift witness: touch a tracked file after open -> the working tree no longer
    # equals the bound head-tree. THE mutation that drops the tree comparison must
    # red THIS test alone.
    repo, head_tree = _pkg_repo(tmp_path, test_body=PASS_TEST)
    (repo / "src" / "demo_pkg" / "__init__.py").write_text("VALUE = 999\n")  # drift
    with pytest.raises(rr.ReviewRunRefused, match="tree_drift"):
        rr.assert_lane_tree_bound(repo, head_tree)


def test_tree_bound_refuses_when_the_marker_records_no_bound_tree(tmp_path: Path):
    repo, _ = _pkg_repo(tmp_path, test_body=PASS_TEST)
    with pytest.raises(rr.ReviewRunRefused, match="no_bound_tree"):
        rr.assert_lane_tree_bound(repo, "")


# ---------------------------------------------------- untracked drift (the 2nd half)

def test_untracked_drift_passes_with_only_run_created_paths(tmp_path: Path):
    # Pristine control: the marker, the lane .venv, an egg-info, and __pycache__ are
    # all things the open/run create -> not drift.
    repo, head_tree = _pkg_repo(tmp_path, test_body=PASS_TEST)
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), head_tree)
    (repo / ".venv" / "bin").mkdir(parents=True)
    (repo / ".venv" / "bin" / "python").write_text("")
    (repo / "src" / "demo_pkg.egg-info").mkdir()
    (repo / "src" / "demo_pkg.egg-info" / "PKG-INFO").write_text("")
    (repo / "__pycache__").mkdir()
    (repo / "__pycache__" / "x.pyc").write_text("")
    rr.assert_no_untracked_drift(repo)  # no raise


def test_untracked_drift_refuses_an_injected_conftest(tmp_path: Path):
    # An untracked conftest.py in the lane root is invisible to `add -u`, so the
    # tracked-tree comparison passes; the untracked scan is what catches it. Dropping
    # the untracked scan reds THIS witness alone.
    repo, head_tree = _pkg_repo(tmp_path, test_body=PASS_TEST)
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), head_tree)
    (repo / "conftest.py").write_text("# injected, not tracked\n")
    assert rr.assert_lane_tree_bound(repo, head_tree) == head_tree  # tracked tree still 'clean'
    with pytest.raises(rr.ReviewRunRefused, match="untracked_drift"):
        rr.assert_no_untracked_drift(repo)


# ----------------------------------------------------------- import under the lane

def test_import_under_the_lane_accepts_a_path_inside(tmp_path: Path):
    lane = tmp_path / "lane"
    (lane / "src" / "demo_pkg").mkdir(parents=True)
    inside = lane / "src" / "demo_pkg" / "__init__.py"
    inside.write_text("")
    rr.assert_import_under_lane(str(inside), lane)  # no raise


def test_import_under_the_lane_refuses_a_path_outside(tmp_path: Path):
    lane = tmp_path / "lane"
    lane.mkdir()
    outside = tmp_path / "other_checkout" / "demo_pkg" / "__init__.py"
    outside.parent.mkdir(parents=True)
    outside.write_text("")
    with pytest.raises(rr.ReviewRunRefused, match="import_escapes_lane"):
        rr.assert_import_under_lane(str(outside), lane)


# --------------------------------------------------------------- verb refusals

def test_run_refuses_a_dir_with_no_marker(tmp_path: Path):
    d = tmp_path / "not-a-lane"
    d.mkdir()
    with pytest.raises(rr.ReviewRunRefused, match="no_marker"):
        rr.run_review_lane(d, package="demo_pkg", pytest_args=[])


def test_run_refuses_a_multi_repo_lane(tmp_path: Path):
    lane = tmp_path / "lane"
    lane.mkdir()
    marker = {
        "kind": "open-gr-reconstruct", "gr_commit": "x",
        "repos": [{"key": "a", "bound_head_tree": "t1"}, {"key": "b", "bound_head_tree": "t2"}],
    }
    (lane / rr._MARKER_NAME).write_text(json.dumps(marker))
    with pytest.raises(rr.ReviewRunRefused, match="multi_repo_lane"):
        rr.run_review_lane(lane, package="demo_pkg", pytest_args=[])


# ------------------------------------------------------------------ integration

def test_run_green_records_a_bound_receipt(tmp_path: Path):
    repo, head_tree = _pkg_repo(tmp_path, test_body=PASS_TEST)
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), head_tree)
    receipt = rr.run_review_lane(
        repo, package="demo_pkg", pytest_args=["-q"], install=_offline_install(repo),
    )
    assert receipt["result"] == "green"
    assert receipt["selected"] >= 1 and receipt["passed"] >= 1 and receipt["failed"] == 0
    assert receipt["bound_head_tree"] == head_tree
    # the install is bound to the lane
    assert str(repo.resolve()) in receipt["resolved_install_path"]
    # the exact test command is recorded (Stromus addition 3)
    assert receipt["test_command"][-1] == "-q" and "pytest" in receipt["test_command"]
    # receipt persisted in the lane, so close-gr reclaims it too
    assert (repo / rr._RECEIPT_NAME).exists()


def test_run_refuses_when_a_k_filter_selects_zero_tests(tmp_path: Path):
    # Stromus addition 2: zero selected is a refusal, not a green — from the summary
    # line, not the exit code (pytest exits 0 having run nothing).
    repo, head_tree = _pkg_repo(tmp_path, test_body=PASS_TEST)
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), head_tree)
    with pytest.raises(rr.ReviewRunRefused, match="zero_collected"):
        rr.run_review_lane(
            repo, package="demo_pkg",
            pytest_args=["-q", "-k", "no_such_test_name_matches_this"],
            install=_offline_install(repo),
        )


def test_run_refuses_an_untracked_conftest_that_would_fake_a_pass(tmp_path: Path):
    # The central-claim probe (Stromus R2): an untracked conftest.py that patches the
    # package turns a red tree green. The run must REFUSE before it can run — the
    # tracked tree is unchanged, so only the untracked scan stops it.
    repo, head_tree = _pkg_repo(tmp_path, test_body="def test_ok():\n    assert False\n")
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), head_tree)
    (repo / "conftest.py").write_text(
        "import demo_pkg\n\n"
        "def pytest_configure(config):\n"
        "    demo_pkg.VALUE = 1  # a patch the tracked tree never shows\n"
    )
    with pytest.raises(rr.ReviewRunRefused, match="untracked_drift"):
        rr.run_review_lane(
            repo, package="demo_pkg", pytest_args=["-q"], install=_offline_install(repo),
        )


def test_run_all_skipped_is_not_green(tmp_path: Path):
    # Stromus R2: a green needs passed >= 1. An all-skipped run has no failure but
    # proves nothing. Dropping the passed>=1 condition reds THIS witness.
    repo, head_tree = _pkg_repo(
        tmp_path, test_body="import pytest\n\ndef test_x():\n    pytest.skip('nope')\n"
    )
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), head_tree)
    receipt = rr.run_review_lane(
        repo, package="demo_pkg", pytest_args=["-q"], install=_offline_install(repo),
    )
    assert receipt["result"] != "green"
    assert receipt["passed"] == 0 and receipt["skipped"] >= 1 and receipt["selected"] >= 1


def test_run_refuses_when_the_import_escapes_the_lane(tmp_path: Path):
    # Stromus addition 4b: a second checkout shadowing the package via PYTHONPATH is
    # a refusal. Install NOTHING under the lane; put a rogue demo_pkg on PYTHONPATH.
    repo, head_tree = _pkg_repo(tmp_path, test_body=PASS_TEST)
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), head_tree)
    rogue = tmp_path / "rogue_checkout"
    (rogue / "demo_pkg").mkdir(parents=True)
    (rogue / "demo_pkg" / "__init__.py").write_text("VALUE = 2\n")
    # install command that only makes host pytest importable (NOT demo_pkg under lane)
    vpy = repo / rr._VENV_DIRNAME / "bin" / "python"
    script = (
        "import site,sys,pathlib;"
        "sp=pathlib.Path(site.getsitepackages()[0]);sp.mkdir(parents=True,exist_ok=True);"
        "(sp/'zz_host.pth').write_text('\\n'.join(sys.argv[1:])+'\\n')"
    )
    install = [str(vpy), "-c", script, *[p for p in sys.path if p]]
    import os
    old = os.environ.get("PYTHONPATH")
    os.environ["PYTHONPATH"] = str(rogue) + (os.pathsep + old if old else "")
    try:
        with pytest.raises(rr.ReviewRunRefused, match="import_escapes_lane"):
            rr.run_review_lane(repo, package="demo_pkg", pytest_args=["-q"], install=install)
    finally:
        if old is None:
            os.environ.pop("PYTHONPATH", None)
        else:
            os.environ["PYTHONPATH"] = old

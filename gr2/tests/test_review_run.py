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
import shlex
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


# ------------------------------------------ install hint + pytest-absent cause

_SEED_SCRIPT = (
    "import site,sys,pathlib;"
    "sp=pathlib.Path(site.getsitepackages()[0]);"
    "sp.mkdir(parents=True,exist_ok=True);"
    "(sp/'zz_lane.pth').write_text('\\n'.join(sys.argv[1:])+'\\n')"
)


def _offline_install_no_pytest(lane: Path) -> list[str]:
    """Seed a .pth with ONLY the package's src — NOT host sys.path — so demo_pkg
    imports but pytest is absent from the lane venv."""
    vpy = lane / rr._VENV_DIRNAME / "bin" / "python"
    return [str(vpy), "-c", _SEED_SCRIPT, str(lane / "src")]


def _hint_install_string() -> str:
    """The offline install as a .review-install `install =` value, with {venv}/{lane}
    placeholders review run substitutes; includes host sys.path so pytest resolves."""
    paths = ["{lane}/src", *[p for p in sys.path if p]]
    return shlex.join(["{venv}", "-c", _SEED_SCRIPT, *paths])


def test_run_refuses_pytest_absent_with_the_named_cause(tmp_path: Path):
    # half 1: an install that brings the package but NOT pytest must
    # refuse `pytest_not_installed`, NOT `unparseable_summary` (the wrong cause the
    # dogfood hit). Keep it a refusal either way.
    repo, head_tree = _pkg_repo(tmp_path, test_body=PASS_TEST)
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), head_tree)
    with pytest.raises(rr.ReviewRunRefused) as exc:
        rr.run_review_lane(
            repo, package="demo_pkg", pytest_args=["-q"],
            install=_offline_install_no_pytest(repo),
        )
    assert exc.value.code == "pytest_not_installed", exc.value.code


def test_review_install_hint_supplies_install_and_package(tmp_path: Path):
    # half 2: a repo that declares .review-install (tracked, so it is part
    # of the bound tree) runs green with NO --install and NO --package.
    repo, _ = _pkg_repo(tmp_path, test_body=PASS_TEST)
    (repo / ".review-install").write_text(
        f"# hint\ninstall = {_hint_install_string()}\npackage = demo_pkg\n"
    )
    _git(repo, "add", ".")
    _git(repo, "commit", "-q", "-m", "hint")
    head = _git(repo, "rev-parse", "HEAD")
    head_tree = _git(repo, "rev-parse", "HEAD^{tree}")
    _write_marker(repo, head, head_tree)
    receipt = rr.run_review_lane(repo, pytest_args=["-q"])  # no install, no package
    assert receipt["result"] == "green", receipt
    assert receipt["passed"] >= 1


def test_no_review_install_hint_still_needs_the_flags(tmp_path: Path):
    # Control: a repo WITHOUT the hint does not auto-resolve — read returns None and a
    # run with neither flag refuses `no_package`, so the hint is what removes the flag.
    repo, head_tree = _pkg_repo(tmp_path, test_body=PASS_TEST)
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), head_tree)
    assert rr.read_install_hint(repo) is None
    with pytest.raises(rr.ReviewRunRefused) as exc:
        rr.run_review_lane(repo, pytest_args=["-q"])  # no hint, no --package
    assert exc.value.code == "no_package", exc.value.code


# ---------------------------------------------------- v3: R2 REQUEST-CHANGES items

def test_hint_bad_binary_refuses_install_failed(tmp_path: Path):
    # P3 (the block): a committed hint whose install names a binary that does not
    # exist must REFUSE `install_failed` naming the command, never raise an uncaught
    # OSError traceback (the one shape review run promises never to give).
    repo, _ = _pkg_repo(tmp_path, test_body=PASS_TEST)
    (repo / ".review-install").write_text(
        "install = /nonexistent/binary --boom\npackage = demo_pkg\n"
    )
    _git(repo, "add", ".")
    _git(repo, "commit", "-q", "-m", "bad hint")
    head = _git(repo, "rev-parse", "HEAD")
    head_tree = _git(repo, "rev-parse", "HEAD^{tree}")
    _write_marker(repo, head, head_tree)
    with pytest.raises(rr.ReviewRunRefused) as exc:
        rr.run_review_lane(repo, pytest_args=["-q"])
    assert exc.value.code == "install_failed", exc.value.code
    assert "/nonexistent/binary" in exc.value.detail


def test_receipt_records_install_command_and_sources(tmp_path: Path):
    # P7: a hint-driven run and a flag-driven run must NOT render identical receipts.
    # Hint run: source is `hint`, and install_command shows the substituted command.
    repo, _ = _pkg_repo(tmp_path, test_body=PASS_TEST)
    (repo / ".review-install").write_text(
        f"install = {_hint_install_string()}\npackage = demo_pkg\n"
    )
    _git(repo, "add", ".")
    _git(repo, "commit", "-q", "-m", "hint")
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), _git(repo, "rev-parse", "HEAD^{tree}"))
    rcpt = rr.run_review_lane(repo, pytest_args=["-q"])
    assert rcpt["result"] == "green", rcpt
    assert rcpt["install_source"] == "hint", rcpt["install_source"]
    assert rcpt["package_source"] == "hint", rcpt["package_source"]
    assert any(str(repo / "src") in tok for tok in rcpt["install_command"]), rcpt["install_command"]

    # Flag run on a fresh lane: source is `flag`.
    repo2, head_tree2 = _pkg_repo(tmp_path / "second", test_body=PASS_TEST)
    _write_marker(repo2, _git(repo2, "rev-parse", "HEAD"), head_tree2)
    rcpt2 = rr.run_review_lane(
        repo2, package="demo_pkg", install=_offline_install(repo2), pytest_args=["-q"]
    )
    assert rcpt2["result"] == "green", rcpt2
    assert rcpt2["install_source"] == "flag", rcpt2["install_source"]
    assert rcpt2["package_source"] == "flag", rcpt2["package_source"]


def test_unknown_hint_key_refuses_bad_hint(tmp_path: Path):
    # P1: a typo'd key (`instal`) must refuse `bad_hint` naming the key, not fall
    # through to the default install and refuse under a cause the repo never declared.
    repo, _ = _pkg_repo(tmp_path, test_body=PASS_TEST)
    (repo / ".review-install").write_text("instal = whoops\npackage = demo_pkg\n")
    with pytest.raises(rr.ReviewRunRefused) as exc:
        rr.read_install_hint(repo)
    assert exc.value.code == "bad_hint", exc.value.code
    assert "instal" in exc.value.detail


# ------------------------------------- follow-on: undeclared extra (pip exits 0)

def test_detect_undeclared_extras_parses_pip_warning():
    # The pure detector: pip's real warning (captured from pip 25 against a package
    # with no extras), older pip's version-less spelling, and a clean install (none).
    out = (
        "Obtaining file:///x\n"
        "WARNING: demo_pkg 0.0.0 does not provide the extra 'alsobad'\n"
        "WARNING: demo_pkg 0.0.0 does not provide the extra 'bogus'\n"
        "Successfully installed demo_pkg-0.0.0\n"
    )
    assert rr.detect_undeclared_extras(out) == ["alsobad", "bogus"]  # sorted, unique
    assert rr.detect_undeclared_extras(
        "WARNING: pkg does not provide the extra 'x'"  # older pip: no version
    ) == ["x"]
    assert rr.detect_undeclared_extras("Successfully installed demo_pkg-0.0.0") == []


def _install_no_pytest_with_undeclared_extra_warning(lane: Path, extra: str) -> list[str]:
    """Like `_offline_install_no_pytest` (brings the package, NOT pytest) but also
    prints pip's real undeclared-extra WARNING on stderr and exits 0 — the exact
    shape of `pip install -e <lane>[<extra>]` when <extra> is a typo: pip warns,
    exits 0, and installs none of that extra's dependencies."""
    vpy = lane / rr._VENV_DIRNAME / "bin" / "python"
    script = (
        _SEED_SCRIPT
        + ";import sys;sys.stderr.write("
        + repr(f"WARNING: demo_pkg 0.0.0 does not provide the extra '{extra}'\n")
        + ")"
    )
    return [str(vpy), "-c", script, str(lane / "src")]


def test_run_refuses_undeclared_extra_naming_it_before_pytest(tmp_path: Path):
    # Root-cause naming. This is the SAME scenario as the pytest-absent test — the
    # install brings the package but not pytest — except the install ALSO emits pip's
    # undeclared-extra warning (a typo'd `[devv]`). The undeclared-extra check fires
    # first, so the run refuses `undeclared_extra` naming `devv` instead of the
    # misleading `pytest_not_installed` the reviewer would otherwise chase. Removing
    # the (4a) block flips this to `pytest_not_installed` (mutation witness).
    repo, head_tree = _pkg_repo(tmp_path, test_body=PASS_TEST)
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), head_tree)
    with pytest.raises(rr.ReviewRunRefused) as exc:
        rr.run_review_lane(
            repo, package="demo_pkg", pytest_args=["-q"],
            install=_install_no_pytest_with_undeclared_extra_warning(repo, "devv"),
        )
    assert exc.value.code == "undeclared_extra", exc.value.code
    assert "devv" in exc.value.detail


def test_spaced_lane_path_survives(tmp_path: Path):
    # P2: split the template FIRST, then substitute per token, so a lane path with a
    # space survives even with an unquoted {venv}/{lane} in the hint line. The prior
    # code substituted before shlex.split, so the space broke the token apart; this
    # test's helper writes the placeholders UNQUOTED (only the seed script + host
    # paths are quoted), which the earlier author suite's shlex.join helper could not.
    spaced = tmp_path / "has space"
    spaced.mkdir()
    repo, _ = _pkg_repo(spaced, test_body=PASS_TEST)
    assert " " in str(repo), str(repo)
    host = [shlex.quote(p) for p in sys.path if p]
    hint_install = " ".join(
        ["{venv}", "-c", shlex.quote(_SEED_SCRIPT), "{lane}/src", *host]
    )
    (repo / ".review-install").write_text(f"install = {hint_install}\npackage = demo_pkg\n")
    _git(repo, "add", ".")
    _git(repo, "commit", "-q", "-m", "spaced hint")
    _write_marker(repo, _git(repo, "rev-parse", "HEAD"), _git(repo, "rev-parse", "HEAD^{tree}"))
    rcpt = rr.run_review_lane(repo, pytest_args=["-q"])
    assert rcpt["result"] == "green", rcpt
    assert rcpt["passed"] >= 1

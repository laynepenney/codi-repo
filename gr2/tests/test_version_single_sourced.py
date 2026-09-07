"""Witness: the distribution version is single-sourced from the repo-root
Cargo.toml at build time, so a published wheel says which grip it belongs to.

The in-tree build backend (gr2/_build/backend.py) writes a VERSION file from
``../Cargo.toml``'s ``[package] version`` before every build hook, and
pyproject's ``[tool.setuptools.dynamic]`` reads the wheel version from that file.
These tests pin both ends of that chain: the backend writes exactly Cargo's
version, and pyproject is wired to read it dynamically from VERSION (never a
hand-maintained literal that could drift from the Rust crate).
"""
from __future__ import annotations

import subprocess
import sys
import tomllib
from pathlib import Path

_GR2_ROOT = Path(__file__).resolve().parents[1]  # gr2/ (the packaging root)
_CARGO = _GR2_ROOT.parent / "Cargo.toml"  # repo-root Cargo.toml, one level up

sys.path.insert(0, str(_GR2_ROOT / "_build"))
from version_from_cargo import sync_version  # noqa: E402  (setuptools-free helper)


def _cargo_version() -> str:
    return tomllib.loads(_CARGO.read_text(encoding="utf-8"))["package"]["version"]


def test_backend_writes_VERSION_equal_to_cargo() -> None:
    """The build backend's version helper (the setuptools-free
    _build/version_from_cargo.sync_version, which backend._sync_version calls)
    writes a VERSION whose contents equal Cargo.toml's [package] version. Run in a
    subprocess with cwd = the packaging root, as PEP 517 invokes the backend."""
    proc = subprocess.run(
        [
            sys.executable,
            "-c",
            "import sys, pathlib; sys.path.insert(0, '_build'); "
            "from version_from_cargo import sync_version; "
            "print(sync_version(pathlib.Path.cwd()))",
        ],
        cwd=_GR2_ROOT,
        capture_output=True,
        text=True,
    )
    assert proc.returncode == 0, proc.stderr
    written = proc.stdout.strip().splitlines()[-1]
    assert written == _cargo_version(), (written, _cargo_version())
    # and the file it wrote agrees
    assert (_GR2_ROOT / "VERSION").read_text().strip() == _cargo_version()


def test_pyproject_declares_version_dynamic_from_VERSION() -> None:
    """The version must be declared dynamic and sourced from the VERSION file, so
    the backend's write is what setuptools reads — not a static literal in
    pyproject that a bump could leave stale."""
    cfg = tomllib.loads((_GR2_ROOT / "pyproject.toml").read_text(encoding="utf-8"))
    assert "version" in cfg["project"].get("dynamic", []), cfg["project"]
    assert "version" not in cfg["project"], "version must not also be a static literal"
    dyn = cfg["tool"]["setuptools"]["dynamic"]["version"]
    assert dyn == {"file": "VERSION"}, dyn
    assert cfg["build-system"]["build-backend"] == "backend", cfg["build-system"]


def test_foreign_cargo_is_ignored_version_comes_from_VERSION(tmp_path) -> None:
    """An sdist unpacked one level below an UNRELATED Rust project must not inherit
    that project's version (measured: a foreign Cargo at 7.7.7 minted gitgrip-7.7.7
    while the sdist's own VERSION said 1.4.0). Foreign Cargo present, sdist VERSION
    present -> the VERSION wins. Removing the [package] name == gitgrip gate
    (mutation) makes this return 7.7.7 and reddens."""
    (tmp_path / "Cargo.toml").write_text('[package]\nname = "other"\nversion = "7.7.7"\n')
    root = tmp_path / "pkg"
    root.mkdir()
    (root / "VERSION").write_text("1.4.0\n")
    assert sync_version(root) == "1.4.0"


def test_grip_cargo_is_used(tmp_path) -> None:
    """A Cargo whose [package] name IS gitgrip is trusted and written to VERSION."""
    (tmp_path / "Cargo.toml").write_text('[package]\nname = "gitgrip"\nversion = "9.9.9"\n')
    root = tmp_path / "pkg"
    root.mkdir()
    assert sync_version(root) == "9.9.9"
    assert (root / "VERSION").read_text().strip() == "9.9.9"


def test_foreign_cargo_and_no_VERSION_refuses(tmp_path) -> None:
    """A foreign Cargo with no sdist VERSION to fall back on must REFUSE, never
    invent a version from the foreign crate."""
    import pytest

    (tmp_path / "Cargo.toml").write_text('[package]\nname = "other"\nversion = "7.7.7"\n')
    root = tmp_path / "pkg"
    root.mkdir()
    with pytest.raises(RuntimeError):
        sync_version(root)

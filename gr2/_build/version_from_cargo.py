"""Pure version-single-sourcing helper — no setuptools import, so it is testable
in a plain runtime venv. The build backend (backend.py) delegates to it.

Writes a VERSION file from the repo-root ``Cargo.toml``'s ``[package] version``,
but ONLY when that Cargo.toml is grip's own crate (``[package] name`` ==
``gitgrip``). Any other ``../Cargo.toml`` is ignored — an sdist unpacked one
level below an unrelated Rust project must not inherit that project's version
(measured: a foreign Cargo at 7.7.7 minted gitgrip-7.7.7 while the sdist's own
VERSION said 1.4.0). When grip's Cargo is not reachable (a wheel built FROM an
unpacked sdist), the VERSION that rode inside the sdist is used. Never invents a
version, and never trusts a foreign crate's.
"""
from __future__ import annotations

import pathlib
import tomllib

_CRATE_NAME = "gitgrip"


def _grip_cargo_version(cargo: pathlib.Path) -> str | None:
    """The [package] version of ``cargo`` iff it is grip's own crate, else None."""
    if not cargo.is_file():
        return None
    pkg = tomllib.loads(cargo.read_text(encoding="utf-8")).get("package", {})
    if pkg.get("name") != _CRATE_NAME:
        return None  # a foreign Cargo.toml — do not trust its version
    return pkg.get("version")


def sync_version(root: pathlib.Path) -> str:
    root = pathlib.Path(root)
    version_file = root / "VERSION"
    version = _grip_cargo_version(root.parent / "Cargo.toml")
    if version is not None:
        version_file.write_text(version + "\n", encoding="utf-8")
        return version
    if version_file.is_file() and version_file.read_text(encoding="utf-8").strip():
        return version_file.read_text(encoding="utf-8").strip()
    raise RuntimeError(
        f"cannot single-source version at {root}: no grip Cargo.toml "
        f"([package] name = {_CRATE_NAME!r}) above it and no populated VERSION file"
    )

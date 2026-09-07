"""In-tree PEP 517 build backend: single-source the distribution version from
the repo-root ``Cargo.toml`` so a published wheel's version says which grip it
belongs to (never a hand-maintained 0.1.0 drifting from the Rust crate).

It wraps setuptools.build_meta: before every build hook it (re)writes a
``VERSION`` file next to pyproject.toml from ``../Cargo.toml``'s
``[package] version``. ``[tool.setuptools.dynamic] version = {file = "VERSION"}``
then reads it. Cargo.toml lives one level above the packaging root (gr2/), so it
is present for a direct wheel build and for the sdist build; a wheel built FROM
an sdist has no Cargo.toml (it is outside the packaging root), and there VERSION
travels inside the sdist (MANIFEST.in) and is left as-is.

requires-python is >=3.11, so tomllib is always stdlib — no build dependency.
"""
from __future__ import annotations

import pathlib

from setuptools import build_meta as _bm

from version_from_cargo import sync_version as _sync

# Re-export the hooks we do not override so the frontend finds a complete backend.
get_requires_for_build_wheel = _bm.get_requires_for_build_wheel
get_requires_for_build_sdist = _bm.get_requires_for_build_sdist
get_requires_for_build_editable = _bm.get_requires_for_build_editable


def _sync_version() -> None:
    """Write VERSION from ../Cargo.toml (via the setuptools-free helper, so the
    version logic is testable in a plain venv). PEP 517 runs the backend with
    cwd = the packaging root."""
    _sync(pathlib.Path.cwd())


def build_wheel(wheel_directory, config_settings=None, metadata_directory=None):
    _sync_version()
    return _bm.build_wheel(wheel_directory, config_settings, metadata_directory)


def build_sdist(sdist_directory, config_settings=None):
    _sync_version()
    return _bm.build_sdist(sdist_directory, config_settings)


def prepare_metadata_for_build_wheel(metadata_directory, config_settings=None):
    _sync_version()
    return _bm.prepare_metadata_for_build_wheel(metadata_directory, config_settings)


def build_editable(wheel_directory, config_settings=None, metadata_directory=None):
    _sync_version()
    return _bm.build_editable(wheel_directory, config_settings, metadata_directory)


def prepare_metadata_for_build_editable(metadata_directory, config_settings=None):
    _sync_version()
    return _bm.prepare_metadata_for_build_editable(metadata_directory, config_settings)

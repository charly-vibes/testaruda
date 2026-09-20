"""Tests for validate-imports.py's Python import parsing (testaruda-rpqs).

The script's filename contains a dash, so it's loaded via importlib.

Run: python3 -m pytest scripts/test_validate_imports.py
"""

import importlib.util
import os
import sys

import pytest

_SCRIPT = os.path.join(os.path.dirname(__file__), "validate-imports.py")
_spec = importlib.util.spec_from_file_location("validate_imports", _SCRIPT)
vi = importlib.util.module_from_spec(_spec)
sys.modules["validate_imports"] = vi
_spec.loader.exec_module(vi)


class TestRelativeImportResolution:
    """Relative imports must resolve against the file's parent package as a
    list of dotted segments — not by iterating the dotted string char-by-char
    (testaruda-rpqs)."""

    FILE = "tests/admin_docs/test_utils.py"

    def test_truth_relative_current_package(self):
        content = "from .tests import AdminDocsSimpleTestCase\n"
        got = vi._py_ast_imports(content, self.FILE, "/repo")
        assert got == {"tests"}, f"got {got!r} — char-iteration artifact?"

    def test_adapter_replica_relative_current_package(self):
        content = "from .tests import AdminDocsSimpleTestCase\n"
        got = vi._py_adapter_imports(content, self.FILE)
        assert got == {"tests"}, f"got {got!r} — char-iteration artifact?"

    def test_truth_and_replica_agree_on_relative(self):
        content = "from .sibling import Thing\nfrom ..aunt import Other\n"
        path = "pkg/sub/mod.py"
        assert vi._py_ast_imports(content, path, "/repo") == vi._py_adapter_imports(
            content, path
        )


class TestTopLevelSegment:
    def test_truth_absolute(self):
        assert vi._py_ast_imports("import unittest\n", "a/b/mod.py", "/repo") == {
            "unittest"
        }

    def test_adapter_replica_absolute(self):
        assert vi._py_adapter_imports("import unittest\n", "a/b/mod.py") == {"unittest"}


class TestCommentStrip:
    """testaruda-wpil: trailing comments must not leak into module names."""

    def test_adapter_replica_lazy_with_noqa(self):
        got = vi._py_adapter_imports("def f():\n    import ctypes  # noqa: F401\n", "t.py")
        assert got == {"ctypes"}, f"got {got!r}"

    def test_truth_ignores_comment(self):
        got = vi._py_ast_imports("import ctypes  # noqa: F401\n", "t.py", "/repo")
        assert got == {"ctypes"}, f"got {got!r}"

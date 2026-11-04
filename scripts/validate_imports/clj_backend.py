"""Clojure backend: :require/:import/use form parsing."""

import os
import re

from validate_imports.base import LanguageBackend


class ClojureBackend(LanguageBackend):

    @property
    def name(self) -> str: return "clojure"

    @property
    def source_extensions(self) -> tuple[str, ...]: return (".clj", ".cljs", ".cljc", ".edn")

    def is_test_file(self, rel_path: str) -> bool:
        base = os.path.basename(rel_path)
        return "/test/" in rel_path or base.startswith("test_") or base.endswith("_test.clj")

    def extract_imports_ast(self, content: str, file_path: str, repo_root: str) -> set[str]:
        return _clj_parse_imports(content)

    def extract_imports_adapter(self, content: str, file_path: str, repo_root: str) -> set[str]:
        return _clj_parse_imports(content)


def _clj_require_parts(block: str) -> set[str]:
    """Deps from one :require [...] block (namespace/:as/:refer style)."""
    deps: set[str] = set()
    for part in block.split():
        part = part.strip().rstrip("]")
        if not part or part.startswith(":") or part.startswith("["):
            continue
        deps.add(part.split("/")[0] if "/" in part else part.split(".")[0])
    return deps


def _clj_import_parts(block: str) -> set[str]:
    """Package prefixes from one :import [...] block (dotted class names)."""
    deps: set[str] = set()
    for part in block.split():
        part = part.strip()
        if part and not part.startswith(":") and "." in part:
            deps.add(part.split(".")[0])
    return deps


def _clj_parse_imports(content: str) -> set[str]:
    """Parse Clojure :require, :import, and use forms.

    Matches :require [namespace :as alias] and :require [[ns1] [ns2]] and
    bare :require namespace.
    """
    deps: set[str] = set()
    for line in content.split("\n"):
        t = line.strip()
        for block in re.findall(r":require\s+\[([^\]]+)\]", t):
            deps |= _clj_require_parts(block)
        for block in re.findall(r":import\s+\[([^\]]+)\]", t):
            deps |= _clj_import_parts(block)
        for mod in re.findall(r"\(use\s+'([^\s)]+)", t):
            deps.add(mod.split("/")[0])
    return deps

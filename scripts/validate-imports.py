#!/usr/bin/env python3
"""
validate-imports.py — Multi-language adapter import parsing validator.

Validates each language adapter's import detection by comparing against
ground-truth extracted via the language's own parser (ast, syn, etc.).

Implementation lives in the validate_imports package (split for the
300-line script gate); this file stays the executable shim and re-exports
the parse functions that scripts/test_validate_imports.py asserts on.

Usage:
  ./scripts/validate-imports.py [options] <repo-path>

Options:
  --language LANG  Force language (auto-detect from repo by default)
  --adapter PATH   Adapter binary path (auto-detect from config by default)
  --output FILE    Write JSON results to file
  --list-languages Show supported languages and exit
  --help           Show this help

Supported languages: python, rust, julia, typescript, clojure
"""

import sys

# scripts/ is the import root when run directly; make the package importable
# from any working directory.
sys.path.insert(0, __file__.rsplit("/", 1)[0])

from validate_imports.python_backend import _py_adapter_imports, _py_ast_imports  # noqa: E402,F401
from validate_imports.cli import main  # noqa: E402

if __name__ == "__main__":
    main()

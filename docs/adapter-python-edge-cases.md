# Python Adapter Edge Case Catalog

**Ticket:** testaruda-64o  
**Status:** Draft  
**Last updated:** 2026-07-21

This document catalogs unusual Python project structures that could break the
Python adapter's `discover`, `static-deps`, `fingerprint`, or `run-args` commands.
Each edge case includes a severity rating and a recommendation.

## Severity Scale

| Rating | Meaning |
|--------|---------|
| **blocker** | Produces incorrect results; must fix before production use |
| **major** | Breaks common project patterns; should fix |
| **minor** | Breaks uncommon patterns; fix when convenient |
| **cosmetic** | Works correctly but could be improved |

---

## Discover

### Edge 1: src/ layout (vs flat layout)

**Scenario:** Source code lives in `src/`, tests in `tests/`. Import paths use
the package name as root (e.g., `from src.model import Model`).

**Affected commands:** discover, static-deps

**Current behavior:** The adapter's WalkDir starts at `"."` and finds all files
including those in `src/`. The `test_` / `_test.py` filter correctly excludes
source files from test discovery. The `file_path_to_module` function correctly
converts `src/model.py` → `src.model`.

**Expected behavior:** No change needed — the adapter already handles this layout
correctly.

**Minimal reproduction:**
```
my_project/
├── src/
│   ├── __init__.py
│   └── model.py
└── tests/
    ├── __init__.py
    └── test_model.py    # from src.model import Model
```

**Fixture needed?** No — works with current code. Test via existing integration
test fixture.

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 2: Tests in separate directory (tests/) vs co-located

**Scenario:** Some projects put tests in a `tests/` directory, others co-locate
them alongside source files (`src/package/test_*.py`).

**Affected commands:** discover

**Current behavior:** The WalkDir finds all `test_*.py` / `*_test.py` files
regardless of location. Both patterns work.

**Expected behavior:** Same as current.

**Minimal reproduction:** Both layouts work:
```
# Co-located
src/myapp/test_models.py    # ← discovered
src/myapp/models.py         # ← not discovered (not a test file)
```
```
# Separate
tests/test_models.py        # ← discovered
```

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 3: Namespace packages (PEP 420)

**Scenario:** A package directory without `__init__.py`. Python 3.3+ treats
these as namespace packages, allowing multiple directories to contribute to
the same package.

**Affected commands:** discover, static-deps

**Current behavior:** The adapter doesn't check for `__init__.py` to determine
package boundaries. Discover walks the directory tree and finds all test files.
`file_path_to_module` works on file paths, not package structure, so namespace
packages don't affect path-to-module conversion.

**However**, import resolution for relative imports could be wrong. The
`parse_python_imports` function resolves relative imports by counting dots and
trimming from the base package path. For namespace packages, the resolution
assumes `__init__.py` boundaries, which don't exist.

**Expected behavior:** Relative imports in namespace packages should resolve
correctly. This requires understanding that namespace packages don't introduce
a package level — they just expose subdirectories.

**Minimal reproduction:**
```
project/
├── pkg_a/
│   └── model.py          # no __init__.py
└── pkg_b/
    └── test_model.py     # from ..pkg_a.model import Model
                          # ← 2 dots: should go up 1 level (project root),
                          #    then into pkg_a
```

**Fixture needed?** Yes — synthetic fixture

**Severity:** minor
**Recommendation:** fix adapter — relative import resolution should account for
namespace packages

---

### Edge 4: `__init__.py` with test discovery implications

**Scenario:** An `__init__.py` file that contains test functions, or test files
placed inside a package directory alongside `__init__.py`.

**Affected commands:** discover

**Current behavior:** The adapter discovers `test_*.py` / `*_test.py` files
regardless of directory. `__init__.py` is not a test file (doesn't match the
pattern), so it's correctly excluded.

**Expected behavior:** Same as current.

**Minimal reproduction:**
```
mypkg/
├── __init__.py
└── test_features.py    # ← discovered correctly
```

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 5: conftest.py at multiple levels

**Scenario:** pytest `conftest.py` files at the project root, `tests/` directory,
and nested test subdirectories (providing shared fixtures, hooks, and plugins).

**Affected commands:** discover

**Current behavior:** The adapter doesn't treat `conftest.py` as a test file
(doesn't match `test_` or `_test.py`), so it's correctly excluded from
discovery results. This is fine because conftest files don't contain tests
(conventionally).

**Expected behavior:** Same as current.

**Minimal reproduction:**
```
project/
├── conftest.py
├── tests/
│   ├── conftest.py
│   ├── test_a.py
│   └── sub/
│       ├── conftest.py
│       └── test_b.py
```

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 6: Non-standard test file naming

**Scenario:** Test files that don't use the `test_` prefix or `_test.py` suffix:
- `check_*.py` (nose convention)
- `*_spec.py` (RSpec convention)
- `test*.py` without underscore (e.g., `testmain.py`)
- `*_test.py` with extra underscores (e.g., `feature_test.py` — this one works)

**Affected commands:** discover

**Current behavior:** The filter `fname.starts_with("test_") && fname.ends_with(".py")
|| fname.ends_with("_test.py")` misses:
- `check_*.py` files
- `*_spec.py` files  
- `test*.py` files without underscore prefix (e.g., `testmain.py`)

**Expected behavior:** The adapter should also detect `check_*.py` and `*_spec.py`
patterns, or make the test file pattern configurable.

**Recommendation:** Two sub-recommendations:
1. Add `check_` prefix support for nose compatibility (e.g., `fname.starts_with("check_")`)
2. Add `*_spec.py` suffix support for RSpec-like conventions (e.g., `fname.ends_with("_spec.py")`)
Alternatively, make test file patterns configurable per project via `testaruda.toml`.

**Minimal reproduction:**
```
tests/
├── test_main.py        # ← discovered
├── check_models.py    # ← NOT discovered
├── api_spec.py        # ← NOT discovered
└── testSmoke.py       # ← NOT discovered
```

**Fixture needed?** Yes — synthetic fixture for `check_` and `_spec` patterns

**Severity:** major
**Recommendation:** fix adapter — add `check_` prefix and `_spec.py` suffix, or
make test file patterns configurable per project

---

### Edge 7: Deeply nested packages

**Scenario:** Source code organized in deeply nested package directories
(e.g., `src/a/b/c/d/e/f/model.py`).

**Affected commands:** discover, static-deps

**Current behavior:** WalkDir handles arbitrary nesting depth. `file_path_to_module`
correctly converts `src/a/b/c/d/e/f/model.py` → `src.a.b.c.d.e.f.model`.
Relative import resolution correctly handles deep nesting via dot counting.

**Expected behavior:** Same as current.

**Minimal reproduction:**
```
src/a/b/c/d/e/f/
├── __init__.py
└── model.py
tests/a/b/c/d/e/f/
└── test_model.py     # from src.a.b.c.d.e.f.model import Model
```

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 8: Non-UTF-8 filenames in tests/sources

**Scenario:** Test or source files with non-ASCII characters in their names
(e.g., `test_ünicöde.py`, `test_日本語.py`, `test_emoji_😀.py`).

**Affected commands:** discover, static-deps, fingerprint

**Current behavior:** On Linux, file paths are byte sequences. Rust's `std::fs`
and `walkdir` yield `OsStr` paths. The adapter converts them via
`file_name().to_string_lossy()`, which replaces non-UTF-8 byte sequences with
`\u{FFFD}` (the Unicode replacement character). This means:
- **Valid UTF-8 Unicode filenames** (e.g., `test_ünicöde.py`, `test_日本語.py`)
  work correctly on any modern Linux system with a UTF-8 locale.
- **Truly non-UTF-8 filenames** (containing arbitrary bytes that don't form
  valid UTF-8) will have those bytes replaced with `\u{FFFD}`, which could
  break pattern matching in `starts_with("test_")` and `ends_with("_test.py")`.
  This is extremely rare in practice.

**Expected behavior:** Test files with valid Unicode names should be discovered
and fingerprinted correctly. Source files with Unicode names should resolve to
valid module paths.

**Minimal reproduction:**
```
tests/
├── test_ünicöde.py    # ← works fine on UTF-8 Linux
├── test_日本語.py      # ← works fine on UTF-8 Linux
└── test_ascii.py      # ← always discovered
```

**Fixture needed?** Yes — synthetic fixture with Unicode filenames

**Severity:** minor
**Recommendation:** workaround — document that the adapter works on valid
UTF-8 filesystems. Non-UTF-8 byte sequences in filenames may cause issues.

---

### Edge 9: Symlinks to external test directories

**Scenario:** The `tests/` directory is a symlink to an external location
(e.g., `tests/ -> /shared/test-suite/`). Or individual test files are symlinks.

**Affected commands:** discover, static-deps, fingerprint

**Current behavior:** `WalkDir` uses `.follow_links(false)` by default — it
does **not** follow symlinks to directories. This means:
- **Directory symlinks** (e.g., `tests/ -> /shared/test-suite/`) are returned
  as directory entries but their contents are **not traversed**. Test files
  inside the linked directory are silently omitted from discovery.
- **File symlinks** (individual symlinked test files) are included as regular
  entries, so they are discovered and fingerprinted correctly.
- `file_path_to_module` resolves the symlink path as-is (not the real path),
  which may not match the module's actual import path if the link points
  outside the project tree.

**Expected behavior:** The adapter should either:
- Call `.follow_links(true)` to traverse directory symlinks and resolve
  paths to their real (canonical) locations, or
- Provide a configuration option to control symlink behavior

**Minimal reproduction:**
```
my_project/
├── tests/ -> /shared/test-suite/   # symlink → NOT traversed
│   └── test_shared.py              # ← silently missed
├── src/
│   └── model.py
└── test_real.py                    # ← discovered
```

**Fixture needed?** Yes — synthetic fixture with symlinks

**Severity:** major
**Recommendation:** fix adapter — add `follow_links(true)` and canonical path
resolution, or make symlink behavior configurable

---

### Edge 10: tox environments with multiple Python versions

**Scenario:** Project uses `tox` to test against multiple Python versions.
The `.tox/` directory contains virtual environments with vendored test file
copies.

**Affected commands:** discover

**Current behavior:** The adapter explicitly excludes `.tox` from the WalkDir
filter. This is correct — vendored tests in `.tox/` should not be discovered.

**Expected behavior:** Same as current.

**Minimal reproduction:**
```
project/
├── .tox/
│   ├── py39/
│   │   └── .../test_vendored.py    # ← excluded
│   └── py310/
│       └── .../test_vendored.py    # ← excluded
├── tests/
│   └── test_real.py                # ← discovered
```

**Fixture needed?** No — the exclusion is present in code but untested (only `.venv` exclusion has a dedicated test)

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 11: Editable installs (pip install -e .)

**Scenario:** When a package is installed in editable mode (`pip install -e .`),
the source code is linked directly into site-packages. The adapter walks the
project root, not site-packages, so this doesn't affect discovery directly.

**Affected commands:** discover, static-deps

**Current behavior:** The adapter discovers tests within the project root only.
Editable installs of *other* packages into the project's environment don't
affect discovery. However, if the project has test files that import from
editable-installed dependencies, the `static-deps` command will look for those
dependencies' source files in the project tree and won't find them.

**Expected behavior:** The adapter should either:
- Track dependencies from editable installs (via `.egg-link` or `.pth` files),
- Or report them as unresolved dependencies.

**Minimal reproduction:**
```
my_project/
├── src/my_project/
│   └── main.py
├── tests/
│   └── test_main.py       # import editable_pkg
└── editable-pkg.egg-link  # points to /other/path/editable_pkg
```

**Fixture needed?** Yes — synthetic fixture with `.egg-link` file

**Severity:** minor
**Recommendation:** workaround — document that editable installs outside the
project root are not tracked. The unresolved dependency list will include them.

---

## Static-deps

### Edge 12: Relative imports — all varieties

**Scenario:** Python relative imports in all their forms:
- `from .module import X` — sibling module import
- `from ..module import X` — parent package module import
- `from . import X` — current package import
- `from .. import X` — parent package import
- `from ...module import X` — grandparent + module

**Affected commands:** static-deps

**Current behavior:** The `parse_python_imports` function handles all these
varieties. It resolves relative imports by counting dots and trimming the
base package path accordingly. The implementation has specific handling for
`from . import X` (current package only) and `from .. import X` (parent
package only, no module). The `parse_python_imports_relative_*` tests cover
these cases.

**Expected behavior:** Same as current.

**Minimal reproduction:** Already covered by existing tests:
- `test_parse_python_imports_relative_current_package`
- `test_parse_python_imports_relative_parent_package`
- `test_parse_python_imports_relative_import_current_package_only`
- `test_parse_python_imports_relative_from_double_dot`
- `test_parse_python_imports_relative_deep`
- `test_parse_python_imports_relative_above_root_skipped`

**Fixture needed?** No — already tested

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 13: Conditional imports (try/except ImportError)

**Scenario:** Imports guarded by `try/except ImportError` blocks for optional
dependencies. For example, `try: import orjson; except ImportError: import json`.

**Affected commands:** static-deps

**Current behavior:** The adapter's `parse_python_imports` does a simple
line-based scan for `import ` and `from `. It finds these imports regardless
of surrounding control flow. This is correct — both branches' imports are
recorded as dependencies.

**Expected behavior:** Same as current.

**Minimal reproduction:**
```python
try:
    import orjson
except ImportError:
    import json
```
Both `orjson` and `json` are correctly found.

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 14: Dynamic imports (importlib.import_module, __import__)

**Scenario:** Imports performed at runtime via `importlib.import_module()` or
`__import__()` with dynamic module names constructed from strings or variables.

**Affected commands:** static-deps

**Current behavior:** The adapter only scans for `import X` and `from X import Y`
syntax patterns. Dynamic imports like `importlib.import_module("numpy")` are
not detected. The argument to `import_module` may be a string literal (which
could be parsed) or a variable (which can't be statically analyzed).

**Expected behavior:** The adapter should detect `importlib.import_module()`
calls with string literal arguments as additional dependencies. Variable-based
dynamic imports are inherently undetectable and should be documented as a
limitation.

**Minimal reproduction:**
```python
import importlib

# String literal — could be detected:
np = importlib.import_module("numpy")

# Variable — undetectable:
module_name = "pandas"
pd = importlib.import_module(module_name)
```

**Fixture needed?** Yes — synthetic fixture with `importlib.import_module`

**Severity:** blocker
**Recommendation:** fix adapter — add detection of `importlib.import_module()`
and `__import__()` calls with string literal arguments

---

### Edge 15: Re-exports (from x import y as z, from x import *)

**Scenario:** Modules that re-export symbols from other modules:
- `from x import y as z` — rename on import
- `from x import *` — wildcard import
- `from x import y` — used as if `y` is from the importing module

**Affected commands:** static-deps

**Current behavior:** The adapter correctly extracts the source module for
all three patterns:
- `from x import y as z` → dependency on `x` (the `as z` part is ignored)
- `from x import *` → dependency on `x`
- `from x import y` → dependency on `x`

The adapter correctly records the dependency on the *source* module, not the
importing module's namespace.

**Expected behavior:** Same as current.

**Minimal reproduction:**
```python
# package/__init__.py
from .submodule import useful_function as fn
from .another import *

# test_file.py
from package import fn   # ← dependency on package, which re-exports
```

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 16: Circular imports

**Scenario:** Two modules that import each other (directly or transitively).

**Affected commands:** static-deps

**Current behavior:** The adapter's `parse_python_imports` doesn't perform
recursive import resolution. It only records the first-level imports of each
test file. Circular imports at the first level would create bidirectional
edges, but the adapter doesn't recurse so it won't follow cycles.

**Expected behavior:** The adapter should handle circular imports gracefully
by not recursing into them (which it already doesn't). The static-deps result
is correct for first-level imports.

**Minimal reproduction:**
```python
# a.py
from b import helper

# b.py
from a import config
```

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 17: Imports in `__init__.py` (package re-exports)

**Scenario:** A package's `__init__.py` that imports and re-exports symbols
from submodules. Test files that import from the package (not the submodule)
need to be resolved to the actual submodule for dependency tracking.

**Affected commands:** static-deps

**Current behavior:** The adapter records the import as a dependency on the
package (e.g., `from mypkg import useful_func` → depends on `mypkg`).
However, it doesn't follow the re-export chain to find the actual submodule
(e.g., `mypkg.submodule`). This means:
- If `mypkg/__init__.py` re-exports from `mypkg/submodule.py`, and the user
  changes `mypkg/submodule.py`, the adapter won't find test files that import
  from `mypkg` (not `mypkg.submodule`).

This is a different concern from Edge 15, which covers the parser correctly
identifying the source module of a re-export. Edge 17 is about the dependency
chain not being followed through `__init__.py`.

**Expected behavior:** The adapter should optionally follow `__init__.py`
re-exports to build the full dependency graph. This is related to the
symbol-level modeling capability.

**Minimal reproduction:**
```
mypkg/
├── __init__.py              # from .submodule import useful_func
└── submodule.py             # changes here
tests/
└── test_features.py         # from mypkg import useful_func
                             # ← dependency on mypkg, not mypkg.submodule
                             #    so change to submodule.py won't be detected
```

**Fixture needed?** Yes — synthetic fixture with `__init__.py` re-exports

**Severity:** major
**Recommendation:** fix adapter — add `__init__.py` import resolution to follow
re-export chains, or document as a limitation linked to `symbol_model_complete`

---

### Edge 18: C extension modules with .so/.pyd files

**Scenario:** Projects that use compiled C extensions (`.so` on Linux, `.pyd`
on Windows) alongside or instead of pure Python modules.

**Affected commands:** static-deps, fingerprint

**Current behavior:** The adapter treats `.so` and `.pyd` files as opaque
binary files. For `static-deps`, it can't parse imports from them. For
`fingerprint`, it can hash them (blake3 works on any binary input).

Additionally, `file_path_to_module` produces a corrupted module name for
`.so`/`.pyd` files. For example:
`file_path_to_module("mypkg/_speedups.cpython-312-x86_64-linux-gnu.so")`
→ `strip_suffix(".py")` has no match (file ends with `.so`)
→ replaces `/` with `.` → `"mypkg._speedups.cpython-312-x86_64-linux-gnu.so"`
→ the `.so` suffix and platform ABI tag are preserved in the module name,
  which won't match any import statement.

**Expected behavior:** The adapter should:
- For `static-deps`: strip `.so`/`.pyd` **and** the platform ABI suffix
  (e.g., `.cpython-312-x86_64-linux-gnu`) from the module name derivation;
  skip import parsing on binary files; report them in the unresolved list
- For `fingerprint`: hash them as-is (already works)

**Minimal reproduction:**
```
mypkg/
├── __init__.py
├── _speedups.cpython-312-x86_64-linux-gnu.so   # compiled extension
└── pure.py
tests/
└── test_mypkg.py          # import mypkg._speedups  # ← can't resolve
```

**Fixture needed?** Yes — synthetic fixture with a `.so` file (or simulant)

**Severity:** blocker
**Recommendation:** fix adapter — strip `.so`/`.pyd` and ABI tag suffix from
module name derivation; gracefully handle binary extension modules (skip
import parsing, include in unresolved list)

---

### Edge 19: Type-checking-only imports (TYPE_CHECKING)

**Scenario:** Imports placed inside `if TYPE_CHECKING:` blocks to avoid runtime
imports. These are used for type hints only.

**Affected commands:** static-deps

**Current behavior:** The adapter's line-based parser doesn't distinguish
between imports inside `if TYPE_CHECKING:` blocks and regular imports. This
produces false positive dependency edges.

**Expected behavior:** The adapter should either:
- Skip imports inside `TYPE_CHECKING` blocks (correct behavior), or
- Include them but mark them with a lower weight (acceptable compromise)

**Minimal reproduction:**
```python
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from expensive_module import HeavyClass  # ← false positive dependency

def get_heavy() -> "HeavyClass":
    ...
```

**Fixture needed?** Yes — synthetic fixture with `TYPE_CHECKING` guard

**Severity:** minor
**Recommendation:** fix adapter — add `TYPE_CHECKING` block detection in
`parse_python_imports`

---

### Edge 20: Lazy imports inside function/class bodies

**Scenario:** Imports placed inside function or class bodies to defer import
cost until first use.

**Affected commands:** static-deps

**Current behavior:** The adapter scans all lines of the file regardless of
indentation or nesting. Lazy imports inside function bodies are correctly
detected.

**Expected behavior:** Same as current.

**Minimal reproduction:**
```python
def get_model():
    from myapp.models import HeavyModel  # ← correctly detected
    return HeavyModel()
```

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 21: sys.path manipulation

**Scenario:** Projects that modify `sys.path` at runtime to add custom import
paths. This can happen in:
- `conftest.py` (inserting project root into `sys.path`)
- `__init__.py` (adding sibling directories)
- `setup.py` / `setup.cfg` configurations

**Affected commands:** static-deps

**Current behavior:** The adapter doesn't read or track `sys.path` modifications.
It resolves module paths purely from file paths. If a test file imports from
a module that's only accessible via a modified `sys.path`, the adapter won't
find the import resolution.

**Expected behavior:** The adapter should detect `sys.path` modifications in
`conftest.py` and `__init__.py` files and adjust its module resolution
accordingly. This is a complex feature — a simpler alternative is to document
the limitation.

**Minimal reproduction:**
```python
# conftest.py
import sys
sys.path.insert(0, "/path/to/other/project")

# tests/test_integration.py
from other_project import Something  # ← sys.path makes this work
                                       #    but adapter can't resolve it
```

**Fixture needed?** Yes — synthetic fixture with `sys.path` manipulation

**Severity:** major
**Recommendation:** workaround — document that `sys.path` manipulation is not
tracked. Affected imports will appear in the `unresolved` list.

---

### Edge 22: egg-link / pth file dependencies

**Scenario:** Python packages installed via `pip install -e` create `.egg-link`
files, and `.pth` files can extend `sys.path`. These are typically in
site-packages, not the project root.

**Affected commands:** static-deps

**Current behavior:** The adapter walks only the project root directory. It
doesn't read `.egg-link` or `.pth` files. Dependencies that point to external
packages (installed via `-e` flag) will be unresolved.

**Expected behavior:** The adapter should either:
- Read `.egg-link` files and add the linked paths to the search scope, or
- Report linked imports as unresolved with a clear message

**Minimal reproduction:**
```
# .eggs/some-app.egg-link  (typically in site-packages, not project root)
/path/to/some-app

# tests/test_some_app.py
from some_app import helper  # ← unresolved because adapter can't find some_app
```

**Fixture needed?** No — this is a site-packages concern, not a project-level
fixture

**Severity:** minor
**Recommendation:** workaround — document that editable installs outside the
project root produce unresolved dependencies

---

## Fingerprint

### Edge 23: Very large files

**Scenario:** Test or source files that are very large (e.g., generated test
data, long integration tests, fixtures).

**Affected commands:** fingerprint

**Current behavior:** The adapter reads the entire file into memory with
`std::fs::read(file)`, then hashes it with blake3. For very large files
(>100MB), this could cause:
- High memory usage
- Slow fingerprinting
- Potential OOM on memory-constrained systems

**Expected behavior:** The adapter should stream the file through blake3 in
chunks to bound memory usage.

**Minimal reproduction:** Create a 1GB Python file and fingerprint it:
```python
# tests/test_huge.py
# ... 1GB of generated test data
```

**Fixture needed?** No — use an existing large file or generate one

**Severity:** minor
**Recommendation:** fix adapter — stream file content through blake3 in chunks
(e.g., 64KB buffer)

---

### Edge 24: Binary files checked into repo

**Scenario:** Repositories that contain binary files alongside Python code
(e.g., `.png`, `.jpg`, `.zip`, `.pickle` test fixtures).

**Affected commands:** fingerprint

**Current behavior:** The adapter uses `blake3::hash` on raw bytes, which works
for any file type. Binary files are hashed correctly.

**Expected behavior:** Same as current.

**Minimal reproduction:**
```
tests/
├── test_with_fixture.py
└── fixtures/
    └── test_data.pkl   # ← hashed correctly
```

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 25: Empty files

**Scenario:** Test files or source files that are empty (zero bytes).

**Affected commands:** fingerprint

**Current behavior:** The adapter reads zero bytes and computes the blake3 hash
of an empty input. This is deterministic and correct.

**Expected behavior:** Same as current.

**Minimal reproduction:**
```
tests/
└── test_empty.py   # empty file → blake3("") → deterministic hash
```

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 26: Files with only comments

**Scenario:** Files that contain only comments and no executable code.

**Affected commands:** fingerprint

**Current behavior:** The adapter hashes the file content as-is. Comments are
content, so the hash reflects the full file.

**Expected behavior:** Same as current. Changing a comment changes the hash,
which is correct (the file changed, even if it's only a comment).

**Minimal reproduction:**
```python
# tests/test_placeholder.py
# This test is a placeholder
# TODO: write actual tests
```

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 27: Generated files (protobuf, grpc)

**Scenario:** Files that are generated by code generators (protobuf, gRPC,
OpenAPI generators) and checked into the repository.

**Affected commands:** fingerprint

**Current behavior:** The adapter hashes all files without distinguishing
between hand-written and generated code. Generated files will be fingerprinted
and their hashes will change when regenerated, causing unnecessary re-runs.

**Expected behavior:** The adapter should either:
- Allow users to exclude generated files from fingerprinting, or
- Detect generated file headers (e.g., `@generated` marker) and handle them
  differently

**Minimal reproduction:**
```
project/
├── proto/
│   └── model_pb2.py    # generated by protoc — changes on every proto change
├── src/
│   └── service.py      # hand-written
└── tests/
    └── test_service.py
```

**Fixture needed?** Yes — synthetic fixture with a generated file marker

**Severity:** minor
**Recommendation:** workaround — document that generated files can be excluded
via the project's `.gitignore` or by adding them to the adapter's exclude list

---

## Run-args

### Edge 28: Custom pytest configurations (pytest.ini, pyproject.toml)

**Scenario:** Projects that configure pytest through `pytest.ini`,
`pyproject.toml` (under `[tool.pytest.ini_options]`), `setup.cfg`, or
`tox.ini`.

**Affected commands:** run-args

**Current behavior:** The adapter generates `run-args` as:
`pytest <selected_files> -v --junitxml=target/test-results.xml`

Pytest automatically discovers and applies configuration files in the project
root. The adapter doesn't need to read or pass them explicitly.

**Expected behavior:** Same as current.

**Minimal reproduction:**
```ini
# pytest.ini
[pytest]
addopts = -ra --strict-markers
testpaths = tests
```
Running `pytest tests/test_foo.py -v --junitxml=target/test-results.xml`
will automatically pick up `addopts = -ra --strict-markers` from `pytest.ini`.

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

### Edge 29: Parallel test runners (xdist)

**Scenario:** Projects that use `pytest-xdist` for parallel test execution
(e.g., `pytest -n auto`).

**Affected commands:** run-args

**Current behavior:** The adapter's `run-args` doesn't include any `-n` flag.
Tests run sequentially by default, which is slower but correct.

**Expected behavior:** The adapter should either:
- Add `-n auto` to run-args when xdist is available, or
- Make parallel execution configurable

**Minimal reproduction:**
```
# In pyproject.toml or requirements
# pytest-xdist installed

# Current adapter run-args:
# pytest tests/test_foo.py tests/test_bar.py -v --junitxml=target/test-results.xml

# Expected (with xdist):
# pytest tests/test_foo.py tests/test_bar.py -v -n auto --junitxml=target/test-results.xml
```

**Fixture needed?** No

**Severity:** major
**Recommendation:** fix adapter — check for xdist availability and add `-n auto`
to run-args when available, or make the parallel flag configurable

---

### Edge 30: Test markers and parametrization

**Scenario:** Tests that use pytest markers (`@pytest.mark.slow`) or
parametrization (`@pytest.mark.parametrize`).

**Affected commands:** run-args

**Current behavior:** The adapter passes selected test file paths to pytest.
Pytest handles marker filtering and parametrization internally. The adapter
doesn't need to understand markers or parametrization.

**Expected behavior:** Same as current.

**Minimal reproduction:**
```python
import pytest

@pytest.mark.slow
@pytest.mark.parametrize("n", range(100))
def test_compute(n):
    assert n ** 2 >= 0
```
Running `pytest tests/test_compute.py -v --junitxml=target/test-results.xml`
will run all 100 parametrized test cases.

**Fixture needed?** No

**Severity:** cosmetic
**Recommendation:** won't fix

---

## Summary

| # | Edge Case | Severity | Recommendation | Fixture Needed |
|---|-----------|----------|----------------|---------------|
| 1 | src/ layout | cosmetic | won't fix | No |
| 2 | Tests in separate dir vs co-located | cosmetic | won't fix | No |
| 3 | Namespace packages (PEP 420) | minor | fix adapter | Yes |
| 4 | `__init__.py` discovery implications | cosmetic | won't fix | No |
| 5 | conftest.py at multiple levels | cosmetic | won't fix | No |
| 6 | Non-standard test file naming | **major** | fix adapter | Yes |
| 7 | Deeply nested packages | cosmetic | won't fix | No |
| 8 | Unicode filenames | minor | workaround | Yes |
| 9 | Symlinks to external directories | **major** | fix adapter | Yes |
| 10 | tox environments | cosmetic | won't fix | No |
| 11 | Editable installs (pip install -e) | minor | workaround | Yes |
| 12 | Relative imports — all varieties | cosmetic | won't fix | No |
| 13 | Conditional imports | cosmetic | won't fix | No |
| 14 | Dynamic imports (importlib) | **blocker** | fix adapter | Yes |
| 15 | Re-exports (from x import y as z) | cosmetic | won't fix | No |
| 16 | Circular imports | cosmetic | won't fix | No |
| 17 | Imports in `__init__.py` (re-exports) | **major** | fix adapter | Yes |
| 18 | C extension modules (.so/.pyd) | **blocker** | fix adapter | Yes |
| 19 | TYPE_CHECKING imports | minor | fix adapter | Yes |
| 20 | Lazy imports inside functions | cosmetic | won't fix | No |
| 21 | sys.path manipulation | **major** | workaround | Yes |
| 22 | egg-link / pth file deps | minor | workaround | No |
| 23 | Very large files | minor | fix adapter | No |
| 24 | Binary files in repo | cosmetic | won't fix | No |
| 25 | Empty files | cosmetic | won't fix | No |
| 26 | Files with only comments | cosmetic | won't fix | No |
| 27 | Generated files (protobuf, grpc) | minor | workaround | Yes |
| 28 | Custom pytest configs | cosmetic | won't fix | No |
| 29 | Parallel test runners (xdist) | **major** | fix adapter | No |
| 30 | Test markers and parametrization | cosmetic | won't fix | No |

### Priority breakdown

| Severity | Count | Tickets |
|----------|-------|---------|
| blocker | 2 | 14 (dynamic imports), 18 (C extensions) |
| major | 5 | 6 (non-standard naming), 9 (symlinks), 17 (__init__.py re-exports), 21 (sys.path), 29 (xdist) |
| minor | 7 | 3 (namespace packages), 8 (Unicode filenames), 11 (editable installs), 19 (TYPE_CHECKING), 22 (egg-link / pth), 23 (large files), 27 (generated files) |
| cosmetic | 16 | rest |

### Test strategy

**Can be tested with real-world codebases:**
- src/ layout (Edge 1)
- Non-standard test file naming (Edge 6) — `testSmoke.py` pattern
- Deeply nested packages (Edge 7)
- Imports in `__init__.py` (Edge 17)
- Lazy imports (Edge 20)
- Custom pytest configs (Edge 28)
- Test markers and parametrization (Edge 30)

**Need synthetic fixtures:**
- Namespace packages (Edge 3)
- Non-standard naming variants (Edge 6) — `check_*` prefix and `_spec.py` suffix
- Unicode filenames (Edge 8)
- Symlinks (Edge 9)
- Editable installs (Edge 11)
- Dynamic imports (Edge 14)
- C extension modules (Edge 18)
- TYPE_CHECKING (Edge 19)
- sys.path manipulation (Edge 21)
- Generated files (Edge 27)
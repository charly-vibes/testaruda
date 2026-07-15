//! testaruda-adapter-python — Reference adapter for Python projects.
//!
//! Reads JSON commands from stdin, responds on stdout.
//! Protocol: single JSON line → single JSON line response.

use std::io::{BufRead, BufReader, Write};

fn main() {
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        let response = handle_command(&trimmed);
        let out = serde_json::to_string(&response)
            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization failed"}"#.to_string());
        println!("{}", out);
        std::io::stdout().flush().ok();
    }
}

fn handle_command(input: &str) -> serde_json::Value {
    let cmd: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => return json_err(&format!("invalid JSON: {}", e)),
    };

    let command = cmd["command"].as_str().unwrap_or("");
    match command {
        "handshake" => cmd_handshake(),
        "discover" => cmd_discover(),
        "static-deps" => cmd_static_deps(&cmd),
        "fingerprint" => cmd_fingerprint(&cmd),
        "run-args" => cmd_run_args(&cmd),
        "ingest" => cmd_ingest(&cmd),
        _ => json_err(&format!("unknown command: {}", command)),
    }
}

fn json_ok(result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"ok": true, "result": result})
}

fn json_err(msg: &str) -> serde_json::Value {
    serde_json::json!({"ok": false, "error": msg})
}

/// Handshake: declare capabilities (TIA-ADAPT-002).
fn cmd_handshake() -> serde_json::Value {
    json_ok(serde_json::json!({
        "name": "python-adapter",
        "version": "0.1.0",
        "protocol": 1,
        "languages": ["python"],
        "granularity": "file",
        "capabilities": {
            "symbol_model_complete": false,
            "fingerprinting": true,
            "runtime_edges": false
        }
    }))
}

/// Discover: find test_*.py and *_test.py files (TIA-ADAPT-004).
/// Excludes common virtual environment and cache directories.
fn cmd_discover() -> serde_json::Value {
    let mut tests = Vec::new();

    let excluded_dirs = [
        ".venv",
        "venv",
        "__pycache__",
        ".mypy_cache",
        ".pytest_cache",
        "build",
        "dist",
        ".git",
        "target",
        "node_modules",
        ".tox",
    ];

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !excluded_dirs.contains(&name.as_ref())
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_string_lossy().to_string();
        // Strip ./ prefix for clean node_id
        let clean_path = path.strip_prefix("./").unwrap_or(&path);
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname.starts_with("test_") && fname.ends_with(".py") || fname.ends_with("_test.py") {
            tests.push(serde_json::json!({
                "node_id": clean_path,
                "suite_kind": "unit",
                "file": clean_path,
            }));
        }
    }

    json_ok(serde_json::Value::Array(tests))
}

/// Static-deps: find test files that import from the changed source files (TIA-ADAPT-005).
///
/// For each changed file, derives its Python module name from the file path,
/// then searches all test files for imports of that module. Creates edges
/// from importing test files to the changed source file.
fn cmd_static_deps(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let changed_files: Vec<String> = match params["changed_files"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        None => return json_err("missing 'params.changed_files'"),
    };

    let mut unresolved = Vec::new();
    let mut edges = Vec::new();

    // Discover all test files
    let discover_all = cmd_discover();
    let test_files: Vec<(String, String)> = if let Some(results) = discover_all["result"].as_array()
    {
        results
            .iter()
            .filter_map(|t| {
                let node_id = t["node_id"].as_str()?;
                let file = t["file"].as_str()?;
                Some((node_id.to_string(), file.to_string()))
            })
            .collect()
    } else {
        Vec::new()
    };

    // Build a map: imported module name → list of (test_node_id, test_file_path)
    // For each test file, parse its imports to find which source files it depends on.
    let mut import_to_tests: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();
    for (node_id, file) in &test_files {
        if let Ok(content) = std::fs::read_to_string(file) {
            let imports = parse_python_imports(&content, file);
            for imp in imports {
                import_to_tests
                    .entry(imp)
                    .or_default()
                    .push((node_id.clone(), file.clone()));
            }
        }
    }

    // For each changed file, derive its module name and look up test files
    // that import from it.
    for file in &changed_files {
        // Derive module name from the file path
        // e.g., "src/cositos/model.py" → "src.cositos.model"
        let module_name = file_path_to_module(file);

        // Check if any test file imports this module
        // Try exact match first, then try matching just the file stem
        let matching_tests = import_to_tests.get(&module_name).cloned().or_else(|| {
            // Fallback: match by just the file stem (without package path)
            // to handle imports like 'from model import X' where 'model'
            // is imported directly without a package prefix.
            // This is a heuristic; the full solution (complete import
            // graph at discover time) is tracked in testaruda-16f.
            let stem = module_name
                .rsplit('.')
                .next()
                .unwrap_or(&module_name)
                .to_string();
            import_to_tests.get(&stem).cloned()
        });

        if let Some(tests) = matching_tests {
            for (test_node_id, _test_file) in &tests {
                edges.push(serde_json::json!({
                    "from": test_node_id,
                    "to": file,
                    "weight": 1_000_000,
                    "origin": "static",
                }));
            }
        } else {
            // If the changed file is itself a test file, create self-referential
            // edges (test→source = itself) so the file is included in selection
            if file.ends_with("_test.py") || file.contains("/test_") || file.starts_with("test_") {
                if let Some(test_tuples) = test_files.iter().find(|(_, f)| f == file) {
                    let (test_node_id, _) = test_tuples;
                    edges.push(serde_json::json!({
                        "from": test_node_id,
                        "to": file,
                        "weight": 1_000_000,
                        "origin": "static",
                    }));
                }
            }

            // File could not be matched to any test — add to unresolved
            if std::fs::read_to_string(file).is_err() {
                unresolved.push(file.clone());
            }
        }
    }

    // Collect all discovered test node IDs as candidates
    let candidates: Vec<String> = test_files
        .iter()
        .map(|(node_id, _)| node_id.clone())
        .collect();

    serde_json::json!({
        "ok": true,
        "candidates": candidates,
        "edges": edges,
        "unresolved": unresolved,
        "symbol_edges": [],
    })
}

/// Parse `import` and `from ... import` statements from Python source.
/// Returns full module paths (e.g., `from src.model import X` → `"src.model"`).
/// Relative imports (`from .module import X`) are resolved to absolute module
/// paths using the importing file's location.
fn parse_python_imports(content: &str, file_path: &str) -> Vec<String> {
    let mut deps = Vec::new();

    // Derive the base package from the file path.
    // e.g., "src/package/test_module.py" → base package is "src.package"
    let base_module = file_path_to_module(file_path);
    let base_package: Vec<&str> = base_module
        .rsplit('.')
        .skip(1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            let module = trimmed
                .strip_prefix("import ")
                .map(|s| s.split(" as ").next().unwrap_or(s).trim().to_string());
            if let Some(m) = module {
                deps.push(m);
            }
        } else if trimmed.starts_with("from ") {
            let rest = trimmed.strip_prefix("from ").unwrap_or("");

            // Check for relative imports (starts with one or more dots)
            if rest.starts_with('.') {
                let dot_count = rest.chars().take_while(|c| *c == '.').count();
                let after_dots = rest[dot_count..].trim();

                // Split the remaining part: "module_path import X" or just "import X"
                let after_dots = after_dots.trim();
                let module_part = if after_dots.starts_with("import") {
                    // from . import X — no module path after the dots
                    ""
                } else {
                    after_dots
                        .split(" import ")
                        .next()
                        .map(|s| s.trim())
                        .unwrap_or("")
                };

                // Resolve: go up (dot_count - 1) levels from the base package
                let mut parts = base_package.clone();
                if dot_count > 0 {
                    let levels_up = dot_count.saturating_sub(1);
                    let len = parts.len();
                    if levels_up < len {
                        parts.truncate(len - levels_up);
                    } else {
                        // Can't resolve above the project root — skip
                        continue;
                    }
                }

                if !module_part.is_empty() {
                    parts.push(module_part);
                }

                let resolved = parts.join(".");
                if !resolved.is_empty() {
                    deps.push(resolved);
                }
            } else {
                // Absolute import: from foo.bar import baz
                let module = rest.split(" import ").next().map(|s| s.trim().to_string());
                if let Some(m) = module {
                    deps.push(m);
                }
            }
        }
    }
    deps
}

/// Convert a Python file path to its module name.
/// E.g., `"src/cositos/model.py"` → `"src.cositos.model"`
fn file_path_to_module(path: &str) -> String {
    path.strip_suffix(".py")
        .unwrap_or(path)
        .replace('/', ".")
        .trim_start_matches('.')
        .to_string()
}

/// Fingerprint: compute blake3 hashes for files (TIA-ADAPT-006).
fn cmd_fingerprint(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let files: Vec<String> = match params["files"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        None => return json_err("missing 'params.files'"),
    };

    let mut fingerprints = Vec::new();
    for file in &files {
        let content = match std::fs::read(file) {
            Ok(c) => c,
            Err(e) => return json_err(&format!("cannot read {}: {}", file, e)),
        };
        let hash = blake3::hash(&content);
        fingerprints.push(serde_json::json!({
            "file": file,
            "fingerprint": hash.to_hex().to_string(),
            "symbol": null,
        }));
    }

    serde_json::json!({"ok": true, "fingerprints": fingerprints})
}

/// Run-args: return native runner arguments for the selected test set (TIA-ADAPT-007).
/// Does NOT execute the tests.
fn cmd_run_args(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let selected: Vec<String> = match params["selected"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        None => return json_err("missing 'params.selected'"),
    };

    if selected.is_empty() {
        return json_err("no tests selected");
    }

    // Build pytest args: pass selected test file paths directly
    let runner_args: Vec<String> = std::iter::once("pytest".to_string())
        .chain(selected.iter().cloned())
        .chain(std::iter::once("-v".to_string()))
        .chain(std::iter::once(
            "--junitxml=target/test-results.xml".to_string(),
        ))
        .collect();

    let collection_path = "target/test-results.xml".to_string();

    json_ok(serde_json::json!({
        "runner_args": runner_args,
        "collection_path": collection_path,
    }))
}

/// Ingest: parse pytest output and return runtime edges and results (TIA-ADAPT-008).
fn cmd_ingest(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let run_output = match params["run_output"].as_str() {
        Some(s) => s,
        None => return json_err("missing 'params.run_output'"),
    };

    if run_output.is_empty() {
        return json_err("empty run output");
    }

    let mut per_test_results: Vec<serde_json::Value> = Vec::new();

    // Parse pytest output for test results
    // Lines look like: "test_file.py::test_name PASSED" or "FAILED"
    for line in run_output.lines() {
        let trimmed = line.trim();

        if trimmed.ends_with(" PASSED") {
            let test_id = trimmed
                .strip_suffix(" PASSED")
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if !test_id.is_empty() {
                per_test_results.push(serde_json::json!({
                    "test_id": test_id,
                    "outcome": "passed",
                }));
            }
        } else if trimmed.ends_with(" FAILED") {
            let test_id = trimmed
                .strip_suffix(" FAILED")
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if !test_id.is_empty() {
                per_test_results.push(serde_json::json!({
                    "test_id": test_id,
                    "outcome": "failed",
                }));
            }
        }
    }

    // If no standard test output found, try to parse as JSON-line format
    if per_test_results.is_empty() {
        for line in run_output.lines() {
            let trimmed = line.trim();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if val.get("test_id").is_some() && val.get("outcome").is_some() {
                    per_test_results.push(val);
                }
            }
        }
    }

    // Build runtime edges: for each test result, create a self-edge from the
    // test_id to its file path (e.g., "tests/test_model.py::test_something" →
    // "tests/test_model.py"). The file path is the part before "::".
    let mut runtime_edges: Vec<serde_json::Value> = Vec::new();
    for test in &per_test_results {
        if let Some(test_id) = test["test_id"].as_str() {
            if let Some(file_path) = test_id.split("::").next() {
                if !file_path.is_empty() {
                    runtime_edges.push(serde_json::json!({
                        "from": test_id,
                        "to": file_path,
                        "weight": 1_000_000,
                        "origin": "runtime",
                    }));
                }
            }
        }
    }

    json_ok(serde_json::json!({
        "runtime_edges": runtime_edges,
        "per_test_results": per_test_results,
        "external_inputs": [],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_python_imports_absolute() {
        let content = "import os\nimport sys\nfrom collections import OrderedDict\n";
        let imports = parse_python_imports(content, "test.py");
        assert!(imports.contains(&"os".to_string()));
        assert!(imports.contains(&"sys".to_string()));
        assert!(imports.contains(&"collections".to_string()));
    }

    #[test]
    fn test_parse_python_imports_preserves_full_path() {
        let content = "from src.model import Something\nfrom cositos.protocol import X\n";
        let imports = parse_python_imports(content, "test.py");
        assert!(imports.contains(&"src.model".to_string()));
        assert!(imports.contains(&"cositos.protocol".to_string()));
    }

    #[test]
    fn test_parse_python_imports_import_as() {
        let content = "import numpy as np\nimport pandas as pd\n";
        let imports = parse_python_imports(content, "test.py");
        assert!(imports.contains(&"numpy".to_string()));
        assert!(imports.contains(&"pandas".to_string()));
    }

    #[test]
    fn test_parse_python_imports_relative_current_package() {
        // from .module import X — resolves to same package as the importing file
        let content = "from .sibling import Something\n";
        let imports = parse_python_imports(content, "src/package/test_module.py");
        assert!(
            imports.contains(&"src.package.sibling".to_string()),
            "expected src.package.sibling, got: {:?}",
            imports
        );
    }

    #[test]
    fn test_parse_python_imports_relative_parent_package() {
        // from ..module import X — resolves to parent package
        let content = "from ..other import Something\n";
        let imports = parse_python_imports(content, "src/package/sub/test_module.py");
        assert!(
            imports.contains(&"src.package.other".to_string()),
            "expected src.package.other, got: {:?}",
            imports
        );
    }

    #[test]
    fn test_parse_python_imports_relative_import_current_package_only() {
        // from . import X — imports from the current package itself
        let content = "from . import something\n";
        let imports = parse_python_imports(content, "src/package/test_module.py");
        assert!(
            imports.contains(&"src.package".to_string()),
            "expected src.package, got: {:?}",
            imports
        );
    }

    #[test]
    fn test_parse_python_imports_relative_from_double_dot() {
        // from .. import X — imports from the parent package with no explicit module
        let content = "from .. import something\n";
        let imports = parse_python_imports(content, "src/package/sub/test_module.py");
        assert!(
            imports.contains(&"src.package".to_string()),
            "expected src.package, got: {:?}",
            imports
        );
    }

    #[test]
    fn test_parse_python_imports_relative_deep() {
        // from ...module import X — three dots: go up 2 levels
        // File at src/a/b/c/test_module.py, package is src.a.b.c
        // from ...top → go up 2 levels to src.a, then add top → src.a.top
        let content = "from ...top import Something\n";
        let imports = parse_python_imports(content, "src/a/b/c/test_module.py");
        assert!(
            imports.contains(&"src.a.top".to_string()),
            "expected src.a.top, got: {:?}",
            imports
        );
    }

    #[test]
    fn test_parse_python_imports_relative_above_root_skipped() {
        // from .. import X when file is at the top level — should be skipped
        let content = "from .. import something\n";
        let imports = parse_python_imports(content, "test.py");
        assert!(
            !imports.contains(&"".to_string()),
            "should not add empty or invalid resolved paths"
        );
    }

    #[test]
    fn test_parse_python_imports_mixed_absolute_and_relative() {
        let content = "import os\nfrom .model import Model\nfrom ..utils import helper\n";
        let imports = parse_python_imports(content, "src/package/test_module.py");
        assert!(imports.contains(&"os".to_string()));
        assert!(imports.contains(&"src.package.model".to_string()));
        assert!(imports.contains(&"src.utils".to_string()));
    }

    #[test]
    fn test_file_path_to_module() {
        assert_eq!(file_path_to_module("src/model.py"), "src.model");
        assert_eq!(
            file_path_to_module("src/cositos/model.py"),
            "src.cositos.model"
        );
        assert_eq!(file_path_to_module("model.py"), "model");
        assert_eq!(
            file_path_to_module("tests/test_model.py"),
            "tests.test_model"
        );
    }

    #[test]
    fn test_cmd_static_deps_source_changed_finds_importing_tests() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let tests_dir = dir.path().join("tests");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&tests_dir).unwrap();

        std::fs::write(src_dir.join("model.py"), "class Model:\n    pass\n").unwrap();
        std::fs::write(
            tests_dir.join("test_model.py"),
            "from src.model import Model\n\ndef test_model():\n    assert Model()\n",
        )
        .unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let cmd = serde_json::json!({
            "command": "static-deps",
            "params": {
                "changed_files": ["src/model.py"]
            }
        });
        let result = cmd_static_deps(&cmd);

        std::env::set_current_dir(&orig_dir).unwrap();

        assert!(result["ok"].as_bool().unwrap());
        let edges = result["edges"].as_array().unwrap();
        assert!(!edges.is_empty(), "should have edges from test to source");

        let edge = &edges[0];
        assert_eq!(edge["from"].as_str().unwrap(), "tests/test_model.py");
        assert_eq!(edge["to"].as_str().unwrap(), "src/model.py");
        assert_eq!(edge["origin"].as_str().unwrap(), "static");
    }

    #[test]
    fn test_cmd_static_deps_test_file_changed_self_edge() {
        let dir = tempfile::tempdir().unwrap();
        let tests_dir = dir.path().join("tests");
        std::fs::create_dir_all(&tests_dir).unwrap();

        std::fs::write(
            tests_dir.join("test_model.py"),
            "def test_model():\n    pass\n",
        )
        .unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let cmd = serde_json::json!({
            "command": "static-deps",
            "params": {
                "changed_files": ["tests/test_model.py"]
            }
        });
        let result = cmd_static_deps(&cmd);

        std::env::set_current_dir(&orig_dir).unwrap();

        assert!(result["ok"].as_bool().unwrap());
        let edges = result["edges"].as_array().unwrap();
        assert!(!edges.is_empty(), "changed test file should get self-edge");
        let edge = &edges[0];
        assert_eq!(edge["from"].as_str().unwrap(), "tests/test_model.py");
        assert_eq!(edge["to"].as_str().unwrap(), "tests/test_model.py");
    }

    #[test]
    fn test_cmd_static_deps_no_matching_tests() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        std::fs::write(src_dir.join("util.py"), "def helper(): pass\n").unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let cmd = serde_json::json!({
            "command": "static-deps",
            "params": {
                "changed_files": ["src/util.py"]
            }
        });
        let result = cmd_static_deps(&cmd);

        std::env::set_current_dir(&orig_dir).unwrap();

        assert!(result["ok"].as_bool().unwrap());
        let edges = result["edges"].as_array().unwrap();
        assert!(edges.is_empty(), "no test imports src/util.py");
    }

    #[test]
    fn test_cmd_static_deps_unresolved_file() {
        let dir = tempfile::tempdir().unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let cmd = serde_json::json!({
            "command": "static-deps",
            "params": {
                "changed_files": ["nonexistent.py"]
            }
        });
        let result = cmd_static_deps(&cmd);

        std::env::set_current_dir(&orig_dir).unwrap();

        assert!(result["ok"].as_bool().unwrap());
        let unresolved = result["unresolved"].as_array().unwrap();
        assert!(!unresolved.is_empty());
        assert!(unresolved
            .iter()
            .any(|v| v.as_str() == Some("nonexistent.py")));
    }

    #[test]
    fn test_cmd_static_deps_multiple_source_files() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        let tests_dir = dir.path().join("tests");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::create_dir_all(&tests_dir).unwrap();

        std::fs::write(src_dir.join("model.py"), "class Model: pass\n").unwrap();
        std::fs::write(src_dir.join("view.py"), "class View: pass\n").unwrap();
        std::fs::write(
            tests_dir.join("test_model.py"),
            "from src.model import Model\n\ndef test_model(): pass\n",
        )
        .unwrap();
        std::fs::write(
            tests_dir.join("test_view.py"),
            "from src.view import View\n\ndef test_view(): pass\n",
        )
        .unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let cmd = serde_json::json!({
            "command": "static-deps",
            "params": {
                "changed_files": ["src/model.py", "src/view.py"]
            }
        });
        let result = cmd_static_deps(&cmd);

        std::env::set_current_dir(&orig_dir).unwrap();

        assert!(result["ok"].as_bool().unwrap());
        let edges = result["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 2);

        let froms: Vec<&str> = edges.iter().map(|e| e["from"].as_str().unwrap()).collect();
        let tos: Vec<&str> = edges.iter().map(|e| e["to"].as_str().unwrap()).collect();
        assert!(froms.contains(&"tests/test_model.py"));
        assert!(froms.contains(&"tests/test_view.py"));
        assert!(tos.contains(&"src/model.py"));
        assert!(tos.contains(&"src/view.py"));
    }

    #[test]
    fn test_cmd_discover_excludes_venv() {
        let dir = tempfile::tempdir().unwrap();
        let venv_dir = dir
            .path()
            .join(".venv")
            .join("lib")
            .join("python3.12")
            .join("site-packages");
        std::fs::create_dir_all(&venv_dir).unwrap();

        // Write a vendored test file inside .venv
        std::fs::write(
            venv_dir.join("test_vendored.py"),
            "def test_vendored(): pass\n",
        )
        .unwrap();

        // Write a real project test file
        let tests_dir = dir.path().join("tests");
        std::fs::create_dir_all(&tests_dir).unwrap();
        std::fs::write(tests_dir.join("test_real.py"), "def test_real(): pass\n").unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let result = cmd_discover();

        std::env::set_current_dir(&orig_dir).unwrap();

        assert!(result["ok"].as_bool().unwrap());
        let items = result["result"].as_array().unwrap();
        let node_ids: Vec<&str> = items.iter().filter_map(|t| t["node_id"].as_str()).collect();

        assert!(
            node_ids.contains(&"tests/test_real.py"),
            "project test should be discovered"
        );
        assert!(
            !node_ids.iter().any(|id| id.contains(".venv")),
            "vendored tests in .venv should be excluded, got: {:?}",
            node_ids
        );
        assert_eq!(node_ids.len(), 1, "only one test should be discovered");
    }

    // ===== cmd_ingest tests =====

    #[test]
    fn test_cmd_ingest_pytest_output_returns_runtime_edges() {
        let cmd = serde_json::json!({
            "command": "ingest",
            "params": {
                "run_output": "tests/test_model.py::test_something PASSED\ntests/test_model.py::test_other FAILED\nsrc/test_util.py::test_helper PASSED\n"
            }
        });
        let result = cmd_ingest(&cmd);
        assert!(result["ok"].as_bool().unwrap());

        let per_test = result["result"]["per_test_results"].as_array().unwrap();
        assert_eq!(per_test.len(), 3);

        let runtime_edges = result["result"]["runtime_edges"].as_array().unwrap();
        assert!(!runtime_edges.is_empty(), "should have runtime edges");

        // Each test should have a self-edge (test_id → its file path)
        for edge in runtime_edges {
            assert_eq!(edge["origin"].as_str().unwrap(), "runtime");
            let from = edge["from"].as_str().unwrap();
            let to = edge["to"].as_str().unwrap();
            // The 'to' path should be the file part of the test_id
            assert!(to.contains('/'), "to should be a file path, got: {}", to);
            assert!(
                from.contains("::"),
                "from should be a test_id with ::, got: {}",
                from
            );
        }
    }

    #[test]
    fn test_cmd_ingest_runtime_edges_self_edge_per_test() {
        let cmd = serde_json::json!({
            "command": "ingest",
            "params": {
                "run_output": "tests/a_test.py::test_a PASSED\ntests/b_test.py::test_b PASSED\n"
            }
        });
        let result = cmd_ingest(&cmd);
        assert!(result["ok"].as_bool().unwrap());

        let runtime_edges = result["result"]["runtime_edges"].as_array().unwrap();
        assert_eq!(runtime_edges.len(), 2, "should have one edge per test");

        // Check first edge: from tests/a_test.py::test_a → tests/a_test.py
        let edge = &runtime_edges[0];
        assert_eq!(edge["from"].as_str().unwrap(), "tests/a_test.py::test_a");
        assert_eq!(edge["to"].as_str().unwrap(), "tests/a_test.py");
        assert_eq!(edge["origin"].as_str().unwrap(), "runtime");

        // Check second edge: from tests/b_test.py::test_b → tests/b_test.py
        let edge = &runtime_edges[1];
        assert_eq!(edge["from"].as_str().unwrap(), "tests/b_test.py::test_b");
        assert_eq!(edge["to"].as_str().unwrap(), "tests/b_test.py");
        assert_eq!(edge["origin"].as_str().unwrap(), "runtime");
    }

    #[test]
    fn test_cmd_ingest_empty_output_returns_error() {
        let cmd = serde_json::json!({
            "command": "ingest",
            "params": {
                "run_output": ""
            }
        });
        let result = cmd_ingest(&cmd);
        assert!(!result["ok"].as_bool().unwrap());
        assert!(result["error"].as_str().unwrap().contains("empty"));
    }

    #[test]
    fn test_cmd_ingest_missing_run_output_returns_error() {
        let cmd = serde_json::json!({
            "command": "ingest",
            "params": {}
        });
        let result = cmd_ingest(&cmd);
        assert!(!result["ok"].as_bool().unwrap());
        assert!(result["error"].as_str().unwrap().contains("missing"));
    }

    #[test]
    fn test_cmd_ingest_json_line_format() {
        let cmd = serde_json::json!({
            "command": "ingest",
            "params": {
                "run_output": "{\"test_id\":\"tests/test_x.py::test_x\",\"outcome\":\"passed\"}\n{\"test_id\":\"tests/test_y.py::test_y\",\"outcome\":\"failed\"}\n"
            }
        });
        let result = cmd_ingest(&cmd);
        assert!(result["ok"].as_bool().unwrap());

        let per_test = result["result"]["per_test_results"].as_array().unwrap();
        assert_eq!(per_test.len(), 2);

        let runtime_edges = result["result"]["runtime_edges"].as_array().unwrap();
        assert!(
            !runtime_edges.is_empty(),
            "JSON line format should also produce runtime edges"
        );
    }

    #[test]
    fn test_cmd_ingest_external_inputs_included() {
        let cmd = serde_json::json!({
            "command": "ingest",
            "params": {
                "run_output": "tests/test_model.py::test_something PASSED\n"
            }
        });
        let result = cmd_ingest(&cmd);
        assert!(result["ok"].as_bool().unwrap());
        assert!(result["result"]["external_inputs"].is_array());
    }

    #[test]
    fn test_cmd_discover_excludes_cache_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let cache_dir = dir.path().join("__pycache__");
        std::fs::create_dir_all(&cache_dir).unwrap();

        std::fs::write(cache_dir.join("test_cache.py"), "def test_cache(): pass\n").unwrap();

        let tests_dir = dir.path().join("tests");
        std::fs::create_dir_all(&tests_dir).unwrap();
        std::fs::write(tests_dir.join("test_real.py"), "def test_real(): pass\n").unwrap();

        let orig_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();

        let result = cmd_discover();

        std::env::set_current_dir(&orig_dir).unwrap();

        assert!(result["ok"].as_bool().unwrap());
        let items = result["result"].as_array().unwrap();
        let node_ids: Vec<&str> = items.iter().filter_map(|t| t["node_id"].as_str()).collect();

        assert_eq!(node_ids.len(), 1, "only project test should be discovered");
        assert_eq!(node_ids[0], "tests/test_real.py");
    }
}

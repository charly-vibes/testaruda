# Command-Line Help for `testaruda`

This document contains the help content for the `testaruda` command-line program.

**Command Overview:**

* [`testaruda`↴](#testaruda)
* [`testaruda init`↴](#testaruda-init)
* [`testaruda select`↴](#testaruda-select)
* [`testaruda calibrate`↴](#testaruda-calibrate)
* [`testaruda ingest`↴](#testaruda-ingest)
* [`testaruda graph`↴](#testaruda-graph)
* [`testaruda import`↴](#testaruda-import)
* [`testaruda explain`↴](#testaruda-explain)
* [`testaruda oracle`↴](#testaruda-oracle)
* [`testaruda discover`↴](#testaruda-discover)
* [`testaruda metrics`↴](#testaruda-metrics)
* [`testaruda fingerprint`↴](#testaruda-fingerprint)
* [`testaruda doctor`↴](#testaruda-doctor)
* [`testaruda feedback`↴](#testaruda-feedback)
* [`testaruda completions`↴](#testaruda-completions)
* [`testaruda status`↴](#testaruda-status)

## `testaruda`

Clap-derivable args struct for `--json` / `--human`.

Embed this in your clap CLI struct with `#[command(flatten)]`:

```rust,no_run use clap::Parser; use genesis::guide::CliFormat;

#[derive(Parser)] struct Cli { #[command(flatten)] pub format: CliFormat, }

let cli = Cli::parse(); let fmt = cli.format.format();  // auto-detects TTY vs pipe/agent ```

When neither `--json` nor `--human` is set, the format is auto-detected: - stdout is a terminal (TTY) → `Human` - stdout is piped or redirected → `Json`

This ensures agents and CI pipelines always receive machine-readable JSON by default, while humans at a terminal get readable output.

**Usage:** `testaruda [OPTIONS] <COMMAND>`

###### **Subcommands:**

* `init` — Initialize store and config in the current project
* `select` — Select affected tests from a code change
* `calibrate` — Evaluate the predictive ranking calibration gate (TIA-VER-005)
* `ingest` — Ingest test run results to update the model
* `graph` — Show the current dependency graph
* `import` — Import a dependency graph from a JSON export
* `explain` — Explain why a test was or was not selected
* `oracle` — Run the Soufflé oracle for cross-validation
* `discover` — Discover tests via configured adapters
* `metrics` — Show operational metrics
* `fingerprint` — Refresh all content unit fingerprints from disk
* `doctor` — Validate project configuration via genesis suite_linter
* `feedback` — Submit a feedback issue about an error
* `completions` — Generate shell completions
* `status` — Display cross-tool health summary

###### **Options:**

* `-v`, `--verbose` — Increase output verbosity (-v, -vv, -vvv)
* `-q`, `--quiet` — Suppress non-error output
* `-j`, `--json` — Output machine-readable JSON (default when stdout is not a TTY)
* `--human` — Output human-readable text (default when stdout is a TTY)



## `testaruda init`

Initialize store and config in the current project

**Usage:** `testaruda init`



## `testaruda select`

Select affected tests from a code change

**Usage:** `testaruda select [OPTIONS]`

###### **Options:**

* `--base <BASE>` — Base revision (git ref)
* `--head <HEAD>` — Head revision (git ref)
* `--files <FILES>` — Explicit changed-file list (comma-separated)
* `--shadow` — Shadow mode: compute but report all tests should run (TIA-CI-007)
* `--agent` — Agent output format: structured JSON for LLM agent consumption (TIA-AGENT-001) Conflicts with --pre-edit
* `--pre-edit` — Pre-edit blast radius: report affected tests for proposed changes (TIA-AGENT-005) Conflicts with --agent
* `--ci` — CI mode: run selected tests and ingest results automatically (TIA-CI-008)
* `--safe` — Safe mode: pre-flight checks then fall back to `cargo test` if anything is missing (config, store, git refs) or confidence is low. Implies --ci. Recommended: pass --base and --head to test the diff between two git refs; otherwise falls back to uncommitted changes
* `--ordering <ORDERING>` — Selection ordering mode

  Default value: `default`

  Possible values:
  - `default`:
    No specific ordering — results in Ascent's internal iteration order
  - `deterministic`:
    Byte-stable ordering: sort by test ID (TIA-SEL-005)
  - `duration`:
    Order by descending recorded mean duration (TIA-SEL-006)
  - `predictive`:
    Order by descending historical failure rate (TIA-SEL-007)




## `testaruda calibrate`

Evaluate the predictive ranking calibration gate (TIA-VER-005)

**Usage:** `testaruda calibrate [OPTIONS]`

###### **Options:**

* `--threshold <THRESHOLD>` — Recall threshold (0.0–1.0) for promotion (default: 0.8)

  Default value: `0.8`



## `testaruda ingest`

Ingest test run results to update the model

**Usage:** `testaruda ingest [OPTIONS] <PATH>`

###### **Arguments:**

* `<PATH>` — Path to run output file

###### **Options:**

* `--raw` — Raw test output — delegate to the project's configured adapter for parsing and store runtime edges from the execution
* `--adapter <ADAPTER>` — Adapter binary to use for raw output parsing (default: auto-detect)



## `testaruda graph`

Show the current dependency graph

**Usage:** `testaruda graph`



## `testaruda import`

Import a dependency graph from a JSON export

**Usage:** `testaruda import <PATH>`

###### **Arguments:**

* `<PATH>` — Path to graph JSON file



## `testaruda explain`

Explain why a test was or was not selected

**Usage:** `testaruda explain [OPTIONS] <TEST_ID>`

###### **Arguments:**

* `<TEST_ID>` — Test node ID

###### **Options:**

* `--change <CHANGE>` — Change set reference



## `testaruda oracle`

Run the Soufflé oracle for cross-validation

**Usage:** `testaruda oracle [OPTIONS]`

###### **Options:**

* `--program <PROGRAM>` — Path to Soufflé Datalog program



## `testaruda discover`

Discover tests via configured adapters

**Usage:** `testaruda discover`



## `testaruda metrics`

Show operational metrics

**Usage:** `testaruda metrics`



## `testaruda fingerprint`

Refresh all content unit fingerprints from disk

**Usage:** `testaruda fingerprint`



## `testaruda doctor`

Validate project configuration via genesis suite_linter

**Usage:** `testaruda doctor [OPTIONS]`

###### **Options:**

* `--fix` — Apply safe fixes



## `testaruda feedback`

Submit a feedback issue about an error

**Usage:** `testaruda feedback [OPTIONS] <KIND>`

###### **Arguments:**

* `<KIND>` — Kind of issue (bug|feature|question)

###### **Options:**

* `--from-last-error` — Use the last error from scratch (--from-last-error)
* `--dry-run` — Dry run — print what would be submitted



## `testaruda completions`

Generate shell completions

**Usage:** `testaruda completions <SHELL>`

###### **Arguments:**

* `<SHELL>` — Shell to generate completions for

  Possible values: `bash`, `elvish`, `fish`, `powershell`, `zsh`




## `testaruda status`

Display cross-tool health summary

**Usage:** `testaruda status`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>


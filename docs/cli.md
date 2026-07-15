    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s
     Running `target/debug/testaruda gen-cli-docs`
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

## `testaruda`

Language-agnostic test selection engine — compute the affected test set from a code change via provenance-semiring dependency analysis

**Usage:** `testaruda <COMMAND>`

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
* `--json` — Emit machine-readable JSON plan (TIA-CI-006) Conflicts with --pre-edit and --agent
* `--agent` — Agent output format: structured JSON for LLM agent consumption (TIA-AGENT-001) Conflicts with --json and --pre-edit
* `--pre-edit` — Pre-edit blast radius: report affected tests for proposed changes (TIA-AGENT-005) Conflicts with --json and --agent
* `--ci` — CI mode: run selected tests and ingest results automatically (TIA-CI-008)
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



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>


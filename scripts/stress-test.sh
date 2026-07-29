#!/usr/bin/env bash
#
# stress-test.sh — Multi-language adapter stress-test harness.
#
# Runs the adapter pipeline against a real-world codebase and reports
# structured results for analysis. Supports Python, Rust, Julia,
# TypeScript, and Clojure adapters via auto-detection.
#
# Usage:
#   ./scripts/stress-test.sh [options] <repo-url-or-path>
#
# Options:
#   --base REF         Git base ref for changed files (default: HEAD~1)
#   --head REF         Git head ref for changed files (default: HEAD)
#   --sample N         Max test items to fetch run-args for (default: 10)
#   --output PATH      Write JSON results to file (default: stdout)
#   --adapter PATH     Adapter binary or command string (default: auto-detect from repo)
#                       e.g. "titi testaruda-adapter" for subcommand adapters
#   --test-dir PATH    Subdirectory to discover tests in (for monorepos)
#   --timeout SECS     Timeout per adapter command in seconds (default: 30)
#   --help             Show this help
#
# The script:
#   1. Clones (if URL) or uses the provided repo path
#   2. Auto-detects the adapter from project markers (or --adapter)
#   3. Runs all 6 adapter commands: handshake, discover, fingerprint,
#      static-deps, run-args, ingest
#   4. Times each command and reports errors gracefully
#   5. Outputs structured JSON to stdout
#
# ARG_MAX safety (testaruda-15t): large payloads are written to temp files
# and passed via --argfile to avoid exceeding the kernel E2BIG limit.
##   ./scripts/stress-test.sh target/scratch/click
#   ./scripts/stress-test.sh target/scratch/tokei
#   ./scripts/stress-test.sh --base HEAD~5 --head HEAD target/scratch/httpx
#   ./scripts/stress-test.sh --sample 50 --output results.json .
#   for d in target/scratch/*/; do ./scripts/stress-test.sh "$d"; done
#   # For monorepos, discover tests from subpackage:
#   ./scripts/stress-test.sh --test-dir packages/zod target/scratch/zod
#
# Dependencies: jq, git, and at least one adapter binary (on PATH, --adapter, or auto-detected)
set -euo pipefail

# ============================================================================
# Defaults
# ============================================================================
BASE="HEAD~1"
HEAD="HEAD"
SAMPLE=10
ADAPTER=""
TIMEOUT=30
OUTPUT=""
TEST_DIR=""
REPO=""

# ============================================================================
# Parse arguments
# ============================================================================
while [[ $# -gt 0 ]]; do
    case "$1" in
        --base)      shift; BASE="$1" ;;
        --head)      shift; HEAD="$1" ;;
        --sample)    shift; SAMPLE="$1" ;;
        --output)    shift; OUTPUT="$1" ;;
        --adapter)   shift; ADAPTER="$1" ;;
        --timeout)   shift; TIMEOUT="$1" ;;
        --test-dir)  shift; TEST_DIR="$1" ;;
        --help)      head -40 "$0" | grep -E "^#" | sed 's/^# \?//'; exit 0 ;;
        *)           REPO="$1" ;;
    esac
    shift
done

if [[ -z "$REPO" ]]; then
    echo "Error: <repo-url-or-path> is required" >&2
    exit 1
fi

# ============================================================================
# Helpers
# ============================================================================

# Send a single JSON command to the adapter, return the single response line.
# Uses a fresh adapter subprocess per command (like the integration tests).
# Supports command-string adapters (e.g., "titi testaruda-adapter") via
# $ADAPTER_BIN and $ADAPTER_ARGS (set during adapter resolution).
adapter_command() {
    local cmd="$1"
    local cwd="${2:-.}"

    local resp
    if [[ -n "$ADAPTER_ARGS" ]]; then
        resp="$(cd "$cwd" && echo "$cmd" | timeout "$TIMEOUT" "$ADAPTER_BIN" $ADAPTER_ARGS 2>/dev/null | head -1 || true)"
    else
        resp="$(cd "$cwd" && echo "$cmd" | timeout "$TIMEOUT" "$ADAPTER_BIN" 2>/dev/null | head -1 || true)"
    fi
    echo "$resp"
}

# JSON-safe string: convert to a single-line JSON string
# Writes to a temp file and uses --rawfile to avoid ARG_MAX issues
# with very long paths on systems with small limits (testaruda-15t).
json_str() {
    local val="$1"
    if [[ -z "$val" ]]; then
        echo '""'
        return
    fi
    local tmpfile
    tmpfile="$(mktemp "${TMPDIR:-/tmp}/testaruda_jq.XXXXXX" 2>/dev/null)"
    if [[ -z "$tmpfile" ]]; then
        # fallback if mktemp fails
        jq -n --arg v "$val" '$v'
        return
    fi
    printf '%s' "$val" > "$tmpfile"
    jq -n --rawfile v "$tmpfile" '$v'
    rm -f "$tmpfile"
}

# JSON-safe number
json_num() {
    if [[ -z "$1" ]]; then echo "0"; else echo "$1"; fi
}

# Auto-detect adapter from repo contents
detect_adapter() {
    local dir="$1"
    if [[ -f "$dir/Cargo.toml" ]]; then echo "testaruda-adapter-rust"
    elif [[ -f "$dir/pyproject.toml" || -f "$dir/setup.py" || -f "$dir/setup.cfg" || -f "$dir/requirements.txt" || -f "$dir/Pipfile" ]]; then echo "testaruda-adapter-python"
    elif [[ -f "$dir/Project.toml" ]]; then echo "testaruda-adapter-julia"
    elif [[ -f "$dir/vitest.config.ts" || -f "$dir/vitest.config.js" || -f "$dir/jest.config.ts" || -f "$dir/jest.config.js" ]]; then echo "testaruda-adapter-typescript"
    elif [[ -f "$dir/deps.edn" || -f "$dir/project.clj" ]]; then echo "testaruda-adapter-clojure"
    elif ls "$dir"/*.csproj &>/dev/null 2>&1 || ls "$dir"/*.sln &>/dev/null 2>&1 || ls "$dir"/*.slnx &>/dev/null 2>&1; then echo "titi testaruda-adapter"
    else echo ""; fi
}

# Resolve adapter binary path for non-command-string adapters.
# Returns the resolved single binary path or empty string.
resolve_adapter_binary() {
    local bin="$1"
    if command -v "$bin" &>/dev/null; then echo "$bin"; return 0; fi
    if [[ -x "$bin" ]]; then
        if [[ "$bin" != /* ]]; then
            echo "$(cd . && realpath "$bin" 2>/dev/null || echo "$bin")"
        else
            echo "$bin"
        fi
        return 0
    fi
    local script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    if [[ -x "$script_dir/target/debug/$bin" ]]; then
        echo "$script_dir/target/debug/$bin"
        return 0
    fi
    if [[ -x "$script_dir/target/release/$bin" ]]; then
        echo "$script_dir/target/release/$bin"
        return 0
    fi
    echo ""
    return 1
}

# Split command string into binary and args for array-safe invocation.
# Echoes the binary path on the first line and args (space-separated) on the second.
# For single-word adapters, the second line is empty.
resolve_adapter() {
    local input="$1"
    # If it looks like a command string (contains space), split it
    if [[ "$input" == *" "* ]]; then
        local bin="${input%% *}"
        local args="${input#* }"
        local resolved="$(resolve_adapter_binary "$bin")"
        if [[ -z "$resolved" ]]; then
            echo "$bin"
            echo "$args"
            return 1
        fi
        echo "$resolved"
        echo "$args"
        return 0
    fi
    resolved="$(resolve_adapter_binary "$input")"
    if [[ -z "$resolved" ]]; then
        echo "$input"
        echo ""
        return 1
    fi
    echo "$resolved"
    echo ""
    return 0
}

# Get the repo slug from URL or path
get_repo_slug() {
    local input="$1"
    if [[ "$input" == https://* ]] || [[ "$input" == git@* ]]; then
        echo "$input" | sed -E 's|^.*[:/]([^/]+/[^/]+)\.git$|\1|; s|^.*[:/]([^/]+/[^/]+)$|\1|' | head -1
    else
        basename "$(cd "$input" 2>/dev/null && pwd || echo "$input")"
    fi
}

# Clone a repo into target/scratch/
clone_repo() {
    local url="$1"
    local slug="$2"
    local dest="target/scratch/$slug"

    if [[ -d "$dest" ]]; then
        echo "  Repo already cloned at $dest, updating..." >&2
        (cd "$dest" && git pull --ff-only 2>/dev/null || true)
    else
        echo "  Cloning $url -> $dest ..." >&2
        mkdir -p target/scratch
        git clone --depth=100 "$url" "$dest" 2>/dev/null || {
            echo "Error: failed to clone $url" >&2
            exit 1
        }
    fi
    echo "$dest"
}

# ============================================================================
# Main
# ============================================================================

# Resolve repo path
WORK_DIR=""
if [[ "$REPO" == https://* ]] || [[ "$REPO" == git@* ]]; then
    SLUG="$(get_repo_slug "$REPO")"
    WORK_DIR="$(clone_repo "$REPO" "$SLUG")"
else
    WORK_DIR="$(cd "$REPO" && pwd)"
    SLUG="$(basename "$WORK_DIR")"
fi

if [[ ! -d "$WORK_DIR" ]]; then
    echo "Error: not a directory: $WORK_DIR" >&2
    exit 1
fi

# Detect adapter if not specified
if [[ -z "$ADAPTER" ]]; then
    DETECTED="$(detect_adapter "$WORK_DIR")"
    if [[ -z "$DETECTED" ]]; then
        echo "Error: cannot auto-detect adapter for $SLUG. Specify --adapter <binary>" >&2
        exit 1
    fi
    ADAPTER="$DETECTED"
fi

RESOLVED="$(resolve_adapter "$ADAPTER")" || true
# Parse resolved adapter — first line is binary, second is args (if command-string)
ADAPTER_BIN="$(echo "$RESOLVED" | head -1)"
ADAPTER_ARGS="$(echo "$RESOLVED" | tail -1)"

if [[ -z "$ADAPTER_BIN" ]] || ! command -v "$ADAPTER_BIN" &>/dev/null && [[ ! -x "$ADAPTER_BIN" ]]; then
    echo "Error: adapter binary not found: $ADAPTER" >&2
    echo "  Build it: cargo build --bin $ADAPTER" >&2
    exit 1
fi
ADAPTER_DISPLAY="$ADAPTER_BIN${ADAPTER_ARGS:+ $ADAPTER_ARGS}"

echo "=== Stress-testing $ADAPTER_DISPLAY against: $SLUG ===" >&2

# ============================================================================
# Phase 1: Handshake
# ============================================================================
echo "  [1/6] Handshake..." >&2

START="$(date +%s%N)"
HANDSHAKE_RESPONSE="$(adapter_command '{"command":"handshake"}' "$WORK_DIR")"
HANDSHAKE_DURATION=$(( ($(date +%s%N) - START) / 1000000 ))

HANDSHAKE_OK=false
HANDSHAKE_CAPABILITIES="null"
HANDSHAKE_ERROR="null"
if echo "$HANDSHAKE_RESPONSE" | jq -e '.ok == true' >/dev/null 2>&1; then
    HANDSHAKE_OK=true
    HANDSHAKE_CAPABILITIES="$(echo "$HANDSHAKE_RESPONSE" | jq '.result')"
else
    HANDSHAKE_ERROR="$(echo "$HANDSHAKE_RESPONSE" | jq '.error // "unknown error"' 2>/dev/null || echo '"no response"')"
fi

# ============================================================================
# Phase 2: Discover
# ============================================================================
echo "  [2/6] Discover..." >&2

START="$(date +%s%N)"

if [[ -n "$TEST_DIR" ]]; then
    DISCOVER_CMD=$(jq -c -n --arg td "$TEST_DIR" '{"command":"discover","params":{"test_directories":[$td]}}')
else
    DISCOVER_CMD='{"command":"discover"}'
fi
DISCOVER_RESPONSE="$(adapter_command "$DISCOVER_CMD" "$WORK_DIR")"
DISCOVER_DURATION=$(( ($(date +%s%N) - START) / 1000000 ))

DISCOVER_OK=false
DISCOVER_COUNT=0
DISCOVER_FILES="[]"
DISCOVER_ERROR="null"
if echo "$DISCOVER_RESPONSE" | jq -e '.ok == true' >/dev/null 2>&1; then
    DISCOVER_OK=true
    # Handle two discover response formats:
    #   canonical: result = [TestItem, ...]
    #   titi:      result.tests = [TestItem, ...]
    if echo "$DISCOVER_RESPONSE" | jq -e '.result | type == "array"' >/dev/null 2>&1; then
        DISCOVER_COUNT="$(echo "$DISCOVER_RESPONSE" | jq '.result | length')"
        DISCOVER_FILES="$(echo "$DISCOVER_RESPONSE" | jq '.result | map(.file)')"
    elif echo "$DISCOVER_RESPONSE" | jq -e '.result.tests | type == "array"' >/dev/null 2>&1; then
        DISCOVER_COUNT="$(echo "$DISCOVER_RESPONSE" | jq '.result.tests | length')"
        DISCOVER_FILES="$(echo "$DISCOVER_RESPONSE" | jq '.result.tests | map(.file)')"
    else
        DISCOVER_ERROR='"unexpected discover response format"'
    fi
else
    DISCOVER_ERROR="$(echo "$DISCOVER_RESPONSE" | jq '.error // "unknown error"' 2>/dev/null || echo '"no response"')"
fi

# ============================================================================
# Phase 3: Fingerprint
# ============================================================================
echo "  [3/6] Fingerprint..." >&2

FINGERPRINT_OK=false
FINGERPRINT_COUNT=0
FINGERPRINT_ERROR="null"
FINGERPRINT_DURATION=0

if [[ "$DISCOVER_COUNT" -gt 0 ]]; then
    # Build file list from discover results
    FILE_ARGS="$(echo "$DISCOVER_RESPONSE" | jq -r '.result | map(.file) | .[]')"
    FP_CMD='{"command":"fingerprint","params":{"files":['
    first=true
    while IFS= read -r f; do
        if [[ -z "$f" ]]; then continue; fi
        if $first; then first=false; else FP_CMD+=','; fi
        FP_CMD+="$(json_str "$f")"
    done <<< "$FILE_ARGS"
    FP_CMD+=']}}'

    START="$(date +%s%N)"
    FP_RESPONSE="$(adapter_command "$FP_CMD" "$WORK_DIR")"
    FINGERPRINT_DURATION=$(( ($(date +%s%N) - START) / 1000000 ))

    if echo "$FP_RESPONSE" | jq -e '.ok == true' >/dev/null 2>&1; then
        FINGERPRINT_OK=true
        FINGERPRINT_COUNT="$(echo "$FP_RESPONSE" | jq '.fingerprints // .result.fingerprints // [] | length')"
    else
        FINGERPRINT_ERROR="$(echo "$FP_RESPONSE" | jq '.error // "fingerprint failed"' 2>/dev/null || echo '"no response"')"
    fi
fi

# ============================================================================
# Phase 4: Static-deps (via git diff)
# ============================================================================
echo "  [4/6] Static-deps..." >&2

STATIC_DEPS_OK=false
STATIC_DEPS_CHANGED=0
STATIC_DEPS_EDGES=0
STATIC_DEPS_UNRESOLVED=0
STATIC_DEPS_ERROR="null"
STATIC_DEPS_DURATION=0

CHANGED_FILES="$(cd "$WORK_DIR" && git diff --name-only "$BASE" "$HEAD" 2>/dev/null || true)"
CHANGED_COUNT="$(echo "$CHANGED_FILES" | grep -c . || true)"

if [[ "$CHANGED_COUNT" -gt 0 ]]; then
    SD_CMD='{"command":"static-deps","params":{"changed_files":['
    first=true
    while IFS= read -r f; do
        if [[ -z "$f" ]]; then continue; fi
        if $first; then first=false; else SD_CMD+=','; fi
        SD_CMD+="$(json_str "$f")"
    done <<< "$CHANGED_FILES"
    SD_CMD+=']}}'

    START="$(date +%s%N)"
    SD_RESPONSE="$(adapter_command "$SD_CMD" "$WORK_DIR")"
    STATIC_DEPS_DURATION=$(( ($(date +%s%N) - START) / 1000000 ))

    if echo "$SD_RESPONSE" | jq -e '.ok == true' >/dev/null 2>&1; then
        STATIC_DEPS_OK=true
        STATIC_DEPS_CHANGED="$CHANGED_COUNT"
        STATIC_DEPS_EDGES=$(echo "$SD_RESPONSE" | jq '[.edges // .result.edges // {} | to_entries[] | .value | length] | add // 0')
        STATIC_DEPS_UNRESOLVED=$(echo "$SD_RESPONSE" | jq '[.edges // .result.edges // {} | to_entries[] | select(.value == "unresolved")] | length')
    else
        STATIC_DEPS_ERROR="$(echo "$SD_RESPONSE" | jq '.error // "static-deps failed"' 2>/dev/null || echo '"no response"')"
    fi
fi

# ============================================================================
# Phase 5: Run-args (sample)
# ============================================================================
echo "  [5/6] Run-args..." >&2

RUN_ARGS_OK=false
RUN_ARGS_COUNT=0
RUN_ARGS_ERROR="null"
RUN_ARGS_DURATION=0

if [[ "$DISCOVER_COUNT" -gt 0 ]]; then
    SELECTED_NODES="$(echo "$DISCOVER_RESPONSE" | jq -r ".result | map(.node_id) | .[0:$SAMPLE] | .[]")"

    RA_CMD='{"command":"run-args","params":{"selected":['
    first=true
    while IFS= read -r nid; do
        if [[ -z "$nid" ]]; then continue; fi
        if $first; then first=false; else RA_CMD+=','; fi
        RA_CMD+="$(json_str "$nid")"
    done <<< "$SELECTED_NODES"
    RA_CMD+=']}}'

    START="$(date +%s%N)"
    RA_RESPONSE="$(adapter_command "$RA_CMD" "$WORK_DIR")"
    RUN_ARGS_DURATION=$(( ($(date +%s%N) - START) / 1000000 ))

    if echo "$RA_RESPONSE" | jq -e '.ok == true' >/dev/null 2>&1; then
        RUN_ARGS_OK=true
        RUN_ARGS_COUNT="$(echo "$SELECTED_NODES" | grep -c . || true)"
    else
        RUN_ARGS_ERROR="$(echo "$RA_RESPONSE" | jq '.error // "run-args failed"' 2>/dev/null || echo '"no response"')"
    fi
fi

# ============================================================================
# Phase 6: Ingest (simulated)
# ============================================================================
echo "  [6/6] Ingest..." >&2

INGEST_OK=false
INGEST_COUNT=0
INGEST_ERROR="null"
INGEST_DURATION=0

if [[ "$DISCOVER_COUNT" -gt 0 ]]; then
    # Simulate test runner output for discovered tests
    SIMULATED_LINES="$(echo "$DISCOVER_RESPONSE" | jq -r ".result | map(.node_id) | .[0:10] | .[]")"

    if [[ -z "$SIMULATED_LINES" ]]; then
        echo "  WARNING: no test node_ids found, skipping ingest" >&2
        INGEST_ERROR='"no node_ids in discover results"'
    else
        # Build newline-separated run_output
        RUN_OUTPUT=""
        first=true
        while IFS= read -r tid; do
            if [[ -z "$tid" ]]; then continue; fi
            if $first; then first=false; else RUN_OUTPUT+=$'\n'; fi
            RUN_OUTPUT+="{\"test_id\":\"$tid\",\"outcome\":\"passed\",\"duration_ms\":5}"
        done <<< "$SIMULATED_LINES"

        # Escape the run_output for JSON
        RUN_OUTPUT_ESCAPED="$(echo "$RUN_OUTPUT" | jq -Rs .)"

        INGEST_CMD="{\"command\":\"ingest\",\"params\":{\"run_output\":$RUN_OUTPUT_ESCAPED}}"

        START="$(date +%s%N)"
        INGEST_RESPONSE="$(adapter_command "$INGEST_CMD" "$WORK_DIR")"
        INGEST_DURATION=$(( ($(date +%s%N) - START) / 1000000 ))

        if echo "$INGEST_RESPONSE" | jq -e '.ok == true' >/dev/null 2>&1; then
            INGEST_OK=true
            INGEST_COUNT="$(echo "$INGEST_RESPONSE" | jq '.result.per_test_results | length')"
        else
            INGEST_ERROR="$(echo "$INGEST_RESPONSE" | jq '.error' 2>/dev/null || echo null)"
        fi
    fi
fi

# ============================================================================
# Output
# ============================================================================

REPO_URL="$(cd "$WORK_DIR" && git remote get-url origin 2>/dev/null || echo "")"
HEAD_HASH="$(cd "$WORK_DIR" && git rev-parse HEAD 2>/dev/null || echo "")"
HEAD_MSG="$(cd "$WORK_DIR" && git log --oneline -1 2>/dev/null || echo "")"

# Write large payloads to temp files to avoid ARG_MAX (testaruda-15t)
DISCOVER_FILES_FILE="${TMPDIR:-/tmp}/testaruda_discover_files.$$.json"
printf '%s' "$DISCOVER_FILES" > "$DISCOVER_FILES_FILE"

OUTPUT_JSON="$(jq -n \
    --arg slug "$SLUG" \
    --arg repo_url "$REPO_URL" \
    --arg head_hash "$HEAD_HASH" \
    --arg head_msg "$HEAD_MSG" \
    --argjson handshake_ok "$HANDSHAKE_OK" \
    --argjson handshake_duration "$HANDSHAKE_DURATION" \
    --arg handshake_capabilities "$HANDSHAKE_CAPABILITIES" \
    --arg handshake_error "$HANDSHAKE_ERROR" \
    --argjson discover_ok "$DISCOVER_OK" \
    --argjson discover_duration "$DISCOVER_DURATION" \
    --argjson discover_count "$DISCOVER_COUNT" \
    --rawfile discover_files "$DISCOVER_FILES_FILE" \
    --arg discover_error "$DISCOVER_ERROR" \
    --argjson fingerprint_ok "$FINGERPRINT_OK" \
    --argjson fingerprint_duration "$FINGERPRINT_DURATION" \
    --argjson fingerprint_count "$FINGERPRINT_COUNT" \
    --arg fingerprint_error "$FINGERPRINT_ERROR" \
    --argjson static_deps_ok "$STATIC_DEPS_OK" \
    --argjson static_deps_duration "$STATIC_DEPS_DURATION" \
    --argjson static_deps_changed "$STATIC_DEPS_CHANGED" \
    --argjson static_deps_edges "$STATIC_DEPS_EDGES" \
    --argjson static_deps_unresolved "$STATIC_DEPS_UNRESOLVED" \
    --arg static_deps_error "$STATIC_DEPS_ERROR" \
    --argjson run_args_ok "$RUN_ARGS_OK" \
    --argjson run_args_duration "$RUN_ARGS_DURATION" \
    --argjson run_args_count "$RUN_ARGS_COUNT" \
    --arg run_args_error "$RUN_ARGS_ERROR" \
    --argjson ingest_ok "$INGEST_OK" \
    --argjson ingest_duration "$INGEST_DURATION" \
    --argjson ingest_count "$INGEST_COUNT" \
    --arg ingest_error "$INGEST_ERROR" \
    --arg adapter "$ADAPTER" \
    '{
  "schema": "https://testaruda.dev/schemas/stress-test-v1",
  "generated": (now | strftime("%Y-%m-%dT%H:%M:%SZ")),
  "repo": {
    "slug": $slug,
    "url": $repo_url,
    "head": $head_hash,
    "head_message": $head_msg
  },
  "adapter": $adapter,
  "results": {
    "handshake": {
      "ok": $handshake_ok,
      "duration_ms": $handshake_duration,
      "capabilities": $handshake_capabilities,
      "error": ($handshake_error | fromjson? // null)
    },
    "discover": {
      "ok": $discover_ok,
      "duration_ms": $discover_duration,
      "test_count": $discover_count,
      "test_files": ($discover_files | fromjson),
      "error": ($discover_error | fromjson? // null)
    },
    "fingerprint": {
      "ok": $fingerprint_ok,
      "duration_ms": $fingerprint_duration,
      "file_count": $fingerprint_count,
      "error": ($fingerprint_error | fromjson? // null)
    },
    "static_deps": {
      "ok": $static_deps_ok,
      "duration_ms": $static_deps_duration,
      "changed_files": $static_deps_changed,
      "edges": $static_deps_edges,
      "unresolved": $static_deps_unresolved,
      "error": ($static_deps_error | fromjson? // null)
    },
    "run_args": {
      "ok": $run_args_ok,
      "duration_ms": $run_args_duration,
      "selected_count": $run_args_count,
      "error": ($run_args_error | fromjson? // null)
    },
    "ingest": {
      "ok": $ingest_ok,
      "duration_ms": $ingest_duration,
      "ingested_count": $ingest_count,
      "error": ($ingest_error | fromjson? // null)
    }
  }
}')"

if [[ -n "$OUTPUT" ]]; then
    echo "$OUTPUT_JSON" > "$OUTPUT"
    echo "Results written to $OUTPUT" >&2
else
    echo "$OUTPUT_JSON"
fi

# Print summary to stderr
echo "=== Summary ===" >&2
echo "  Handshake:    ${HANDSHAKE_DURATION}ms" >&2
echo "  Discover:     ${DISCOVER_DURATION}ms (${DISCOVER_COUNT} tests)" >&2
echo "  Fingerprint:  ${FINGERPRINT_DURATION}ms (${FINGERPRINT_COUNT} files)" >&2
echo "  Static-deps:  ${STATIC_DEPS_DURATION}ms (${STATIC_DEPS_CHANGED} files, ${STATIC_DEPS_EDGES} edges, ${STATIC_DEPS_UNRESOLVED} unresolved)" >&2
echo "  Run-args:     ${RUN_ARGS_DURATION}ms (${RUN_ARGS_COUNT} selected)" >&2
echo "  Ingest:       ${INGEST_DURATION}ms (${INGEST_COUNT} ingested)" >&2
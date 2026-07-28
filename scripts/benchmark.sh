#!/usr/bin/env bash
#
# benchmark.sh — Multi-language adapter benchmark.
#
# Measures discover, fingerprint, and static-deps performance across
# codebases of varying sizes. Outputs structured JSON for CI trend tracking.
#
# Usage:
#   ./scripts/benchmark.sh [options] <repo-url-or-path> [<repo-url-or-path>...]
#
# Options:
#   --adapter PATH   Adapter binary (default: auto-detect from repo)
#   --iterations N   Number of runs per measurement (default: 3)
#   --output PATH    Write JSON results to file (default: stdout)
#   --timeout SECS   Timeout per adapter command in seconds (default: 30)
#   --help           Show this help
#
# Examples:
#   ./scripts/benchmark.sh target/scratch/click
#   ./scripts/benchmark.sh --iterations 5 --output results.json target/scratch/*
#   ./scripts/benchmark.sh --adapter target/debug/testaruda-adapter-rust target/scratch/tokei
set -euo pipefail

# ============================================================================
# Defaults
# ============================================================================
ADAPTER=""
ITERATIONS=3
TIMEOUT=30
OUTPUT=""
REPOS=()

# ============================================================================
# Parse arguments
# ============================================================================
while [[ $# -gt 0 ]]; do
    case "$1" in
        --adapter)    shift; ADAPTER="$1" ;;
        --iterations) shift; ITERATIONS="$1" ;;
        --output)     shift; OUTPUT="$1" ;;
        --timeout)    shift; TIMEOUT="$1" ;;
        --help)       head -30 "$0" | grep -E "^#" | sed 's/^# \?//'; exit 0 ;;
        *)            REPOS+=("$1") ;;
    esac
    shift
done

if [[ ${#REPOS[@]} -eq 0 ]]; then
    echo "Error: at least one <repo-url-or-path> is required" >&2
    exit 1
fi

# ============================================================================
# Helpers
# ============================================================================

adapter_command() {
    local cmd="$1"
    local cwd="${2:-.}"
    local adapter_bin="${ADAPTER_BIN:-$ADAPTER}"
    local resp
    resp="$(cd "$cwd" && echo "$cmd" | timeout "$TIMEOUT" "$adapter_bin" 2>/dev/null | head -1 || true)"
    echo "$resp"
}

detect_adapter() {
    local dir="$1"
    if [[ -f "$dir/Cargo.toml" ]]; then echo "testaruda-adapter-rust"
    elif [[ -f "$dir/pyproject.toml" || -f "$dir/setup.py" || -f "$dir/setup.cfg" || -f "$dir/requirements.txt" || -f "$dir/Pipfile" ]]; then echo "testaruda-adapter-python"
    elif [[ -f "$dir/Project.toml" ]]; then echo "testaruda-adapter-julia"
    elif [[ -f "$dir/vitest.config.ts" || -f "$dir/vitest.config.js" || -f "$dir/jest.config.ts" || -f "$dir/jest.config.js" ]]; then echo "testaruda-adapter-typescript"
    elif [[ -f "$dir/deps.edn" || -f "$dir/project.clj" ]]; then echo "testaruda-adapter-clojure"
    else echo ""; fi
}

resolve_repo() {
    local input="$1"
    if [[ "$input" == https://* ]] || [[ "$input" == git@* ]]; then
        local slug
        slug="$(echo "$input" | sed -E 's|^.*[:/]([^/]+/[^/]+)\.git$|\1|; s|^.*[:/]([^/]+/[^/]+)$|\1|' | head -1)"
        local dest="target/scratch/$slug"
        if [[ -d "$dest" ]]; then
            (cd "$dest" && git pull --ff-only 2>/dev/null || true)
        else
            mkdir -p target/scratch
            git clone --depth=100 "$input" "$dest" 2>/dev/null || {
                echo "Error: failed to clone $input" >&2
                exit 1
            }
        fi
        echo "$dest"
    else
        (cd "$input" && pwd)
    fi
}

json_str() { jq -n --arg v "$1" '$v'; }

# Run N iterations, collect times as a newline-separated string
run_timed() {
    local cmd="$1"
    local cwd="$2"
    local times_str=""
    for ((i=1; i<=ITERATIONS; i++)); do
        local start end elapsed
        start="$(date +%s%N)"
        resp="$(adapter_command "$cmd" "$cwd")"
        end="$(date +%s%N)"
        elapsed=$(( (end - start) / 1000000 ))
        times_str+="$elapsed"$'\n'
    done
    echo "$times_str"
}

compute_stats() {
    local times_str="$1"
    local -a times=()
    while IFS= read -r line; do
        [[ -n "$line" ]] && times+=("$line")
    done <<< "$times_str"
    if [[ ${#times[@]} -eq 0 ]]; then
        echo "0 0 0"
        return
    fi
    local min="${times[0]}" max="${times[0]}" sum=0
    for t in "${times[@]}"; do
        (( t < min )) && min=$t
        (( t > max )) && max=$t
        (( sum += t ))
    done
    local mean=$(( sum / ${#times[@]} ))
    echo "$min $max $mean"
}

# ============================================================================
# Main
# ============================================================================

ALL_RESULTS='[]'

for REPO_INPUT in "${REPOS[@]}"; do
    WORK_DIR="$(resolve_repo "$REPO_INPUT")"
    SLUG="$(basename "$WORK_DIR")"

    if [[ -z "$ADAPTER" ]]; then
        DETECTED="$(detect_adapter "$WORK_DIR")"
        if [[ -z "$DETECTED" ]]; then
            echo "Warning: cannot auto-detect adapter for $SLUG, skipping..." >&2
            continue
        fi
        ADAPTER_BIN="$DETECTED"
        if ! command -v "$ADAPTER_BIN" &>/dev/null; then
            if [[ -x "target/debug/$DETECTED" ]]; then
                ADAPTER_BIN="$(cd . && realpath "target/debug/$DETECTED" 2>/dev/null || echo "target/debug/$DETECTED")"
            fi
        fi
    else
        ADAPTER_BIN="$ADAPTER"
        # Resolve to absolute path if relative
        if [[ "$ADAPTER_BIN" != /* ]] && ! command -v "$ADAPTER_BIN" &>/dev/null; then
            ADAPTER_BIN="$(cd . && realpath "$ADAPTER_BIN" 2>/dev/null || echo "$ADAPTER_BIN")"
        fi
    fi

    if [[ ! -x "$ADAPTER_BIN" ]] && ! command -v "$ADAPTER_BIN" &>/dev/null; then
        echo "Warning: adapter $ADAPTER_BIN not available for $SLUG, skipping..." >&2
        continue
    fi

    echo "=== Benchmarking $SLUG with $ADAPTER_BIN ===" >&2

    # Handshake
    echo "  [handshake]..." >&2
    HS_RESPONSE="$(adapter_command '{"command":"handshake"}' "$WORK_DIR")"
    HS_OK=$(echo "$HS_RESPONSE" | jq '.ok == true' 2>/dev/null || echo false)
    if [[ "$HS_OK" != "true" ]]; then
        echo "Warning: handshake failed for $SLUG, skipping..." >&2
        continue
    fi

    # Discover
    echo "  [discover] ($ITERATIONS iterations)..." >&2
    DISCOVER_RESPONSE=""
    DISCOVER_TIMES_STR=""
    for ((i=1; i<=ITERATIONS; i++)); do
        start="$(date +%s%N)"
        resp="$(adapter_command '{"command":"discover"}' "$WORK_DIR")"
        end="$(date +%s%N)"
        elapsed=$(( (end - start) / 1000000 ))
        DISCOVER_RESPONSE="$resp"
        DISCOVER_TIMES_STR+="$elapsed"$'\n'
    done
    read -r DISCOVER_MIN DISCOVER_MAX DISCOVER_MEAN <<< "$(compute_stats "$DISCOVER_TIMES_STR")"
    DISCOVER_COUNT="$(echo "$DISCOVER_RESPONSE" | jq '.result | length' 2>/dev/null || echo 0)"

    # Fingerprint
    echo "  [fingerprint] ($ITERATIONS iterations)..." >&2
    FP_CMD='{"command":"fingerprint","params":{"files":['
    if [[ "$DISCOVER_COUNT" -gt 0 ]]; then
        first=true
        while IFS= read -r f; do
            [[ -z "$f" ]] && continue
            if $first; then first=false; else FP_CMD+=','; fi
            FP_CMD+="$(json_str "$f")"
        done < <(echo "$DISCOVER_RESPONSE" | jq -r '.result[].file')
    fi
    FP_CMD+=']}}'

    FP_TIMES_STR=""
    FP_FILES=0
    for ((i=1; i<=ITERATIONS; i++)); do
        start="$(date +%s%N)"
        resp="$(adapter_command "$FP_CMD" "$WORK_DIR")"
        end="$(date +%s%N)"
        elapsed=$(( (end - start) / 1000000 ))
        FP_TIMES_STR+="$elapsed"$'\n'
        if [[ "$i" -eq 1 ]]; then
            FP_FILES="$(echo "$resp" | jq '[.result.fingerprints // .fingerprints // []][0] | length' 2>/dev/null || echo 0)"
        fi
    done
    read -r FP_MIN FP_MAX FP_MEAN <<< "$(compute_stats "$FP_TIMES_STR")"

    # Static-deps
    echo "  [static-deps] ($ITERATIONS iterations)..." >&2
    CHANGED_FILES="$(cd "$WORK_DIR" && git diff --name-only HEAD~1 HEAD 2>/dev/null || true)"
    CHANGED_COUNT="$(echo "$CHANGED_FILES" | grep -c . || true)"

    SD_TIMES_STR=""
    SD_EDGES=0
    SD_UNRESOLVED=0
    if [[ "$CHANGED_COUNT" -gt 0 ]]; then
        SD_CMD='{"command":"static-deps","params":{"changed_files":['
        first=true
        while IFS= read -r f; do
            [[ -z "$f" ]] && continue
            if $first; then first=false; else SD_CMD+=','; fi
            SD_CMD+="$(json_str "$f")"
        done <<< "$CHANGED_FILES"
        SD_CMD+=']}}'

        for ((i=1; i<=ITERATIONS; i++)); do
            start="$(date +%s%N)"
            resp="$(adapter_command "$SD_CMD" "$WORK_DIR")"
            end="$(date +%s%N)"
            elapsed=$(( (end - start) / 1000000 ))
            SD_TIMES_STR+="$elapsed"$'\n'
            if [[ "$i" -eq 1 ]]; then
                SD_EDGES=$(echo "$resp" | jq '[.edges // .result.edges // {} | to_entries[] | .value | length] | add // 0')
                SD_UNRESOLVED=$(echo "$resp" | jq '[.edges // .result.edges // {} | to_entries[] | select(.value == "unresolved")] | length')
            fi
        done
    fi
    read -r SD_MIN SD_MAX SD_MEAN <<< "$(compute_stats "$SD_TIMES_STR")"

    # Memory
    MEM_PEAK_KB=0
    if [[ -f /proc/self/status ]]; then
        MEM_PEAK_KB="$(grep VmPeak /proc/self/status 2>/dev/null | awk '{print $2}' || echo 0)"
    fi

    # Build result
    REPO_URL="$(cd "$WORK_DIR" && git remote get-url origin 2>/dev/null || echo "")"
    HEAD_HASH="$(cd "$WORK_DIR" && git rev-parse HEAD 2>/dev/null || echo "")"

    RESULT="$(jq -n \
        --arg slug "$SLUG" \
        --arg repo_url "$REPO_URL" \
        --arg head_hash "$HEAD_HASH" \
        --arg adapter "$ADAPTER_BIN" \
        --argjson test_count "$DISCOVER_COUNT" \
        --argjson discover_min "$DISCOVER_MIN" \
        --argjson discover_max "$DISCOVER_MAX" \
        --argjson discover_mean "$DISCOVER_MEAN" \
        --argjson fp_files "$FP_FILES" \
        --argjson fp_min "$FP_MIN" \
        --argjson fp_max "$FP_MAX" \
        --argjson fp_mean "$FP_MEAN" \
        --argjson sd_changed "$CHANGED_COUNT" \
        --argjson sd_edges "$SD_EDGES" \
        --argjson sd_unresolved "$SD_UNRESOLVED" \
        --argjson sd_min "$SD_MIN" \
        --argjson sd_max "$SD_MAX" \
        --argjson sd_mean "$SD_MEAN" \
        --argjson mem_peak_kb "$MEM_PEAK_KB" \
        '{slug: $slug, url: $repo_url, head: $head_hash, adapter: $adapter,
          test_count: $test_count,
          discover: {min_ms: $discover_min, max_ms: $discover_max, mean_ms: $discover_mean},
          fingerprint: {files: $fp_files, min_ms: $fp_min, max_ms: $fp_max, mean_ms: $fp_mean},
          static_deps: {changed: $sd_changed, edges: $sd_edges, unresolved: $sd_unresolved, min_ms: $sd_min, max_ms: $sd_max, mean_ms: $sd_mean},
          mem_peak_kb: $mem_peak_kb,
          generated: (now | strftime("%Y-%m-%dT%H:%M:%SZ"))}')"

    ALL_RESULTS="$(echo "$ALL_RESULTS" | jq --argjson r "$RESULT" '. + [$r]')"

    echo "=== Results: $SLUG ===" >&2
    printf "  Discover:      %d tests  %d/%d/%d ms (min/mean/max)\n" "$DISCOVER_COUNT" "$DISCOVER_MIN" "$DISCOVER_MEAN" "$DISCOVER_MAX" >&2
    printf "  Fingerprint:   %d files  %d/%d/%d ms\n" "$FP_FILES" "$FP_MIN" "$FP_MEAN" "$FP_MAX" >&2
    printf "  Static-deps:   %d changed, %d edges, %d unresolved  %d/%d/%d ms\n" "$CHANGED_COUNT" "$SD_EDGES" "$SD_UNRESOLVED" "$SD_MIN" "$SD_MEAN" "$SD_MAX" >&2
    printf "  Memory (peak): %d KB\n" "$MEM_PEAK_KB" >&2
done

FINAL="$(jq -n \
    --argjson results "$ALL_RESULTS" \
    --argjson iterations "$ITERATIONS" \
    '{schema: "https://testaruda.dev/schemas/benchmark-v1", generated: (now | strftime("%Y-%m-%dT%H:%M:%SZ")), iterations: $iterations, results: $results}')"

if [[ -n "$OUTPUT" ]]; then
    echo "$FINAL" > "$OUTPUT"
    echo "Results written to $OUTPUT" >&2
else
    echo "$FINAL"
fi
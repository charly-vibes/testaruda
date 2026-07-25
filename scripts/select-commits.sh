#!/usr/bin/env bash
#
# select-commits.sh — Git history mining tool for stress-testing adapters.
#
# Given a git repository, selects commits suitable for stress testing
# based on configurable criteria. Filters by commit type (source-only,
# test-only, mixed, bug-fix), file count range, and samples across the
# repo's history.
#
# Usage:
#   ./scripts/select-commits.sh [options] [<repo-path>]
#
# Options:
#   --min-files N     Minimum changed files per commit (default: 1)
#   --max-files N     Maximum changed files per commit (default: 5)
#   --sample N        Maximum number of commits to output (default: 50)
#   --types LIST      Comma-separated: source,test,mixed,bugfix (default: all)
#   --skip-merges     Skip merge commits (default: true)
#   --skip-deps       Skip dependency-only commits (default: true)
#   --since REF       Only commits after this ref (e.g., --since HEAD~500)
#   --format FORMAT   Output format: json (default) or text
#   --help            Show this help
#
# Examples:
#   ./scripts/select-commits.sh /path/to/repo
#   ./scripts/select-commits.sh --min-files 10 --max-files 50 --sample 5 .
#   ./scripts/select-commits.sh --types bugfix --sample 10 .
#   ./scripts/select-commits.sh --since HEAD~200 --format text .
#
# Output schema (JSON):
#   {
#     "repo": "org/repo",
#     "args": { ... },
#     "commits": [
#       {
#         "hash": "abc123def",
#         "message": "fix: handle edge case in parser",
#         "changed_files": ["src/parser.py", "tests/test_parser.py"],
#         "categories": {
#           "source": ["src/parser.py"],
#           "test": ["tests/test_parser.py"],
#           "config": []
#         },
#         "file_count": 2,
#         "type": "mixed"
#       }
#     ]
#   }
set -euo pipefail

# ============================================================================
# Defaults
# ============================================================================
MIN_FILES=1
MAX_FILES=5
SAMPLE=50
TYPES="source,test,mixed,bugfix"
SKIP_MERGES=true
SKIP_DEPS=true
SINCE=""
FORMAT="json"
REPO=""

# ============================================================================
# Parse arguments
# ============================================================================
while [[ $# -gt 0 ]]; do
    case "$1" in
        --min-files)   shift; MIN_FILES="$1" ;;
        --max-files)   shift; MAX_FILES="$1" ;;
        --sample)      shift; SAMPLE="$1" ;;
        --types)       shift; TYPES="$1" ;;
        --no-skip-merges) SKIP_MERGES=false ;;
        --no-skip-deps)   SKIP_DEPS=false ;;
        --since)       shift; SINCE="$1" ;;
        --format)      shift; FORMAT="$1" ;;
        --help)        head -40 "$0" | grep -E "^#" | sed 's/^# \?//'; exit 0 ;;
        *)             REPO="$1" ;;
    esac
    shift
done

if [[ -z "$REPO" ]]; then
    REPO="."
fi

REPO="$(cd "$REPO" 2>/dev/null && pwd)" || {
    echo "Error: cannot access repo path: $REPO" >&2
    exit 1
}

if [[ ! -d "$REPO/.git" ]]; then
    echo "Error: not a git repository: $REPO" >&2
    exit 1
fi

# ============================================================================
# Helpers
# ============================================================================

# Get the repo slug for JSON output (org/repo form)
get_repo_slug() {
    local remote
    remote="$(cd "$REPO" && git remote get-url origin 2>/dev/null || true)"
    if [[ -z "$remote" ]]; then
        basename "$REPO"
        return
    fi
    # Extract org/repo from various remote URL formats
    # Strip .git suffix, extract org/repo from path
    local slug="$(echo "$remote" | sed 's|\.git$||')"
    echo "$slug" | sed -E 's|^.*[:/]([^/]+/[^/]+)$|\1|'
}

# Classify a file path into source, test, config, dep, doc, or other
classify_file() {
    local path="$1"
    local basename
    basename="$(basename "$path")"

    # Test files
    if [[ "$basename" == test_*.py ]] || [[ "$basename" == *_test.py ]] \
        || [[ "$path" == */test_* ]] || [[ "$path" == */tests/* ]] \
        || [[ "$path" == */spec/* ]] || [[ "$path" == */spec_* ]]; then
        echo "test"
        return
    fi

    # Generated/dependency directories
    case "$path" in
        */target/*|*/node_modules/*|*/vendor/*|*/__pycache__/*|*/.venv/*|*/venv/*|*/build/*|*/dist/*|*/.mypy_cache/*|*/.pytest_cache/*|*/.tox/*)
            echo "dep"; return ;;
    esac

    # Lock files
    case "$basename" in
        Cargo.lock|Gemfile.lock|poetry.lock|yarn.lock|package-lock.json|pnpm-lock.yaml|composer.lock)
            echo "dep"; return ;;
    esac

    # Config files
    case "$basename" in
        *.toml|*.cfg|*.ini|*.yml|*.yaml|*.json|*.conf|Makefile|Dockerfile)
            echo "config"; return ;;
    esac

    # Source files (by extension)
    case "$basename" in
        *.py|*.rs|*.jl|*.ts|*.js|*.tsx|*.jsx|*.go|*.java|*.c|*.cpp|*.h|*.hpp|*.rb|*.scala|*.clj|*.cljs|*.ex|*.exs|*.zig|*.swift|*.kt|*.kts|*.dart)
            echo "source"; return ;;
        *.md|*.rst|*.org|*.txt)
            echo "doc"; return ;;
    esac

    echo "other"
}

# Determine commit type from its changed files
# Returns: source, test, mixed, dep-only, doc-only, other
classify_commit() {
    local has_source=false
    local has_test=false
    local has_dep=true   # starts true; set to false if any non-dep file found
    local f type

    for f in "$@"; do
        type="$(classify_file "$f")"
        case "$type" in
            source) has_source=true; has_dep=false ;;
            test)   has_test=true;   has_dep=false ;;
            config) has_dep=false ;;
            doc)    has_dep=false ;;
            dep)    ;; # dep-only marker
            *)      has_dep=false ;; # "other" counts as non-dep
        esac
    done

    if "$has_source" && "$has_test"; then
        echo "mixed"
    elif "$has_source" && ! "$has_test"; then
        echo "source"
    elif "$has_test" && ! "$has_source"; then
        echo "test"
    elif "$has_dep"; then
        echo "dep-only"
    else
        echo "other"
    fi
}

# Check if this is a bug-fix commit (heuristic: message contains fix/patch/bug keywords)
is_bugfix() {
    local lc
    lc="$(echo "$1" | tr '[:upper:]' '[:lower:]')"
    [[ "$lc" == *fix:* ]] && return 0
    [[ "$lc" == *fix\(* ]] && return 0
    [[ "$lc" == *fixes* ]] && return 0
    [[ "$lc" == *fixing* ]] && return 0
    [[ "$lc" == *bugfix* ]] && return 0
    [[ "$lc" == *bug\ fix* ]] && return 0
    [[ "$lc" == *hotfix* ]] && return 0
    [[ "$lc" == *patch* ]] && return 0
    [[ "$lc" == *resolve* ]] && return 0
    [[ "$lc" == *closes\ \#* ]] && return 0
    [[ "$lc" == *close\ \#* ]] && return 0
    return 1
}

# Check if the commit type is in the allowed types list
type_is_allowed() {
    local commit_type="$1"
    local IFS=','
    for t in $TYPES; do
        t="$(echo "$t" | xargs)"
        if [[ "$t" == "$commit_type" ]]; then
            return 0
        fi
    done
    return 1
}

# ============================================================================
# Main: walk git history and select commits
# ============================================================================

cd "$REPO"

# Build git log command
GIT_LOG_ARGS=(log --no-merges --reverse --name-only --format="COMMIT:%H %s")
if [[ -n "$SINCE" ]]; then
    GIT_LOG_ARGS+=( "$SINCE" )
fi

# Collect selected commits as JSONL in a temp file
TMPFILE="$(mktemp)"
trap 'rm -f "$TMPFILE"' EXIT

count=0
current_hash=""
current_msg=""
current_files=()
in_commit=false

while IFS= read -r line; do
    # Start of a new commit
    if [[ "$line" == COMMIT:* ]]; then
        # Process previous commit if we have one
        if "$in_commit" && [[ ${#current_files[@]} -gt 0 ]]; then
            ctype="$(classify_commit "${current_files[@]}")"
            file_count="${#current_files[@]}"

            # Apply filters
            pass=true

            # File count filter
            if [[ $file_count -lt $MIN_FILES || $file_count -gt $MAX_FILES ]]; then
                pass=false
            fi

            # Dep-only filter
            if "$SKIP_DEPS" && [[ "$ctype" == "dep-only" ]]; then
                pass=false
            fi

            # Type filter
            if "$pass"; then
                if is_bugfix "$current_msg"; then
                    if type_is_allowed "bugfix"; then
                        ctype="bugfix"
                    else
                        pass=false
                    fi
                elif ! type_is_allowed "$ctype"; then
                    pass=false
                fi
            fi

            if "$pass"; then
                # Build category arrays
                source_files=()
                test_files=()
                config_files=()
                for f in "${current_files[@]}"; do
                    ft="$(classify_file "$f")"
                    case "$ft" in
                        source) source_files+=("$f") ;;
                        test)   test_files+=("$f") ;;
                        config) config_files+=("$f") ;;
                    esac
                done

                cat >> "$TMPFILE" <<JSONLEOF
{"hash":"$current_hash","message":$(echo "$current_msg" | jq -Rs .),"changed_files":$(printf '%s\n' "${current_files[@]}" | jq -R . | jq -s -c .),"categories":{"source":$([ ${#source_files[@]} -eq 0 ] && echo '[]' || printf '%s\n' "${source_files[@]}" | jq -R . | jq -s -c .),"test":$([ ${#test_files[@]} -eq 0 ] && echo '[]' || printf '%s\n' "${test_files[@]}" | jq -R . | jq -s -c .),"config":$([ ${#config_files[@]} -eq 0 ] && echo '[]' || printf '%s\n' "${config_files[@]}" | jq -R . | jq -s -c .)},"file_count":$file_count,"type":"$ctype"}
JSONLEOF

                count=$((count + 1))
            fi
        fi

        # Parse new commit header
        current_hash="${line#COMMIT: }"
        current_hash="${current_hash#COMMIT:}"
        current_msg=""
        current_files=()
        in_commit=true

        # Split into hash and message
        h="${current_hash%% *}"
        m="${current_hash#* }"
        if [[ -n "$h" && -n "$m" ]]; then
            current_hash="$h"
            current_msg="$m"
        fi

    elif [[ -n "$line" ]]; then
        # File path line
        current_files+=("$line")
    fi

    if [[ $count -ge $SAMPLE ]]; then
        break
    fi
done < <(git "${GIT_LOG_ARGS[@]}")

# Process the last buffered commit (not handled by the COMMIT: trigger above)
if "$in_commit" && [[ ${#current_files[@]} -gt 0 ]] && [[ $count -lt $SAMPLE ]]; then
    ctype="$(classify_commit "${current_files[@]}")"
    file_count="${#current_files[@]}"

    pass=true
    if [[ $file_count -lt $MIN_FILES || $file_count -gt $MAX_FILES ]]; then
        pass=false
    fi
    if "$SKIP_DEPS" && [[ "$ctype" == "dep-only" ]]; then
        pass=false
    fi
    if "$pass"; then
        if is_bugfix "$current_msg"; then
            if type_is_allowed "bugfix"; then
                ctype="bugfix"
            else
                pass=false
            fi
        elif ! type_is_allowed "$ctype"; then
            pass=false
        fi
    fi

    if "$pass"; then
        source_files=(); test_files=(); config_files=()
        for f in "${current_files[@]}"; do
            ft="$(classify_file "$f")"
            case "$ft" in
                source) source_files+=("$f") ;;
                test)   test_files+=("$f") ;;
                config) config_files+=("$f") ;;
            esac
        done

        cat >> "$TMPFILE" <<JSONLEOF
{"hash":"$current_hash","message":$(echo "$current_msg" | jq -Rs .),"changed_files":$(printf '%s\n' "${current_files[@]}" | jq -R . | jq -s -c .),"categories":{"source":$([ ${#source_files[@]} -eq 0 ] && echo '[]' || printf '%s\n' "${source_files[@]}" | jq -R . | jq -s -c .),"test":$([ ${#test_files[@]} -eq 0 ] && echo '[]' || printf '%s\n' "${test_files[@]}" | jq -R . | jq -s -c .),"config":$([ ${#config_files[@]} -eq 0 ] && echo '[]' || printf '%s\n' "${config_files[@]}" | jq -R . | jq -s -c .)},"file_count":$file_count,"type":"$ctype"}
JSONLEOF
        count=$((count + 1))
    fi
fi

# ============================================================================
# Output
# ============================================================================
REPO_SLUG="$(get_repo_slug)"

if [[ "$FORMAT" == "json" ]]; then
    jq -n \
        --arg repo "$REPO_SLUG" \
        --argjson min_files "$MIN_FILES" \
        --argjson max_files "$MAX_FILES" \
        --argjson sample "$SAMPLE" \
        --arg types "$TYPES" \
        --argjson skip_merges "$SKIP_MERGES" \
        --argjson skip_deps "$SKIP_DEPS" \
        --slurpfile commits "$TMPFILE" \
        '{
           repo: $repo,
           args: {
             min_files: $min_files,
             max_files: $max_files,
             sample: $sample,
             types: ($types / ","),
             skip_merges: $skip_merges,
             skip_deps: $skip_deps
           },
           commits: $commits
         }'
elif [[ "$FORMAT" == "text" ]]; then
    printf "%-8s %-5s %-10s %s\n" "TYPE" "FILES" "HASH" "MESSAGE"
    printf "%-8s %-5s %-10s %s\n" "----" "-----" "----" "-------"
    while IFS= read -r line; do
        hash="$(echo "$line" | jq -r '.hash' | head -c 10)"
        msg="$(echo "$line" | jq -r '.message' | head -c 50)"
        ctype="$(echo "$line" | jq -r '.type')"
        fc="$(echo "$line" | jq -r '.file_count')"
        printf "%-8s %-5s %-10s %s\n" "$ctype" "$fc" "$hash" "$msg"
    done < "$TMPFILE"
    echo ""
    echo "Total: $count commits"
fi
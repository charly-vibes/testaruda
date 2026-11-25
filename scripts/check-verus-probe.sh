#!/usr/bin/env bash
# Purpose: the Verus verification-probe gate (beads `testaruda-rqvc`),
# mirroring scripts/check-kani-probe.sh's shape. This is a PROBE with an
# extend/hold/retire gate — deliberately NOT part of `just ci` and not
# an assurance lane.
#
# Responsibilities: (1) read verus-version.toml and refuse to run under
# any other installed release; provision the pinned zip (sha256
# checked) into ~/.local/share/verus/<release> if missing, idempotently;
# (2) verify every model proof unit in verus-probe/model/ with the
# pinned release (--crate-type lib), parsing the `verification
# results::` line — three-valued outcome per unit (VERIFIED / ERROR /
# EXHAUSTED; a run with no results line is ERROR, never silently green;
# wall-capped 300 s, timeout classified EXHAUSTED); (3) provenance
# guard: the parity crate's transcribed exec walk must match the
# verified model's exec skeleton (normalized) — silent transcription
# drift fails the gate (the kani-probe verbatim-copy lesson); (4) run
# the parity driver (verus-probe/parity, plain cargo) — PARITY PASS
# required, including the chain-contract non-vacuity witness;
# (5) record wall time per unit into target/verus-probe-timings.txt and
# exit nonzero on any error, exhaustion, drift, or parity failure.
#
# Rationale: no systemd memory scope, unlike the kani lane — verus runs
# here are sub-second SMT phases under the release's own rlimits.

set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERUS_TIMEOUT=300
PIN="verus-version.toml"
MODEL_DIR="verus-probe/model"
PARITY_DIR="verus-probe/parity"
TIMINGS="target/verus-probe-timings.txt"

fail() {
	echo "verus probe: $1" >&2
	exit 1
}

pin_field() {
	awk -F'"' -v k="$1" '$0 ~ k" = " {print $2; exit}' "$PIN"
}

[ -f "$PIN" ] || fail "$PIN missing"
RELEASE="$(pin_field release)"
URL="$(pin_field url)"
SHA="$(pin_field sha256)"
[ -n "$RELEASE" ] && [ -n "$URL" ] && [ -n "$SHA" ] || fail "$PIN is missing release/url/sha256"

command -v sha256sum >/dev/null || fail "sha256sum not found on PATH"
command -v cargo >/dev/null || fail "cargo not found on PATH"

# ---- provisioning ------------------------------------------------------
VERUS_HOME="${VERUS_HOME:-$HOME/.local/share/verus}"
INSTALL="$VERUS_HOME/$RELEASE"
VERUS_BIN="$INSTALL/verus"

if [ ! -x "$VERUS_BIN" ]; then
	echo "verus probe: provisioning $RELEASE into $INSTALL"
	mkdir -p "$INSTALL" "$VERUS_HOME/tmp"
	zip="$VERUS_HOME/tmp/$RELEASE.zip"
	curl -fsSL -o "$zip" "$URL" || fail "download of $URL failed"
	echo "$SHA  $zip" | sha256sum -c - >/dev/null ||
		fail "pinned zip sha256 mismatch for $URL"
	unzip -o -q "$zip" -d "$VERUS_HOME/tmp/$RELEASE"
	mv "$VERUS_HOME/tmp/$RELEASE"/*/* "$INSTALL"/ 2>/dev/null ||
		mv "$VERUS_HOME/tmp/$RELEASE"/* "$INSTALL"/
	rm -rf "$VERUS_HOME/tmp/$RELEASE" "$zip"
fi

"$VERUS_BIN" --version | grep -q "$RELEASE" ||
	fail "installed verus does not report the pinned release $RELEASE"

mkdir -p target
: > "$TIMINGS"

run_verus() { # <file> <outfile> [extra args...]
	local file="$1" out="$2"
	shift 2
	local t0 t1 rc
	t0=$(date +%s%N)
	timeout "$VERUS_TIMEOUT" "$VERUS_BIN" "$@" "$file" >"$out" 2>&1
	rc=$?
	t1=$(date +%s%N)
	printf '%s\t%dms\n' "$file" "$(( (t1 - t0) / 1000000 ))" >>"$TIMINGS"
	return $rc
}

# ---- 1. model proof units ----------------------------------------------
echo "verus probe: model proof units ($MODEL_DIR)"
model_units=0
model_out=$(mktemp)
trap 'rm -f "$model_out"' EXIT
for unit in "$MODEL_DIR"/*.rs; do
	[ -e "$unit" ] || fail "no model proof units under $MODEL_DIR"
	rc=0
	run_verus "$unit" "$model_out" --crate-type lib || rc=$?
	out=$(cat "$model_out")
	if [ $rc -eq 124 ]; then
		status=EXHAUSTED
	else
		results=$(printf '%s\n' "$out" | grep -oE "verification results:: [0-9]+ verified, [0-9]+ errors" | tail -1 || true)
		if [ -z "$results" ]; then
			if printf '%s\n' "$out" | grep -qi "rlimit"; then
				status=EXHAUSTED
			else
				status=ERROR
			fi
		else
			errors=$(printf '%s' "$results" | sed -E 's/.*verified, ([0-9]+) errors/\1/')
			if [ "$errors" -eq 0 ]; then
				status=VERIFIED
			elif printf '%s\n' "$out" | grep -qi "rlimit"; then
				status=EXHAUSTED
			else
				status=ERROR
			fi
		fi
	fi
	case "$status" in
	VERIFIED) ;;
	*) fail "model unit $(basename "$unit"): $status
$(printf '%s\n' "$out" | tail -5)" ;;
	esac
	echo "  VERIFIED  $(basename "$unit")  ($results)"
	model_units=$((model_units + 1))
done
[ "$model_units" -gt 0 ] || fail "model produced no proof-unit outcomes"

# ---- 2. provenance guard ------------------------------------------------
# The parity crate's transcribed exec walk must match the verified
# model's exec skeleton. Both files are reduced to their sequence of
# exec skeleton — loop guards + effectful statements in order. Verus
# proof blocks are brace-counted out, ghost bindings dropped, and
# verus-only syntax mapped to plain rust before the diff.
skeleton() { # <file> <fn-name>
	sed -n "/fn $2(/,/^\}/p" "$1" |
		awk '
			/proof \{/ && !inproof { inproof = 1; depth = 0; next }
			inproof {
				depth += gsub(/\{/, "{") - gsub(/\}/, "}");
				if (depth < 0) inproof = 0;
				next
			}
			/let ghost/ { next }
			{ print }
		' |
		grep -E 'while |\.push\(|\.set\(|\.clone\(\)|if visited\[|if u == |if impacted\[|if comp_fallback|if aff|= hit;|= true;' |
		sed -E 's/^[[:space:]]+//; s/[[:space:]]+$//;
			s/if aff_t \{/if aff[t]/;
			s/if aff\[t\] \{/if aff[t]/;
			s/ \{$//;
			s/\.set\(([a-z_]+) as usize, true\);/[\1 as usize] = true;/;
			s/\.set\(([a-z_]+), ([a-z_]+)\);/[\1] = \2;/'
}

# The transcription lives in parity/src/main.rs; extract the same fns
# (they are plain fns there).
parity_file="$PARITY_DIR/src/main.rs"
for fn in impacted_exec_transcribed affected_exec_transcribed; do
	case "$fn" in
	impacted_exec_transcribed) model_fn=impacted_exec ;;
	affected_exec_transcribed) model_fn=affected_exec ;;
	esac
	model_skel="$(skeleton "$unit" "$model_fn")" || true
	parity_skel="$(skeleton "$parity_file" "$fn")" || true
	[ -n "$model_skel" ] || fail "provenance guard: no skeleton extracted for $model_fn from $unit"
	[ -n "$parity_skel" ] || fail "provenance guard: no skeleton extracted for $fn from $parity_file"
	if [ "$model_skel" != "$parity_skel" ]; then
		fail "provenance guard: exec skeleton drift between $model_fn (model) and $fn (parity)
--- model
$model_skel
--- parity
$parity_skel"
	fi
	echo "  PROVENANCE  $model_fn <-> $fn  (skeleton match)"
done

# ---- 3. parity gate -----------------------------------------------------
echo "verus probe: parity gate ($parity_file)"
( cd "$PARITY_DIR" && cargo build -q --offline 2>/dev/null || cargo build -q ) ||
	fail "parity driver build failed"
PARITY_BIN="$PARITY_DIR/target/debug/verus-probe-parity"
[ -x "$PARITY_BIN" ] || fail "parity driver binary missing"
parity_out=$("$PARITY_BIN") || fail "parity driver exited nonzero:
$parity_out"
printf '%s\n' "$parity_out"
printf '%s\n' "$parity_out" | grep -q "PARITY PASS" ||
	fail "parity gate did not pass"
printf '%s\n' "$parity_out" | grep -q "chain-contract: match" ||
	fail "non-vacuity witness missing (chain-contract did not match)"

echo "verus probe: PROBE VERIFIED — $model_units model unit(s) VERIFIED, parity clean"
echo "verus probe: timings in $TIMINGS"

#!/usr/bin/env bash
# Purpose: the Kani bounded-model-checking probe gate (beads
# `testaruda-jdpi`) — provisioning check against the committed pin
# (kani-version.toml), one `cargo kani` run per declared probe harness,
# the three-valued outcome per harness (proved / counterexample /
# undetermined), and the disposition rule.
#
# Responsibilities: (1) refuse to run under any release other than the
# committed pin; (2) run each harness ALWAYS inside a systemd user scope
# capped at KANI_MEM_BUDGET_MB (default 8192, swap off) — the
# catherine-amh incident (uncapped CBMC OOM-killing the workstation at
# 15.4 GB RSS) is the reason; a host without `systemd-run --user`
# refuses rather than runs unbounded; (3) per-harness solver budget
# KANI_HARNESS_TIMEOUT (default 240) via `--harness-timeout` (needs
# `-Z unstable-options` on Kani 0.68.0 — without it every invocation
# errors instantly, catherine's measured flag bug); (4) classify: rc
# 137/143 (budget kill), a timeout or unwinding-assertion banner, or a
# missing VERIFICATION line is UNDETERMINED or a harness error — never a
# counterexample, never silently green; (5) record outcome, wall, and
# peak-RSS into target/kani-probe-timings.txt; (6) exit nonzero on a
# counterexample, a harness error, or an UNDETERMINED whose disposition
# is not recorded in docs/research/kani-probe.md
# (`<harness>: undetermined — ...`); exit 0 only when every harness is
# proved or undetermined-with-disposition.
#
# Rationale: deliberately NOT part of `just test`/CI — a toolchain every
# checkout cannot assume (the Lean-gate precedent). The probe runs
# `--unwinding-assertions`, so an insufficient unwind bound is reported
# (UNDETERMINED) rather than silently cut. KANI_HARNESS_FILTER=<regex>
# runs a maintenance subset under identical enforcement and the
# identical disposition rule — it never weakens either.
#
# Why a separate crate (kani-probe/, verbatim copy — see its header):
# Kani 0.68.0 cannot compile testaruda's real dependency closure — its
# rebuilt std prelude collides with `backtrace`'s `#[macro_use]`
# (pulled by miette), failing the build outright with E0659 at backtrace
# 0.3.75/0.3.76 (`--no-assert-overrides`, the documented escape hatch,
# does NOT clear it). The catherine-engine pattern — a dependency-light
# kernel crate the checker compiles — is the fix a REAL lane must adopt;
# the probe measures tractability on a verbatim copy meanwhile.

set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MEM_BUDGET_MB="${KANI_MEM_BUDGET_MB:-8192}"
HARNESS_TIMEOUT="${KANI_HARNESS_TIMEOUT:-240}"
UNWIND="${KANI_UNWIND:-16}"
FILTER="${KANI_HARNESS_FILTER:-}"
DOC="docs/research/kani-probe.md"
TIMINGS="target/kani-probe-timings.txt"

fail() { echo "kani probe: $1" >&2; exit 1; }

# The declared probe harness list — keep in sync with
# kani-probe/src/lib.rs (every #[kani::proof] fn).
DECLARED_HARNESSES=(
	pipeline_panic_freedom
	selection_soundness_no_phantom_tests
	confidence_ceiling_invariant
	fallback_completeness
	floor_collect_panic_freedom
	floor_contract_two_sided
	collect_lookup_consistency
)

command -v cargo >/dev/null 2>&1 || fail "cargo not found on PATH"
command -v kani >/dev/null 2>&1 || fail "kani not found on PATH"
systemd-run --user --scope true 2>/dev/null || fail "systemd-run --user scope unavailable — refusing to run uncapped"

PIN="$(awk -F'"' '/^release = /{print $2; exit}' kani-version.toml)"
[ -n "$PIN" ] || fail "kani-version.toml has no release pin"
INSTALLED="$(cargo kani --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || true)"
[ "$PIN" = "$INSTALLED" ] ||
	fail "pin mismatch: kani-version.toml pins $PIN, installed is ${INSTALLED:-unknown} (provision: cargo install --locked kani-verifier@$PIN && cargo kani setup)"
echo "kani probe: pin OK ($INSTALLED), unwind $UNWIND, harness-timeout ${HARNESS_TIMEOUT}s, cap ${MEM_BUDGET_MB}MB"

mkdir -p target
: > "$TIMINGS"

failures=0
for h in "${DECLARED_HARNESSES[@]}"; do
	if [ -n "$FILTER" ] && ! [[ "$h" =~ $FILTER ]]; then
		echo "kani probe: $h skipped (filter)"
		continue
	fi

	log="target/kani-probe-$h.log"
	timefile="target/kani-probe-$h.time"
	start=$SECONDS
	set +e
	( cd "$ROOT/kani-probe" && systemd-run --user --scope -p "MemoryMax=${MEM_BUDGET_MB}M" -p MemorySwapMax=0 \
		/usr/bin/time -v -o "$ROOT/$timefile" \
		cargo kani -Z unstable-options --default-unwind "$UNWIND" \
		--harness-timeout "$HARNESS_TIMEOUT" --exact --harness "$h" ) >"$ROOT/$log" 2>&1
	hrc=$?
	set -e
	wall=$((SECONDS - start))

	unit="$(grep -oE 'run-[0-9]+-[a-z0-9-]+\.scope' "$log" | head -1 || true)"
	scope_peak="$(systemctl --user show -p MemoryPeak --value "$unit" 2>/dev/null || true)"
	peak_kb="$(grep -oE 'Maximum resident set size \(kbytes\): [0-9]+' "$timefile" 2>/dev/null | grep -oE '[0-9]+$' | tail -1 || true)"
	peak="${peak_kb:-${scope_peak:-unknown}}"

	outcome=""
	if [ "$hrc" -eq 137 ] || [ "$hrc" -eq 143 ]; then
		outcome="UNDETERMINED (scope kill rc=$hrc)"
	elif grep -qE 'VERIFICATION:- SUCCESSFUL' "$log"; then
		outcome="PROVED"
	elif grep -qE 'VERIFICATION:- FAILED' "$log"; then
		if grep -qiE 'CBMC timed out|unwinding assertion|unwind' "$log"; then
			outcome="UNDETERMINED (bound/timeout)"
		else
			outcome="COUNTEREXAMPLE"
		fi
	else
		outcome="HARNESS_ERROR (rc=$hrc)"
	fi

	echo "kani probe: $h → $outcome (wall ${wall}s, peak ${peak}B)"
	printf '%s\t%s\t%s\t%s\t%s\n' "$h" "$hrc" "$outcome" "$wall" "$peak" >>"$TIMINGS"

	case "$outcome" in
	PROVED) ;;
	*UNDETERMINED*)
		if ! grep -qE "^$h: undetermined —" "$DOC" 2>/dev/null; then
			echo "kani probe: FAIL — $h is UNDETERMINED with no recorded disposition in $DOC" >&2
			failures=$((failures + 1))
		fi
		;;
	*)
		echo "kani probe: FAIL — $h: $outcome" >&2
		failures=$((failures + 1))
		;;
	esac
done

if [ "$failures" -gt 0 ]; then
	fail "$failures harness(es) failed the disposition rule"
fi
echo "kani probe: all declared harnesses proved or undetermined-with-disposition"

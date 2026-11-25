// Verus probe parity driver (beads `testaruda-rqvc`).
//
// Purpose: concrete-fixture cross-check for the verification probe in
// `../model/selection_reachability.rs` (impacted snapshot relaxation +
// affected marking, panic-freedom and functional spec discharged by Z3
// under the pinned Verus release).
//
// Provenance caveat (the kani-probe precedent, testaruda-jdpi): the
// `model_exec` functions below are a TRANSCRIPTION of the verified
// exec functions — this verus distribution verifies only
// (`--crate-type lib`, no binary emission), so the proven code cannot
// be linked here directly. Drift between this transcription and the
// model invalidates the parity signal, not the model proof; the gate
// script's provenance guard greps this file against the model's exec
// bodies (normalized) so silent divergence fails the gate.
//
// The `reference` functions mirror the shipped selector's Boolean
// semantics (`src/selection`): impacted = the CU_CAP-step closure of
// (changed ∪ unresolved) over cu-dep edges; affected = always-run ∪
// impacted-dep tests ∪ fallback-component tests, ascending. A mismatch
// here means the probe model or this transcription has drifted from
// the shipped core — re-read the model header before "fixing" either
// side.

const CU_CAP: usize = 8;
const T_CAP: usize = 8;
const COMP_CAP: usize = 8;

// ---- transcribed verified exec (see provenance caveat) ----------------

fn impacted_exec_transcribed(seeds: &[u32], deps: &[(u32, u32)]) -> Vec<bool> {
    let mut visited: Vec<bool> = Vec::new();
    let mut c: usize = 0;
    while c < CU_CAP {
        visited.push(seeds.contains(&(c as u32)));
        c += 1;
    }
    let mut pass: usize = 0;
    while pass < CU_CAP {
        let mut next = visited.clone();
        let mut a: usize = 0;
        while a < CU_CAP {
            let mut hit = false;
            if visited[a] {
                hit = true;
            }
            let mut j: usize = 0;
            while j < deps.len() {
                let (u, b) = deps[j];
                if u == a as u32 && visited[b as usize] {
                    hit = true;
                }
                j += 1;
            }
            next[a] = hit;
            a += 1;
        }
        visited = next;
        pass += 1;
    }
    visited
}

fn affected_exec_transcribed(
    impacted: &[bool],
    test_deps: &[(u32, u32)],
    always_run: &[u32],
    comp_fallback: &[u32],
    test_comp: &[(u32, u32)],
) -> Vec<u32> {
    let mut aff: Vec<bool> = Vec::new();
    let mut t: usize = 0;
    while t < T_CAP {
        aff.push(always_run.contains(&(t as u32)));
        t += 1;
    }
    let mut i: usize = 0;
    while i < test_deps.len() {
        let (td, tc) = test_deps[i];
        if impacted[tc as usize] {
            aff[td as usize] = true;
        }
        i += 1;
    }
    let mut i: usize = 0;
    while i < test_comp.len() {
        let (td, ck) = test_comp[i];
        if comp_fallback.contains(&ck) {
            aff[td as usize] = true;
        }
        i += 1;
    }
    let mut affected: Vec<u32> = Vec::new();
    let mut t: usize = 0;
    while t < T_CAP {
        if aff[t] {
            affected.push(t as u32);
        }
        t += 1;
    }
    affected
}

// ---- plain-Rust reference of the shipped Boolean core -----------------

fn impacted_reference(seeds: &[u32], deps: &[(u32, u32)]) -> Vec<bool> {
    let mut visited = vec![false; CU_CAP];
    for &s in seeds {
        if (s as usize) < CU_CAP {
            visited[s as usize] = true;
        }
    }
    for _pass in 0..CU_CAP {
        let mut next = visited.clone();
        for a in 0..CU_CAP {
            let mut hit = visited[a];
            for &(u, b) in deps {
                if u == a as u32 && (b as usize) < CU_CAP && visited[b as usize] {
                    hit = true;
                }
            }
            next[a] = hit;
        }
        visited = next;
    }
    visited
}

fn affected_reference(
    impacted: &[bool],
    test_deps: &[(u32, u32)],
    always_run: &[u32],
    comp_fallback: &[u32],
    test_comp: &[(u32, u32)],
) -> Vec<u32> {
    let mut aff = vec![false; T_CAP];
    for &t in always_run {
        if (t as usize) < T_CAP {
            aff[t as usize] = true;
        }
    }
    for &(td, tc) in test_deps {
        if (td as usize) < T_CAP && (tc as usize) < CU_CAP && impacted[tc as usize] {
            aff[td as usize] = true;
        }
    }
    for &(td, ck) in test_comp {
        if (td as usize) < T_CAP && comp_fallback.contains(&ck) {
            aff[td as usize] = true;
        }
    }
    (0..T_CAP).filter(|&t| aff[t]).map(|t| t as u32).collect()
}

// ---- fixtures ----------------------------------------------------------

fn main() {
    let mut failures = 0usize;

    let impacted_fixtures: Vec<(&str, Vec<u32>, Vec<(u32, u32)>)> = vec![
        ("empty", vec![], vec![]),
        ("seed-only", vec![5], vec![]),
        ("chain", vec![0], vec![(1, 0), (2, 1), (3, 2)]),
        ("diamond", vec![0], vec![(1, 0), (2, 0), (3, 1), (3, 2)]),
        ("cycle", vec![0], vec![(1, 0), (0, 1), (2, 1)]),
        ("unreachable", vec![1], vec![(2, 3), (3, 4)]),
        ("all-seeds", (0..CU_CAP as u32).collect(), vec![(7, 6)]),
    ];
    for (name, seeds, deps) in &impacted_fixtures {
        let got = impacted_exec_transcribed(seeds, deps);
        let want = impacted_reference(seeds, deps);
        let ok = got == want;
        println!("{name}: {}", if ok { "match" } else { "MISMATCH got={got:?} want={want:?}" });
        if !ok {
            failures += 1;
        }
    }

    // impacted contract spot-check on the chain fixture: the closure of
    // {0} over 0<-1<-2<-3 is exactly {0,1,2,3} (the Z3 proof's k-step
    // claim, exercised concretely; a non-vacuity witness).
    let chain = impacted_exec_transcribed(&[0u32], &[(1, 0), (2, 1), (3, 2)]);
    let chain_ok = chain == [true, true, true, true, false, false, false, false];
    println!("chain-contract: {}", if chain_ok { "match" } else { "MISMATCH {chain:?}" });
    if !chain_ok {
        failures += 1;
    }

    let imp = impacted_reference(&[0u32], &[(1, 0), (2, 1), (3, 2)]);
    let affected_fixtures: Vec<(&str, Vec<bool>, Vec<(u32, u32)>, Vec<u32>, Vec<u32>, Vec<(u32, u32)>)> = vec![
        ("aff-empty", vec![false; CU_CAP], vec![], vec![], vec![], vec![]),
        ("aff-always-run", vec![false; CU_CAP], vec![], vec![2, 5], vec![], vec![]),
        (
            "aff-impacted-deps",
            imp.clone(),
            vec![(0, 1), (4, 3), (6, 2)],
            vec![],
            vec![],
            vec![],
        ),
        (
            "aff-fallback",
            vec![false; CU_CAP],
            vec![],
            vec![],
            vec![11],
            vec![(1, 11), (7, 0)],
        ),
        (
            "aff-mixed",
            imp.clone(),
            vec![(5, 2)],
            vec![0],
            vec![4],
            vec![(2, 4), (3, 9)],
        ),
    ];
    for (name, imp, td, ar, cf, tc) in &affected_fixtures {
        let got = affected_exec_transcribed(imp, td, ar, cf, tc);
        let want = affected_reference(imp, td, ar, cf, tc);
        let ok = got == want;
        println!("{name}: {}", if ok { "match" } else { "MISMATCH got={got:?} want={want:?}" });
        if !ok {
            failures += 1;
        }
    }

    // composition: impacted feeding affected end to end (non-vacuity:
    // the dep marking must fire on the chain closure).
    let vis = impacted_exec_transcribed(&[0u32], &[(1, 0), (2, 1), (3, 2)]);
    let got = affected_exec_transcribed(&vis, &[(0, 1), (4, 3)], &[], &[], &[]);
    let want = affected_reference(&impacted_reference(&[0u32], &[(1, 0), (2, 1), (3, 2)]), &[(0, 1), (4, 3)], &[], &[], &[]);
    let comp_ok = got == want;
    println!("composition: {} got={got:?} want={want:?}", if comp_ok { "match" } else { "MISMATCH" });
    if !comp_ok {
        failures += 1;
    }

    if failures == 0 {
        println!("PARITY PASS: 14 fixture groups, 0 mismatches");
    } else {
        println!("PARITY FAIL: {failures} mismatching fixture groups");
        std::process::exit(1);
    }
}

// Verus functional-proof probe seed (beads `testaruda-rqvc`).
//
// Purpose: prove the selection algorithm's Boolean core — impacted
// (reverse reachability) and affected — panic-free with a full
// functional spec, as a bounded model over dense ids. This is the
// family the Kani probe (testaruda-jdpi) wrote harnesses for but could
// not discharge: no symbolic inputs here, per-function contracts
// discharged by Z3.
//
// Model contract (the verified fragment):
// - content-unit ids and test ids are both dense and bounded
//   (`CU_CAP` / `T_CAP`); component keys in fixtures are remapped into
//   a bounded domain by the caller. Out-of-domain input is outside the
//   fragment (`requires`) — the parity driver names that boundary.
// - `impacted` = the CU_CAP-step closure of the seed set
//   (changed ∪ unresolved) under cu-dep edges; over ≤ CU_CAP nodes the
//   k-step closure is the mathematical closure (simple chains have
//   ≤ CU_CAP − 1 edges).
// - `affected` = always-run tests ∪ tests on an impacted unit ∪ tests
//   in a fallback component.
//
// Exec strategy: **snapshot relaxation** — each pass reads only the
// pre-pass visited vector and writes a fresh one, so pass k's result is
// exactly the k-step closure. That makes `exec == spec` a direct
// induction with no cycle-shortening or cardinality lemmas (the
// in-place worklist variants are where allocation-shape proofs
// explode — the lesson from the Kani probe is applied here).
//
// Proof idioms (this Verus release, per the vendored guide corpus and
// probe iteration):
// - loop state is characterized FUNCTIONALLY
//   (`v@ =~= Seq::new(n, |k| …)`); plain `==` on Seq is not
//   extensional, `=~=` is.
// - quantifier triggers must be DIRECT Seq indices (`deps@[i]`);
//   spec-fn-wrapped terms (`edge_at(deps@, i)`) do not fire.
// - requires do not reach loop bodies under loop isolation — every
//   fact a loop needs is restated as an invariant.
// - ghost types stay inside `spec fn`s; the exec layer is plain Vec.
use vstd::prelude::*;

verus! {

/// Dense content-unit id domain: every cu id in a model input is < CU_CAP.
pub const CU_CAP: usize = 8;
/// Dense test id domain: every test id in a model input is < T_CAP.
pub const T_CAP: usize = 8;
/// Dense component-key domain (fixture components are remapped into it).
pub const COMP_CAP: usize = 8;

/// One closure step for node `a` over a snapshot vector (impact flows
/// b -> a along edge (a, b): `a` depends on `b`).
pub open spec fn step_membership(prev: Seq<bool>, deps: Seq<(u32, u32)>, a: usize) -> bool {
    prev[a as int]
        || exists|j: int| #![trigger deps[j]]
            0 <= j < deps.len() && deps[j].0 == a as u32 && prev[deps[j].1 as int]
}

/// The k-step closure of a seed vector under `deps`, as a vector.
pub open spec fn closure_v(seeds: Seq<bool>, deps: Seq<(u32, u32)>, k: usize) -> Seq<bool>
    decreases k,
{
    if k == 0 {
        seeds
    } else {
        let prev = closure_v(seeds, deps, (k - 1) as usize);
        Seq::new(CU_CAP as nat, |a: int| step_membership(prev, deps, a as usize))
    }
}

/// The seed set (changed ∪ unresolved) as a vector over the cu id domain.
pub open spec fn seed_vec(seeds: Seq<u32>) -> Seq<bool> {
    Seq::new(CU_CAP as nat, |a: int| seeds.contains(a as u32))
}

/// The Boolean selection semantics: which cu ids are impacted.
pub open spec fn impacted_at(c: u32, seeds: Seq<u32>, deps: Seq<(u32, u32)>) -> bool {
    closure_v(seed_vec(seeds), deps, CU_CAP)[c as int]
}

/// Snapshot-relaxation reachability: the seed set closed under the cu
/// dep edges. Each pass reads only the pre-pass visited vector, so
/// after pass k the visited vector is exactly the k-step closure.
#[verifier::loop_isolation(false)]
pub fn impacted_exec(seeds: &Vec<u32>, deps: &Vec<(u32, u32)>) -> (visited: Vec<bool>)
    requires
        forall|c: u32| #![trigger seeds@.contains(c)]
            seeds@.contains(c) ==> (c as usize) < CU_CAP,
        forall|i: int| #![trigger deps@[i]]
            0 <= i < deps@.len() ==> deps@[i].0 < CU_CAP && deps@[i].1 < CU_CAP,
    ensures
        visited.len() == CU_CAP,
        visited@ =~= closure_v(seed_vec(seeds@), deps@, CU_CAP),
{
    let mut visited: Vec<bool> = Vec::new();
    let mut c: usize = 0;
    while c < CU_CAP
        invariant
            visited.len() == c,
            c <= CU_CAP,
            visited@ =~= Seq::new(c as nat, |k: int| seeds@.contains(k as u32)),
        decreases CU_CAP - c,
    {
        visited.push(seeds.contains(&(c as u32)));
        c += 1;
    }
    proof {
        assert(visited@ =~= seed_vec(seeds@));
    }
    let mut pass: usize = 0;
    while pass < CU_CAP
        invariant
            visited.len() == CU_CAP,
            pass <= CU_CAP,
            visited@ =~= closure_v(seed_vec(seeds@), deps@, pass),
        decreases CU_CAP - pass,
    {
        let mut next = visited.clone();
        let mut a: usize = 0;
        while a < CU_CAP
            invariant
                next.len() == CU_CAP,
                visited.len() == CU_CAP,
                a <= CU_CAP,
                next@ =~= Seq::new(
                    CU_CAP as nat,
                    |k: int|
                        if (k as usize) < a {
                            step_membership(visited@, deps@, k as usize)
                        } else {
                            visited@[k]
                        },
                ),
            decreases CU_CAP - a,
        {
            let mut hit = false;
            if visited[a] {
                hit = true;
            }
            let mut j: usize = 0;
            while j < deps.len()
                invariant
                    j <= deps.len(),
                    a < CU_CAP,
                    visited.len() == CU_CAP,
                    forall|i: int| #![trigger deps@[i]]
                        0 <= i < deps.len() ==> deps@[i].0 < CU_CAP && deps@[i].1 < CU_CAP,
                    hit == (visited@[a as int]
                        || exists|p: int| #![trigger deps@[p]]
                            0 <= p < j && deps@[p].0 == a as u32
                                && visited@[deps@[p].1 as int]),
                decreases deps.len() - j,
            {
                proof {
                    assert(deps@[j as int].0 < CU_CAP);
                    assert(deps@[j as int].1 < CU_CAP);
                }
                let (u, b) = deps[j];
                if u == a as u32 && visited[b as usize] {
                    hit = true;
                }
                j += 1;
            }
            proof {
                assert(hit == step_membership(visited@, deps@, a));
            }
            next.set(a, hit);
            a += 1;
        }
        proof {
            assert(next@ =~= Seq::new(
                CU_CAP as nat,
                |k: int| step_membership(visited@, deps@, k as usize),
            ));
            assert(visited@ =~= closure_v(seed_vec(seeds@), deps@, pass));
            assert(closure_v(seed_vec(seeds@), deps@, (pass + 1usize) as usize)
                =~= Seq::new(CU_CAP as nat, |a: int|
                    step_membership(closure_v(seed_vec(seeds@), deps@, pass), deps@, a as usize)));
            assert(next@ =~= closure_v(seed_vec(seeds@), deps@, (pass + 1usize) as usize));
        }
        visited = next;
        pass += 1;
    }
    visited
}

/// Membership of a test id in the affected vector, stated at Seq level
/// (exists-over-index — avoids `Seq::contains`, which needs index_of
/// lemmas across `push` that are not axioms in this release).
pub open spec fn member_s(s: Seq<u32>, t: u32) -> bool {
    exists|j: int| 0 <= j < s.len() && s[j] == t
}

/// Snapshot-marking affectedness: always-run, impacted deps, fallback
/// components. Returns the affected test ids in ascending order.
#[verifier::loop_isolation(false)]
pub fn affected_exec(
    impacted: &Vec<bool>,
    test_deps: &Vec<(u32, u32)>,
    always_run: &Vec<u32>,
    comp_fallback: &Vec<u32>,
    test_comp: &Vec<(u32, u32)>,
) -> (affected: Vec<u32>)
    requires
        impacted.len() == CU_CAP,
        forall|i: int| #![trigger test_deps@[i]]
            0 <= i < test_deps@.len() ==> test_deps@[i].0 < T_CAP
                && test_deps@[i].1 < CU_CAP,
        forall|c: u32| #![trigger always_run@.contains(c)]
            always_run@.contains(c) ==> (c as usize) < T_CAP,
        forall|k: u32| #![trigger comp_fallback@.contains(k)]
            comp_fallback@.contains(k) ==> (k as usize) < COMP_CAP,
        forall|i: int| #![trigger test_comp@[i]]
            0 <= i < test_comp@.len() ==> test_comp@[i].0 < T_CAP
                && test_comp@[i].1 < COMP_CAP,
    ensures
        affected.len() <= T_CAP,
        forall|t: u32| #![trigger member_s(affected@, t)]
            member_s(affected@, t)
                == ((t as usize) < T_CAP
                    && (always_run@.contains(t)
                        || exists|i: int| #![trigger test_deps@[i]]
                            0 <= i < test_deps@.len() && test_deps@[i].0 == t
                                && impacted@[test_deps@[i].1 as int]
                        || exists|i: int| #![trigger test_comp@[i]]
                            0 <= i < test_comp@.len() && test_comp@[i].0 == t
                                && comp_fallback@.contains(test_comp@[i].1))),
        forall|j: int, k: int|
            0 <= j < k && k < affected.len() ==> affected@[j] < affected@[k],
{
    let mut aff: Vec<bool> = Vec::new();
    let mut t: usize = 0;
    while t < T_CAP
        invariant
            aff.len() == t,
            t <= T_CAP,
            forall|k: usize|
                k < t ==> aff@[k as int] == always_run@.contains(k as u32),
        decreases T_CAP - t,
    {
        aff.push(always_run.contains(&(t as u32)));
        t += 1;
    }
    let mut i: usize = 0;
    while i < test_deps.len()
        invariant
            aff.len() == T_CAP,
            i <= test_deps.len(),
            impacted.len() == CU_CAP,
            forall|i2: int| #![trigger test_deps@[i2]]
                0 <= i2 < test_deps.len() ==> test_deps@[i2].0 < T_CAP
                    && test_deps@[i2].1 < CU_CAP,
            forall|k: usize|
                k < T_CAP ==> aff@[k as int]
                    == (always_run@.contains(k as u32)
                        || exists|p: int| #![trigger test_deps@[p]]
                            0 <= p < i && test_deps@[p].0 == k as u32
                                && impacted@[test_deps@[p].1 as int]),
        decreases test_deps.len() - i,
    {
        proof {
            assert(test_deps@[i as int].0 < T_CAP);
            assert(test_deps@[i as int].1 < CU_CAP);
        }
        let (td, tc) = test_deps[i];
        if impacted[tc as usize] {
            aff.set(td as usize, true);
        }
        i += 1;
    }
    let mut i: usize = 0;
    while i < test_comp.len()
        invariant
            aff.len() == T_CAP,
            i <= test_comp.len(),
            impacted.len() == CU_CAP,
            forall|i2: int| #![trigger test_deps@[i2]]
                0 <= i2 < test_deps.len() ==> test_deps@[i2].0 < T_CAP
                    && test_deps@[i2].1 < CU_CAP,
            forall|i2: int| #![trigger test_comp@[i2]]
                0 <= i2 < test_comp.len() ==> test_comp@[i2].0 < T_CAP
                    && test_comp@[i2].1 < COMP_CAP,
            forall|k: usize|
                k < T_CAP ==> aff@[k as int]
                    == (always_run@.contains(k as u32)
                        || exists|p: int| #![trigger test_deps@[p]]
                            0 <= p < test_deps.len() && test_deps@[p].0 == k as u32
                                && impacted@[test_deps@[p].1 as int]
                        || exists|p: int| #![trigger test_comp@[p]]
                            0 <= p < i && test_comp@[p].0 == k as u32
                                && comp_fallback@.contains(test_comp@[p].1)),
        decreases test_comp.len() - i,
    {
        proof {
            assert(test_comp@[i as int].0 < T_CAP);
            assert(test_comp@[i as int].1 < COMP_CAP);
        }
        let (td, ck) = test_comp[i];
        if comp_fallback.contains(&ck) {
            aff.set(td as usize, true);
        }
        i += 1;
    }
    let mut affected: Vec<u32> = Vec::new();
    let mut t: usize = 0;
    while t < T_CAP
        invariant
            affected.len() <= t,
            t <= T_CAP,
            aff.len() == T_CAP,
            forall|j: int, p: int|
                0 <= j < p && p < affected.len() ==> affected@[j] < affected@[p],
            forall|j: int| 0 <= j < affected.len() ==> (affected@[j] as usize) < t,
            forall|tt: usize| #![trigger member_s(affected@, tt as u32)]
                tt < t ==> member_s(affected@, tt as u32) == aff@[tt as int],
        decreases T_CAP - t,
    {
        let ghost pre_seq = affected@;
        let ghost pre_len = affected.len();
        let ghost pre_t = t;
        let aff_t: bool = aff[t];
        if aff_t {
            affected.push(t as u32);
        }
        proof {
            if aff_t {
                assert(affected.len() == pre_len + 1);
                assert(affected@[pre_len as int] == pre_t as u32);
                assert forall|j: int| 0 <= j < pre_len implies affected@[j] == pre_seq[j] by {
                    if 0 <= j < pre_len { }
                }
            } else {
                assert(affected@ == pre_seq);
            }
            assert forall|tt: usize| tt <= pre_t
                implies member_s(affected@, tt as u32) == aff@[tt as int]
            by {
                if tt < pre_t {
                    assert(member_s(pre_seq, tt as u32) == aff@[tt as int]);
                    assert(member_s(affected@, tt as u32) == member_s(pre_seq, tt as u32));
                } else {
                    if aff_t {
                        assert(affected@[pre_len as int] == pre_t as u32);
                        assert(member_s(affected@, tt as u32));
                    } else {
                        assert(!member_s(affected@, tt as u32));
                    }
                }
            }
        }
        t += 1;
    }
    proof {
        assert forall|t: u32|
            member_s(affected@, t)
                == ((t as usize) < T_CAP
                    && (always_run@.contains(t)
                        || exists|i: int| #![trigger test_deps@[i]]
                            0 <= i < test_deps@.len() && test_deps@[i].0 == t
                                && impacted@[test_deps@[i].1 as int]
                        || exists|i: int| #![trigger test_comp@[i]]
                            0 <= i < test_comp@.len() && test_comp@[i].0 == t
                                && comp_fallback@.contains(test_comp@[i].1)))
        by {
            if (t as usize) < T_CAP {
                assert(member_s(affected@, t) == aff@[t as usize as int]);
                assert(aff@[t as usize as int]
                    == (always_run@.contains(t)
                        || exists|i: int| #![trigger test_deps@[i]]
                            0 <= i < test_deps.len() && test_deps@[i].0 == t
                                && impacted@[test_deps@[i].1 as int]
                        || exists|i: int| #![trigger test_comp@[i]]
                            0 <= i < test_comp.len() && test_comp@[i].0 == t
                                && comp_fallback@.contains(test_comp@[i].1)));
            } else {
                assert forall|j: int| 0 <= j < affected.len() implies affected@[j] != t by {
                    if 0 <= j < affected.len() {
                        assert((affected@[j] as usize) < T_CAP);
                    }
                }
                assert(!member_s(affected@, t));
            }
        }
    }
    affected
}

} // verus!

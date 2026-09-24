//! Threshold OPRF for the uniqueness label (`docs/03` §M1).
//!
//! The single-server [`crate::enrollment::VoprfOracle`] is a real oblivious PRF, but
//! one party holds the whole key and could brute-force the small codice-fiscale space
//! by evaluating the PRF on candidates. The spec's answer is a **threshold** OPRF: the
//! key is split t-of-n across the committee, so no sub-threshold coalition can evaluate
//! it. This module implements that.
//!
//! Construction: a threshold DH-OPRF over Ristretto255. The key `k` is Shamir-shared
//! (`f(0) = k`, share `k_i = f(i)`, public commitment `Y_i = k_i·G`). To label an
//! anchor `x`: the client blinds `H(x)` to `B = r·H(x)`; each committee member returns
//! `Z_i = k_i·B` with a Chaum–Pedersen **DLEQ proof** that the same `k_i` sits behind
//! its public `Y_i`; the client verifies each proof, Lagrange-combines any `t` of them
//! to `Z = k·B`, unblinds `W = r⁻¹·Z = k·H(x)`, and hashes `W` into the label. `W` is
//! independent of the blind and of which `t` members answered, so the label is stable.
//!
//! **What is real:** the threshold cryptography — Shamir sharing, per-share DLEQ
//! verifiability, Lagrange reconstruction — so `t` members can label an anchor, `t-1`
//! cannot, and a member cannot cheat with a share that disagrees with its public
//! commitment. It is a bespoke construction on the vetted `curve25519-dalek` group
//! (allowed as a tested exception, see `docs/CLAUDE.md`); it is not wire-compatible
//! with the RFC 9497 `VoprfOracle`.
//!
//! **What is still modeled (future work):** a real distributed key generation ceremony
//! (here a trusted dealer derives the shares in one process), network transport between
//! members, and proactive share refresh. The committee is run in-process.

use crate::enrollment::{Anchor, Label, UniquenessOracle};
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT as G;
use curve25519_dalek::ristretto::RistrettoPoint;
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256, Sha512};

const H2G_DST: &[u8] = b"isegoria/oprf/hash-to-group/v1";
const DLEQ_DST: &[u8] = b"isegoria/oprf/dleq-challenge/v1";
const OUT_DST: &[u8] = b"isegoria/oprf/output/v1";
const KEYGEN_DST: &[u8] = b"isegoria/oprf/keygen/v1";

fn random_scalar() -> Scalar {
    let mut b = [0u8; 64];
    OsRng.fill_bytes(&mut b);
    Scalar::from_bytes_mod_order_wide(&b)
}

/// Domain-separated hash into the Ristretto255 group (two-map, via SHA-512).
fn hash_to_group(input: &[u8]) -> RistrettoPoint {
    let mut h = Sha512::new();
    h.update((H2G_DST.len() as u64).to_le_bytes());
    h.update(H2G_DST);
    h.update(input);
    RistrettoPoint::from_hash(h)
}

fn scalar_from_seed(seed: &[u8; 32], coeff_index: usize) -> Scalar {
    let mut h = Sha512::new();
    h.update((KEYGEN_DST.len() as u64).to_le_bytes());
    h.update(KEYGEN_DST);
    h.update(seed);
    h.update((coeff_index as u64).to_le_bytes());
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&h.finalize());
    Scalar::from_bytes_mod_order_wide(&wide)
}

/// Fiat–Shamir challenge over the DLEQ transcript.
fn dleq_challenge(points: [&RistrettoPoint; 6]) -> Scalar {
    let mut h = Sha512::new();
    h.update((DLEQ_DST.len() as u64).to_le_bytes());
    h.update(DLEQ_DST);
    for p in points {
        h.update(p.compress().as_bytes());
    }
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&h.finalize());
    Scalar::from_bytes_mod_order_wide(&wide)
}

/// A committee member's secret Shamir share of the OPRF key.
#[derive(Clone)]
pub struct KeyShare {
    index: u32,
    secret: Scalar,
}

/// The public commitment `Y_i = k_i·G` to a member's share.
#[derive(Clone, Copy)]
pub struct PublicShare {
    index: u32,
    commitment: RistrettoPoint,
}

/// A Chaum–Pedersen proof that `log_G(Y_i) == log_B(Z_i)` (same share behind both).
#[derive(Clone)]
pub struct DleqProof {
    challenge: Scalar,
    response: Scalar,
}

/// One member's partial evaluation on the blinded point, with its DLEQ proof.
#[derive(Clone)]
pub struct PartialEval {
    index: u32,
    value: RistrettoPoint,
    proof: DleqProof,
}

impl KeyShare {
    fn public(&self) -> PublicShare {
        PublicShare {
            index: self.index,
            commitment: self.secret * G,
        }
    }

    /// Evaluate on a blinded point and prove the evaluation used this share's key.
    fn evaluate(&self, blinded: &RistrettoPoint) -> PartialEval {
        let value = self.secret * blinded; // Z_i = k_i·B
        let y = self.secret * G; // Y_i = k_i·G
        let nonce = random_scalar();
        let a1 = nonce * G; // A1 = s·G
        let a2 = nonce * blinded; // A2 = s·B
        let challenge = dleq_challenge([&G, &y, blinded, &value, &a1, &a2]);
        let response = nonce + challenge * self.secret; // z = s + c·k_i
        PartialEval {
            index: self.index,
            value,
            proof: DleqProof {
                challenge,
                response,
            },
        }
    }
}

/// Verify a member's DLEQ proof against its public share and the blinded point.
fn verify_partial(public: &PublicShare, blinded: &RistrettoPoint, part: &PartialEval) -> bool {
    if public.index != part.index {
        return false;
    }
    // Recover the prover's commitments: A1 = z·G − c·Y, A2 = z·B − c·Z.
    let a1 = part.proof.response * G - part.proof.challenge * public.commitment;
    let a2 = part.proof.response * blinded - part.proof.challenge * part.value;
    let c = dleq_challenge([&G, &public.commitment, blinded, &part.value, &a1, &a2]);
    c == part.proof.challenge
}

/// Lagrange coefficient at 0 for interpolation over `indices`, for member `i`.
fn lagrange_at_zero(indices: &[u32], i: u32) -> Scalar {
    let xi = Scalar::from(i as u64);
    let mut num = Scalar::ONE;
    let mut den = Scalar::ONE;
    for &j in indices {
        if j == i {
            continue;
        }
        let xj = Scalar::from(j as u64);
        num *= -xj; // (0 − x_j)
        den *= xi - xj; // (x_i − x_j)
    }
    num * den.invert()
}

/// Combine `t` verified partials into `Z = k·B` by Lagrange interpolation at 0.
fn combine(parts: &[PartialEval]) -> RistrettoPoint {
    let indices: Vec<u32> = parts.iter().map(|p| p.index).collect();
    let mut acc = RistrettoPoint::identity();
    for p in parts {
        acc += lagrange_at_zero(&indices, p.index) * p.value;
    }
    acc
}

/// Domain-separated finalization: hash the unblinded `W = k·H(x)` into a 32-byte label.
fn finalize(input: &[u8], w: &RistrettoPoint) -> Label {
    let mut h = Sha256::new();
    h.update((OUT_DST.len() as u64).to_le_bytes());
    h.update(OUT_DST);
    h.update((input.len() as u64).to_le_bytes());
    h.update(input);
    h.update(w.compress().as_bytes());
    Label(h.finalize().into())
}

/// A threshold OPRF committee run in-process: `t`-of-`n` members hold key shares and
/// jointly compute the uniqueness label. Implements [`UniquenessOracle`].
pub struct ThresholdOprfOracle {
    shares: Vec<KeyShare>,
    public_shares: Vec<PublicShare>,
    threshold: usize,
}

impl ThresholdOprfOracle {
    /// Trusted-dealer key generation (real DKG is future work): derive a degree-`t−1`
    /// polynomial from `seed` and hand share `f(i)` to member `i` (1-based). The same
    /// seed rebuilds the same committee, so labels are reproducible across processes.
    ///
    /// # Panics
    ///
    /// Unless `1 <= t <= n`: the committee's shape is operator configuration.
    pub fn new(seed: [u8; 32], n: usize, t: usize) -> Self {
        assert!(t >= 1 && t <= n, "need 1 <= t <= n");
        let coeffs: Vec<Scalar> = (0..t).map(|j| scalar_from_seed(&seed, j)).collect();
        let shares: Vec<KeyShare> = (1..=n as u32)
            .map(|i| {
                let x = Scalar::from(i as u64);
                let mut acc = Scalar::ZERO;
                let mut power = Scalar::ONE;
                for c in &coeffs {
                    acc += c * power; // f(i) = Σ c_j · i^j
                    power *= x;
                }
                KeyShare {
                    index: i,
                    secret: acc,
                }
            })
            .collect();
        let public_shares = shares.iter().map(KeyShare::public).collect();
        ThresholdOprfOracle {
            shares,
            public_shares,
            threshold: t,
        }
    }

    /// Run the protocol with an explicit quorum of member indices (1-based). Returns
    /// the label, or `None` if the quorum is too small or a member's proof fails.
    fn label_with_quorum(&self, input: &[u8], quorum: &[u32]) -> Option<Label> {
        if quorum.len() < self.threshold {
            return None;
        }
        // Lagrange interpolation requires distinct x-coordinates. A duplicate index
        // double-counts one share and, via `Scalar::invert(0) = 0` in the `x_i − x_j`
        // denominator, silently yields a wrong label; reject it instead.
        let mut distinct = quorum.to_vec();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() != quorum.len() {
            return None;
        }
        let h = hash_to_group(input);
        let r = random_scalar();
        let blinded = r * h; // B = r·H(x)

        let mut parts = Vec::with_capacity(quorum.len());
        for &idx in quorum {
            let share = self.shares.iter().find(|s| s.index == idx)?;
            let public = self.public_shares.iter().find(|p| p.index == idx)?;
            let part = share.evaluate(&blinded);
            if !verify_partial(public, &blinded, &part) {
                return None;
            }
            parts.push(part);
        }
        let z = combine(&parts); // Z = k·B
        let w = r.invert() * z; // W = k·H(x)
        Some(finalize(input, &w))
    }
}

/// Fuzzing entry point (T44): the quorum-level protocol is private, but which members
/// answered is exactly the input a client cannot trust.
#[cfg(fuzzing)]
impl ThresholdOprfOracle {
    #[doc(hidden)]
    pub fn fuzz_label_with_quorum(&self, input: &[u8], quorum: &[u32]) -> Option<Label> {
        self.label_with_quorum(input, quorum)
    }
}

impl UniquenessOracle for ThresholdOprfOracle {
    fn label(&self, anchor: &Anchor) -> Label {
        let quorum: Vec<u32> = self.shares[..self.threshold]
            .iter()
            .map(|s| s.index)
            .collect();
        self.label_with_quorum(anchor.0.as_bytes(), &quorum)
            .expect("the first t honest members always produce a label")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(s: &str) -> Anchor {
        Anchor(s.to_string())
    }

    #[test]
    fn label_is_deterministic() {
        let oracle = ThresholdOprfOracle::new([1u8; 32], 5, 3);
        let a = anchor("RSSMRA80A01H501U");
        assert_eq!(oracle.label(&a), oracle.label(&a));
    }

    #[test]
    fn any_t_of_n_subset_yields_the_same_label() {
        // The label depends only on the key, not on which quorum answered.
        let oracle = ThresholdOprfOracle::new([2u8; 32], 5, 3);
        let input = b"VRDLGI85M02F205Z";
        let l1 = oracle.label_with_quorum(input, &[1, 2, 3]).unwrap();
        let l2 = oracle.label_with_quorum(input, &[3, 4, 5]).unwrap();
        let l3 = oracle.label_with_quorum(input, &[1, 3, 5]).unwrap();
        assert_eq!(l1, l2);
        assert_eq!(l1, l3);
    }

    #[test]
    fn fewer_than_t_members_cannot_reconstruct_the_label() {
        let oracle = ThresholdOprfOracle::new([3u8; 32], 5, 3);
        let input = b"RSSMRA80A01H501U";
        let correct = oracle.label(&anchor("RSSMRA80A01H501U"));

        // A sub-threshold quorum is refused outright.
        assert!(oracle.label_with_quorum(input, &[1, 2]).is_none());

        // And interpolating 2 shares as if the threshold were 2 gives a *different*
        // value: the missing share means the recovered secret is not k.
        let two = ThresholdOprfOracle {
            shares: oracle.shares[..2].to_vec(),
            public_shares: oracle.public_shares[..2].to_vec(),
            threshold: 2,
        };
        assert_ne!(two.label_with_quorum(input, &[1, 2]).unwrap(), correct);
    }

    #[test]
    fn a_quorum_with_duplicate_indices_is_refused() {
        // ID-003(ii) / AT-ID-04: Lagrange interpolation needs distinct x-coordinates.
        // A repeated index double-counts a share and (via `Scalar::invert(0) = 0`)
        // would otherwise return a wrong label silently rather than error.
        let oracle = ThresholdOprfOracle::new([7u8; 32], 5, 3);
        let input = b"RSSMRA80A01H501U";
        assert!(oracle.label_with_quorum(input, &[1, 1, 2]).is_none());
        assert!(oracle.label_with_quorum(input, &[2, 2, 2]).is_none());
        // A distinct quorum of the same size still works.
        assert!(oracle.label_with_quorum(input, &[1, 2, 3]).is_some());
    }

    #[test]
    fn a_member_lying_about_its_share_is_caught() {
        // A partial computed with a key that disagrees with the public commitment
        // fails the DLEQ check.
        let oracle = ThresholdOprfOracle::new([4u8; 32], 5, 3);
        let blinded = random_scalar() * hash_to_group(b"x");

        let honest = &oracle.shares[0];
        let good = honest.evaluate(&blinded);
        assert!(verify_partial(&oracle.public_shares[0], &blinded, &good));

        let liar = KeyShare {
            index: honest.index,
            secret: honest.secret + Scalar::ONE,
        };
        let bad = liar.evaluate(&blinded);
        // The liar's own proof is internally consistent, but it does not match the
        // committee's published commitment for that member.
        assert!(!verify_partial(&oracle.public_shares[0], &blinded, &bad));
    }

    #[test]
    fn different_committees_give_different_labels() {
        let a = ThresholdOprfOracle::new([1u8; 32], 5, 3);
        let b = ThresholdOprfOracle::new([2u8; 32], 5, 3);
        assert_ne!(a.label(&anchor("same")), b.label(&anchor("same")));
    }
}

#[cfg(test)]
mod proptests {
    //! Property tests (T42) over arbitrary committees, quorums and anchors. In the crate
    //! because the quorum-level entry point is not part of the public API.
    use super::*;
    use proptest::prelude::*;

    /// A committee shape `(n, t)` with `1 <= t <= n <= 7`, and a seed.
    fn committee() -> impl Strategy<Value = ([u8; 32], usize, usize)> {
        (1usize..=7).prop_flat_map(|n| (any::<[u8; 32]>(), Just(n), 1..=n))
    }

    /// A quorum of `size` distinct member indices drawn from `1..=n`, in any order.
    fn quorum(n: usize, size: usize) -> impl Strategy<Value = Vec<u32>> {
        Just((1..=n as u32).collect::<Vec<u32>>())
            .prop_shuffle()
            .prop_map(move |all| all[..size].to_vec())
    }

    /// Two independent quorums of at least `t` members for one committee, and an anchor.
    fn two_quorums() -> impl Strategy<Value = ([u8; 32], usize, usize, Vec<u32>, Vec<u32>)> {
        committee().prop_flat_map(|(seed, n, t)| {
            (
                Just(seed),
                Just(n),
                Just(t),
                (t..=n).prop_flat_map(move |s| quorum(n, s)),
                (t..=n).prop_flat_map(move |s| quorum(n, s)),
            )
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        /// Any quorum of at least `t` distinct members, in any order, reconstructs the
        /// same label — the one the oracle hands out.
        #[test]
        fn any_quorum_reconstructs_the_same_label(
            (seed, n, t, q1, q2) in two_quorums(),
            input in prop::collection::vec(any::<u8>(), 0..32),
        ) {
            let oracle = ThresholdOprfOracle::new(seed, n, t);
            let l1 = oracle.label_with_quorum(&input, &q1);
            let l2 = oracle.label_with_quorum(&input, &q2);
            prop_assert!(l1.is_some(), "quorum {q1:?} of a {t}-of-{n} committee refused");
            prop_assert_eq!(l1, l2);
        }

        /// A quorum repeating any member is refused, whatever its size and order.
        #[test]
        fn a_quorum_with_a_duplicate_index_is_refused(
            (seed, n, t, q, _) in two_quorums(),
            dup in any::<prop::sample::Index>(),
            at in any::<prop::sample::Index>(),
        ) {
            let oracle = ThresholdOprfOracle::new(seed, n, t);
            let mut with_dup = q.clone();
            let repeated = q[dup.index(q.len())];
            with_dup.insert(at.index(q.len() + 1), repeated);
            prop_assert!(oracle.label_with_quorum(b"anchor", &with_dup).is_none());
        }

        /// Fewer than `t` distinct members, or any index outside the committee, is
        /// refused.
        #[test]
        fn a_short_or_foreign_quorum_is_refused(
            (seed, n, t, q, _) in two_quorums(),
            outsider in prop_oneof![Just(0u32), 8u32..],
            at in any::<prop::sample::Index>(),
        ) {
            let oracle = ThresholdOprfOracle::new(seed, n, t);
            prop_assert!(oracle.label_with_quorum(b"anchor", &q[..t - 1]).is_none());
            let mut foreign = q.clone();
            let i = at.index(q.len());
            foreign[i] = outsider;
            prop_assert!(oracle.label_with_quorum(b"anchor", &foreign).is_none());
        }

        /// Distinct anchors get distinct labels under one committee.
        #[test]
        fn distinct_anchors_get_distinct_labels(
            (seed, n, t) in committee(),
            a in prop::collection::vec(any::<u8>(), 0..32),
            b in prop::collection::vec(any::<u8>(), 0..32),
        ) {
            prop_assume!(a != b);
            let oracle = ThresholdOprfOracle::new(seed, n, t);
            let quorum: Vec<u32> = (1..=t as u32).collect();
            prop_assert_ne!(
                oracle.label_with_quorum(&a, &quorum),
                oracle.label_with_quorum(&b, &quorum)
            );
        }

        /// A partial evaluation whose value is shifted by any non-identity point fails
        /// its DLEQ proof, so a member cannot bias the combined value.
        #[test]
        fn a_shifted_partial_fails_its_proof(
            seed in any::<[u8; 32]>(),
            shift in any::<[u8; 64]>(),
            member in 0usize..5,
        ) {
            let delta = RistrettoPoint::from_uniform_bytes(&shift);
            prop_assume!(delta != RistrettoPoint::identity());
            let oracle = ThresholdOprfOracle::new(seed, 5, 3);
            let blinded = random_scalar() * hash_to_group(b"anchor");
            let mut part = oracle.shares[member].evaluate(&blinded);
            prop_assert!(verify_partial(&oracle.public_shares[member], &blinded, &part));
            part.value += delta;
            prop_assert!(!verify_partial(&oracle.public_shares[member], &blinded, &part));
        }
    }
}

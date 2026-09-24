//! Semaphore-style zero-knowledge nullifier bound to the credential (`docs/03` §M3).
//!
//! Every action carries a per-role nullifier and a proof that it derives from a valid,
//! committee-issued credential — without revealing the secret or which credential. The
//! standard Semaphore stack (a circom/Groth16 membership circuit) is impractical here
//! (a C++ toolchain, bundled proving keys, MSRV conflicts); this achieves the same
//! guarantees with sigma-protocols over the BBS+ credential we already issue.
//!
//! Construction. Let `x` be the credential's secret (a BLS12-381 scalar) and
//! `H_role = HashToG1(role)` a per-role generator with unknown discrete log. The
//! nullifier is `N = x·H_role`. The proof is a BBS+ proof of knowledge of the
//! signature over `x` (revealing nothing) whose blinding for the message `x` is reused
//! as the Schnorr blinding for `N = x·H_role`; a single Fiat–Shamir challenge binds the
//! two, so the `x` inside the credential is the same `x` inside the nullifier.
//!
//! Properties: `N` is deterministic in `(x, role)` (a person acting twice in one role
//! is detectable), reveals nothing about `x` (discrete log), differs across roles and
//! is unlinkable across them (DDH), and is proven to come from a real credential.
//!
//! **What is real:** the ZK nullifier and its binding to the BBS+ credential — a
//! bespoke sigma-protocol composition on vetted primitives (`bbs_plus` PoK + arkworks
//! group), allowed as a tested exception (see `docs/CLAUDE.md`). **Still modeled /
//! separate:** [`crate::nym::derive_nym`] remains the lightweight deterministic
//! address the protocol layer uses; unifying the two (the protocol keying off `N`) and
//! selective-disclosure of the label are future work.

use crate::credential::{AnonymousCredential, IssuerPublic};
use crate::hash::tagged;
use crate::nym::{Nym, Role};

use ark_bls12_381::g1::Config as G1Config;
use ark_bls12_381::{Bls12_381, Fr, G1Affine, G1Projective};
use ark_ec::hashing::curve_maps::wb::WBMap;
use ark_ec::hashing::map_to_curve_hasher::MapToCurveBasedHasher;
use ark_ec::hashing::HashToCurve;
use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::field_hashers::DefaultFieldHasher;
use ark_serialize::CanonicalSerialize;
use ark_std::UniformRand;
use bbs_plus::prelude::{BBSPlusError, PoKOfSignatureG1Proof, PoKOfSignatureG1Protocol};
use dock_crypto_utils::signature::MessageOrBlinding;
use rand_core::OsRng;
use schnorr_pok::pok_generalized_pedersen::compute_random_oracle_challenge;
use sha2::Sha256;
use std::collections::{BTreeMap, BTreeSet};

type E = Bls12_381;

const NULLIFIER_DST: &[u8] = b"isegoria/nullifier/hash-to-g1/v1";

/// Per-role generator `H_role` with unknown discrete log, via hash-to-curve.
fn context_generator(role: Role) -> G1Affine {
    let hasher =
        MapToCurveBasedHasher::<G1Projective, DefaultFieldHasher<Sha256>, WBMap<G1Config>>::new(
            NULLIFIER_DST,
        )
        .expect("valid BLS12-381 G1 hash-to-curve configuration");
    hasher
        .hash(role.tag().as_bytes())
        .expect("hashing to G1 cannot fail")
}

/// A per-role nullifier `N = x·H_role` plus a zero-knowledge proof binding it to a
/// valid credential.
pub struct NullifierProof {
    role: Role,
    nullifier: G1Affine,
    /// `t = ρ·H_role`, the Schnorr commitment shared with the BBS+ proof.
    commitment: G1Affine,
    sig_proof: PoKOfSignatureG1Proof<E>,
}

impl NullifierProof {
    /// The nullifier value — equal for the same person and role, so repeated actions
    /// collide and are detectable.
    pub fn nullifier(&self) -> G1Affine {
        self.nullifier
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// The stable protocol identifier keyed on the verified nullifier (docs/08 INV-9):
    /// `H(compressed N)`. Deterministic in `(secret, role)` because `N = x·H_role` is, so
    /// it is the non-rotatable per-role pseudonym — now cryptographically proven, unlike
    /// [`crate::nym::derive_nym`], which the protocol must not key on. It is independent of
    /// the action `context`, so the same person-role has one id across all its actions.
    pub fn id(&self) -> Nym {
        let mut n = Vec::new();
        self.nullifier.serialize_compressed(&mut n).unwrap();
        Nym(tagged("isegoria/nullifier-id/v1", &[n.as_slice()]))
    }
}

fn challenge(
    contribute: impl FnOnce(&mut Vec<u8>) -> Result<(), BBSPlusError>,
    h_role: &G1Affine,
    nullifier: &G1Affine,
    commitment: &G1Affine,
    context: &[u8],
) -> Fr {
    let mut bytes = Vec::new();
    contribute(&mut bytes).expect("challenge contribution");
    h_role.serialize_compressed(&mut bytes).unwrap();
    nullifier.serialize_compressed(&mut bytes).unwrap();
    commitment.serialize_compressed(&mut bytes).unwrap();
    // Length-prefix the action context so a proof is bound to the action it was made for:
    // a proof for one context fails to verify against another (docs/08 AT-ID-05).
    bytes.extend_from_slice(&(context.len() as u64).to_le_bytes());
    bytes.extend_from_slice(context);
    compute_random_oracle_challenge::<Fr, Sha256>(&bytes)
}

/// Prove a role nullifier from a credential, revealing neither the secret nor the label.
/// `context` binds the proof to the action it authorizes (e.g. the item CID and epoch), so
/// it cannot be replayed onto a different action (docs/08 AT-ID-05).
pub fn prove(
    cred: &AnonymousCredential,
    issuer: &IssuerPublic,
    role: Role,
    context: &[u8],
) -> NullifierProof {
    let mut rng = OsRng;
    let x = *cred.secret();
    let h_role = context_generator(role);
    let nullifier = (h_role * x).into_affine();
    let rho = Fr::rand(&mut rng);
    let commitment = (h_role * rho).into_affine();

    // Reuse `rho` as the BBS+ proof's blinding for the secret (message index 0), so a
    // shared challenge ties the credential's `x` to the nullifier's `x`.
    let label = *cred.label_scalar();
    let mb = [
        MessageOrBlinding::BlindMessageWithConcreteBlinding {
            message: &x,
            blinding: rho,
        },
        MessageOrBlinding::BlindMessageRandomly(&label),
    ];
    let protocol = PoKOfSignatureG1Protocol::init(&mut rng, cred.signature(), issuer.params(), mb)
        .expect("valid signature and parameters");

    let c = challenge(
        |w| protocol.challenge_contribution(&BTreeMap::new(), issuer.params(), w),
        &h_role,
        &nullifier,
        &commitment,
        context,
    );
    let sig_proof = protocol.gen_proof(&c).expect("proof generation");

    NullifierProof {
        role,
        nullifier,
        commitment,
        sig_proof,
    }
}

/// Verify a nullifier proof against the issuing committee's public key, for the action
/// `context` it must be bound to (docs/08 AT-ID-05). A proof made for another context fails.
pub fn verify(proof: &NullifierProof, issuer: &IssuerPublic, context: &[u8]) -> bool {
    let h_role = context_generator(proof.role);
    let c = challenge(
        |w| {
            proof
                .sig_proof
                .challenge_contribution(&BTreeMap::new(), issuer.params(), w)
        },
        &h_role,
        &proof.nullifier,
        &proof.commitment,
        context,
    );

    // 1. The BBS+ proof shows knowledge of a valid credential signature over the hidden
    //    messages (nothing revealed).
    if proof
        .sig_proof
        .verify(
            &BTreeMap::new(),
            &c,
            issuer.public_key().clone(),
            issuer.params().clone(),
        )
        .is_err()
    {
        return false;
    }

    // 2. The nullifier commits to the same secret `x`: with response `s = ρ + c·x` for
    //    message 0, `s·H_role == t + c·N`.
    let s = match proof.sig_proof.get_resp_for_message(0, &BTreeSet::new()) {
        Ok(s) => *s,
        Err(_) => return false,
    };
    let lhs = h_role * s;
    let rhs = proof.commitment.into_group() + proof.nullifier * c;
    lhs == rhs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credential::{Credential, Issuer};
    use crate::enrollment::Label;

    #[test]
    fn a_swapped_nullifier_is_rejected() {
        let issuer = Issuer::new([1u8; 32]);
        let holder = Credential::from_secret([9u8; 32]);
        let (req, pending) = holder.request_issuance(&Label([7u8; 32]), &issuer.public());
        let cred = pending.finalize(issuer.issue(&req).unwrap());

        let mut proof = prove(&cred, &issuer.public(), Role::Judge, b"ctx");
        assert!(verify(&proof, &issuer.public(), b"ctx"));

        // Swap in a well-formed but different nullifier (for x+1 instead of x). The
        // binding is load-bearing: verification fails rather than accepting any group
        // element as the nullifier.
        let x = *cred.secret();
        proof.nullifier = (context_generator(Role::Judge) * (x + Fr::from(1u64))).into_affine();
        assert!(!verify(&proof, &issuer.public(), b"ctx"));
    }

    #[test]
    fn a_proof_does_not_verify_under_a_different_context() {
        let issuer = Issuer::new([1u8; 32]);
        let holder = Credential::from_secret([9u8; 32]);
        let (req, pending) = holder.request_issuance(&Label([7u8; 32]), &issuer.public());
        let cred = pending.finalize(issuer.issue(&req).unwrap());

        // A proof bound to action A cannot be replayed onto action B (docs/08 AT-ID-05).
        let proof = prove(&cred, &issuer.public(), Role::Judge, b"action-A");
        assert!(verify(&proof, &issuer.public(), b"action-A"));
        assert!(!verify(&proof, &issuer.public(), b"action-B"));

        // …but the identifier is the person-role, independent of the action context.
        let other = prove(&cred, &issuer.public(), Role::Judge, b"action-B");
        assert_eq!(proof.id(), other.id());
    }
}

#[cfg(test)]
mod proptests {
    //! Property tests (T42) over arbitrary credentials, roles, contexts and single-byte
    //! corruptions of a proof. In the crate because the proof's byte encoding is not
    //! part of the public API.
    use super::*;
    use crate::credential::{Credential, Issuer};
    use crate::enrollment::Label;
    use ark_serialize::CanonicalDeserialize;
    use proptest::prelude::*;

    const ROLES: [Role; 3] = [Role::Propose, Role::Judge, Role::Respond];

    fn issued(secret: [u8; 32], label: [u8; 32]) -> (IssuerPublic, AnonymousCredential) {
        let issuer = Issuer::new([1u8; 32]);
        let holder = Credential::from_secret(secret);
        let (req, pending) = holder.request_issuance(&Label(label), &issuer.public());
        let cred = pending.finalize(issuer.issue(&req).unwrap());
        (issuer.public(), cred)
    }

    fn role() -> impl Strategy<Value = Role> {
        prop::sample::select(ROLES.to_vec())
    }

    /// Canonical encoding of the proof's group elements and BBS+ proof of knowledge.
    fn encode(proof: &NullifierProof) -> Vec<u8> {
        let mut bytes = Vec::new();
        proof.nullifier.serialize_compressed(&mut bytes).unwrap();
        proof.commitment.serialize_compressed(&mut bytes).unwrap();
        proof.sig_proof.serialize_compressed(&mut bytes).unwrap();
        bytes
    }

    /// Decode with full validation; `None` when the bytes are not a well-formed proof.
    fn decode(role: Role, mut bytes: &[u8]) -> Option<NullifierProof> {
        let nullifier = G1Affine::deserialize_compressed(&mut bytes).ok()?;
        let commitment = G1Affine::deserialize_compressed(&mut bytes).ok()?;
        let sig_proof = PoKOfSignatureG1Proof::<E>::deserialize_compressed(&mut bytes).ok()?;
        bytes.is_empty().then_some(NullifierProof {
            role,
            nullifier,
            commitment,
            sig_proof,
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        /// Any credential proves any role under any context, and the proof verifies.
        #[test]
        fn any_valid_proof_verifies(
            secret in any::<[u8; 32]>(),
            label in any::<[u8; 32]>(),
            role in role(),
            context in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let (issuer, cred) = issued(secret, label);
            let proof = prove(&cred, &issuer, role, &context);
            prop_assert!(verify(&proof, &issuer, &context));
            let decoded = decode(role, &encode(&proof)).expect("canonical encoding decodes");
            prop_assert!(verify(&decoded, &issuer, &context));
        }

        /// The three role nullifiers (and ids) of one person are pairwise distinct, and
        /// each is stable across independent proofs and action contexts.
        #[test]
        fn nullifier_is_distinct_per_role_and_stable_within_one(
            secret in any::<[u8; 32]>(),
            ctx_a in prop::collection::vec(any::<u8>(), 0..16),
            ctx_b in prop::collection::vec(any::<u8>(), 0..16),
        ) {
            let (issuer, cred) = issued(secret, [7u8; 32]);
            let proofs: Vec<_> = ROLES.iter().map(|&r| prove(&cred, &issuer, r, &ctx_a)).collect();
            for i in 0..ROLES.len() {
                for j in i + 1..ROLES.len() {
                    prop_assert_ne!(proofs[i].nullifier(), proofs[j].nullifier());
                    prop_assert_ne!(proofs[i].id(), proofs[j].id());
                }
                let again = prove(&cred, &issuer, ROLES[i], &ctx_b);
                prop_assert_eq!(proofs[i].nullifier(), again.nullifier());
                prop_assert_eq!(proofs[i].id(), again.id());
            }
        }

        /// Two people never share a nullifier in the same role.
        #[test]
        fn distinct_secrets_give_distinct_nullifiers(
            a in any::<[u8; 32]>(),
            b in any::<[u8; 32]>(),
            role in role(),
        ) {
            prop_assume!(a != b);
            let (issuer, ca) = issued(a, [7u8; 32]);
            let (_, cb) = issued(b, [7u8; 32]);
            let pa = prove(&ca, &issuer, role, b"ctx");
            let pb = prove(&cb, &issuer, role, b"ctx");
            prop_assert_ne!(pa.nullifier(), pb.nullifier());
        }

        /// Altering any single byte of a proof (nullifier, commitment or BBS+ proof)
        /// either leaves bytes that are no longer a proof or a proof that fails.
        #[test]
        fn any_altered_proof_byte_fails(
            secret in any::<[u8; 32]>(),
            role in role(),
            at in any::<prop::sample::Index>(),
            mask in 1u8..,
        ) {
            let (issuer, cred) = issued(secret, [7u8; 32]);
            let proof = prove(&cred, &issuer, role, b"ctx");
            let mut bytes = encode(&proof);
            let i = at.index(bytes.len());
            bytes[i] ^= mask;
            if let Some(tampered) = decode(role, &bytes) {
                prop_assert!(!verify(&tampered, &issuer, b"ctx"), "byte {i} ^ {mask:#04x} accepted");
            }
        }

        /// A proof is bound to its role and to its context: relabelling it with another
        /// role, or altering any byte of the context, makes it fail.
        #[test]
        fn a_proof_fails_under_another_role_or_an_altered_context(
            secret in any::<[u8; 32]>(),
            role in role(),
            other in role(),
            context in prop::collection::vec(any::<u8>(), 1..32),
            at in any::<prop::sample::Index>(),
            mask in 1u8..,
        ) {
            let (issuer, cred) = issued(secret, [7u8; 32]);
            let mut proof = prove(&cred, &issuer, role, &context);

            let mut altered = context.clone();
            altered[at.index(context.len())] ^= mask;
            prop_assert!(!verify(&proof, &issuer, &altered));

            if other != role {
                proof.role = other;
                prop_assert!(!verify(&proof, &issuer, &context));
            }
        }
    }
}

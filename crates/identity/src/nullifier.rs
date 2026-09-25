//! Semaphore-style zero-knowledge nullifier bound to the credential (`docs/03` §M3,
//! `docs/08` CRYPTO-005): a bespoke sigma-protocol composition over the BBS+ credential
//! (`N = x·H_role`), allowed as a tested exception (see `docs/CLAUDE.md`).

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

/// Per-role generator `H_role` with unknown discrete log.
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

/// A per-role nullifier `N = x·H_role` with a zero-knowledge proof binding it to a credential.
pub struct NullifierProof {
    role: Role,
    nullifier: G1Affine,
    /// `t = ρ·H_role`, the Schnorr commitment shared with the BBS+ proof.
    commitment: G1Affine,
    sig_proof: PoKOfSignatureG1Proof<E>,
}

impl NullifierProof {
    /// The nullifier value — equal for the same person and role, so repeats collide.
    pub fn nullifier(&self) -> G1Affine {
        self.nullifier
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// The stable protocol identifier `H(compressed N)` (`docs/08` INV-9): deterministic
    /// in `(secret, role)`, independent of `context`. The non-rotatable pseudonym the
    /// protocol must key on — never [`crate::nym::derive_nym`].
    pub fn id(&self) -> Nym {
        let mut n = Vec::new();
        self.nullifier.serialize_compressed(&mut n).unwrap();
        Nym(tagged("isegoria/nullifier-id/v1", &[n.as_slice()]))
    }
}

/// A byte encoding of a proof, for tests and fuzz targets: compressed nullifier,
/// commitment and BBS+ proof — not a wire format; the role travels separately.
#[cfg(any(test, fuzzing))]
impl NullifierProof {
    #[doc(hidden)]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.nullifier.serialize_compressed(&mut bytes).unwrap();
        self.commitment.serialize_compressed(&mut bytes).unwrap();
        self.sig_proof.serialize_compressed(&mut bytes).unwrap();
        bytes
    }

    /// Decodes with full validation; `None` when the bytes are not a well-formed proof.
    #[doc(hidden)]
    pub fn from_bytes(role: Role, mut bytes: &[u8]) -> Option<Self> {
        use ark_serialize::CanonicalDeserialize;
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
}

/// The Fiat–Shamir challenge; fallible since [`verify`] computes it over a received proof.
fn challenge(
    contribute: impl FnOnce(&mut Vec<u8>) -> Result<(), BBSPlusError>,
    h_role: &G1Affine,
    nullifier: &G1Affine,
    commitment: &G1Affine,
    context: &[u8],
) -> Result<Fr, BBSPlusError> {
    let mut bytes = Vec::new();
    contribute(&mut bytes)?;
    h_role.serialize_compressed(&mut bytes)?;
    nullifier.serialize_compressed(&mut bytes)?;
    commitment.serialize_compressed(&mut bytes)?;
    // Length-prefix the action context so a proof is bound to the action it was made for:
    // a proof for one context fails to verify against another (docs/08 AT-ID-05).
    bytes.extend_from_slice(&(context.len() as u64).to_le_bytes());
    bytes.extend_from_slice(context);
    Ok(compute_random_oracle_challenge::<Fr, Sha256>(&bytes))
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
    )
    .expect("serializing the holder's own proof into memory cannot fail");
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
    let Ok(c) = challenge(
        |w| {
            proof
                .sig_proof
                .challenge_contribution(&BTreeMap::new(), issuer.params(), w)
        },
        &h_role,
        &proof.nullifier,
        &proof.commitment,
        context,
    ) else {
        return false;
    };

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

        // The binding is load-bearing: a well-formed nullifier for a different x is rejected.
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

        let other = prove(&cred, &issuer.public(), Role::Judge, b"action-B");
        assert_eq!(proof.id(), other.id());
    }
}

#[cfg(test)]
mod proptests {
    //! Property tests over arbitrary credentials, roles, contexts and single-byte proof
    //! corruptions; in the crate since the byte encoding is not part of the public API.
    use super::*;
    use crate::credential::{Credential, Issuer};
    use crate::enrollment::Label;
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

    fn encode(proof: &NullifierProof) -> Vec<u8> {
        proof.to_bytes()
    }

    fn decode(role: Role, bytes: &[u8]) -> Option<NullifierProof> {
        NullifierProof::from_bytes(role, bytes)
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

        /// Arbitrary or spliced bytes never panic the decoder or verifier, and never verify.
        #[test]
        fn hostile_proof_bytes_never_verify(
            secret in any::<[u8; 32]>(),
            splice in any::<bool>(),
            at in any::<prop::sample::Index>(),
            bytes in prop::collection::vec(any::<u8>(), 0..512),
        ) {
            let (issuer, cred) = issued(secret, [7u8; 32]);
            let genuine = encode(&prove(&cred, &issuer, Role::Judge, b"ctx"));
            let candidate = if splice {
                let mut c = genuine.clone();
                let at = at.index(c.len());
                for (dst, src) in c[at..].iter_mut().zip(&bytes) {
                    *dst = *src;
                }
                c
            } else {
                bytes
            };
            if let Some(proof) = decode(Role::Judge, &candidate) {
                if verify(&proof, &issuer, b"ctx") {
                    prop_assert_eq!(&candidate, &genuine);
                }
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

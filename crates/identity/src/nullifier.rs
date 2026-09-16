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
use crate::nym::Role;

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
}

fn challenge(
    contribute: impl FnOnce(&mut Vec<u8>) -> Result<(), BBSPlusError>,
    h_role: &G1Affine,
    nullifier: &G1Affine,
    commitment: &G1Affine,
) -> Fr {
    let mut bytes = Vec::new();
    contribute(&mut bytes).expect("challenge contribution");
    h_role.serialize_compressed(&mut bytes).unwrap();
    nullifier.serialize_compressed(&mut bytes).unwrap();
    commitment.serialize_compressed(&mut bytes).unwrap();
    compute_random_oracle_challenge::<Fr, Sha256>(&bytes)
}

/// Prove a role nullifier from a credential, revealing neither the secret nor the label.
pub fn prove(cred: &AnonymousCredential, issuer: &IssuerPublic, role: Role) -> NullifierProof {
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
    );
    let sig_proof = protocol.gen_proof(&c).expect("proof generation");

    NullifierProof {
        role,
        nullifier,
        commitment,
        sig_proof,
    }
}

/// Verify a nullifier proof against the issuing committee's public key.
pub fn verify(proof: &NullifierProof, issuer: &IssuerPublic) -> bool {
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

        let mut proof = prove(&cred, &issuer.public(), Role::Judge);
        assert!(verify(&proof, &issuer.public()));

        // Swap in a well-formed but different nullifier (for x+1 instead of x). The
        // binding is load-bearing: verification fails rather than accepting any group
        // element as the nullifier.
        let x = *cred.secret();
        proof.nullifier = (context_generator(Role::Judge) * (x + Fr::from(1u64))).into_affine();
        assert!(!verify(&proof, &issuer.public()));
    }
}

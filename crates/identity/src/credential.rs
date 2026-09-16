//! Anonymous credential and its blind issuance. See `docs/03`, §M2.
//!
//! The committee certifies eligibility for a fresh, already-unique label without
//! learning the holder's secret and without being able to recognise the credential
//! later. This is a real **BBS+** blind signature (crate `bbs_plus`, BLS12-381):
//!
//! 1. the holder commits to its secret (hidden) and proves, in zero knowledge, that
//!    it knows the committed value (`schnorr_pok`);
//! 2. the issuer checks that proof and blind-signs `(secret, label)`, learning only
//!    the label;
//! 3. the holder unblinds into a signature over `(secret, label)` it can later prove
//!    knowledge of.
//!
//! **What is real:** blindness (the issuer never sees the secret), the proof of
//! knowledge that binds the request, and a verifiable BBS+ signature.
//!
//! **What is still modeled (`docs/03` §M2, future work):** the issuing key lives with
//! a *single* [`Issuer`]; the spec calls for a **threshold** t-of-n key split across
//! the committee (so no sub-threshold coalition can issue or link). `bbs_plus` ships a
//! `threshold` module to grow into. Selective-disclosure *presentation* of the
//! credential (proving knowledge of the signature while revealing only the label — the
//! `PoKOfSignature` protocol, tied to the M3 nullifier) is likewise future work; here
//! the holder verifies its own freshly issued signature.

use crate::enrollment::Label;
use crate::nym::{derive_nym, Nym, Role};

use ark_bls12_381::{Bls12_381, Fr, G1Affine};
use ark_ff::PrimeField;
use ark_serialize::CanonicalSerialize;
use ark_std::UniformRand;
use bbs_plus::prelude::{KeypairG2, PublicKeyG2, SecretKey, SignatureG1, SignatureParamsG1};
use rand_core::OsRng;
use schnorr_pok::pok_generalized_pedersen::{
    compute_random_oracle_challenge, SchnorrCommitment, SchnorrResponse,
};
use sha2::Sha256;
use std::collections::BTreeMap;

type E = Bls12_381;

/// Domain label for the deterministic signature parameters.
const PARAMS_LABEL: &[u8] = b"isegoria/bbs+/v1";
/// The credential signs two messages: the hidden secret and the public label.
const MSG_COUNT: u32 = 2;
const IDX_SECRET: usize = 0;
const IDX_LABEL: usize = 1;

/// The root secret a node holds. The three role nyms derive from it; it never
/// leaves the holder.
#[derive(Clone, Debug)]
pub struct Credential {
    secret: [u8; 32],
}

impl Credential {
    /// The holder picks its own secret with a CSPRNG; the issuer must not learn it
    /// (that is what blind issuance guarantees).
    pub fn from_secret(secret: [u8; 32]) -> Self {
        Credential { secret }
    }

    pub fn nym(&self, role: Role) -> Nym {
        derive_nym(&self.secret, role)
    }

    fn secret_scalar(&self) -> Fr {
        Fr::from_le_bytes_mod_order(&self.secret)
    }

    /// Build a blind-issuance request: commit to the secret (hidden from the issuer)
    /// and prove knowledge of it, bound to `label`. Returns the request to send and
    /// the state the holder keeps to unblind the response.
    pub fn request_issuance(
        &self,
        label: &Label,
        issuer: &IssuerPublic,
    ) -> (IssuanceRequest, PendingIssuance) {
        let mut rng = OsRng;
        let secret = self.secret_scalar();
        let blinding = Fr::rand(&mut rng);

        // C = h[0]*secret + h_0*blinding — exactly what commit_to_messages builds.
        let commitment = issuer
            .params
            .commit_to_messages([(IDX_SECRET, &secret)], &blinding)
            .expect("single committed message at a valid index");

        // Prove knowledge of (secret, blinding) opening C, via Fiat–Shamir.
        let bases = pok_bases(&issuer.params);
        let sc = SchnorrCommitment::new(&bases, vec![Fr::rand(&mut rng), Fr::rand(&mut rng)]);
        let challenge = pok_challenge(&bases, &commitment, &sc.t, label);
        let response = sc
            .response(&[secret, blinding], &challenge)
            .expect("witness count matches blindings");

        (
            IssuanceRequest {
                commitment,
                label: label.clone(),
                t: sc.t,
                response,
            },
            PendingIssuance {
                blinding,
                secret,
                label_scalar: label_scalar(label),
            },
        )
    }
}

/// Bases of the Pedersen commitment `C`, in the same order as its witnesses
/// `(secret, blinding)`: `h[IDX_SECRET]` for the message, `h_0` for the blinding.
fn pok_bases(params: &SignatureParamsG1<E>) -> [G1Affine; 2] {
    [params.h[IDX_SECRET], params.h_0]
}

/// Fiat–Shamir challenge over the full transcript: bases, commitment, the prover's
/// `t`, and the label the request is bound to.
fn pok_challenge(bases: &[G1Affine; 2], commitment: &G1Affine, t: &G1Affine, label: &Label) -> Fr {
    let mut bytes = Vec::new();
    for b in bases {
        b.serialize_compressed(&mut bytes).unwrap();
    }
    commitment.serialize_compressed(&mut bytes).unwrap();
    t.serialize_compressed(&mut bytes).unwrap();
    bytes.extend_from_slice(&label.0);
    compute_random_oracle_challenge::<Fr, Sha256>(&bytes)
}

fn label_scalar(label: &Label) -> Fr {
    Fr::from_le_bytes_mod_order(&label.0)
}

/// What a holder sends to the issuer: a commitment hiding the secret, the label it
/// wants signed, and a proof of knowledge of the committed secret.
#[derive(Clone, Debug)]
pub struct IssuanceRequest {
    commitment: G1Affine,
    label: Label,
    t: G1Affine,
    response: SchnorrResponse<G1Affine>,
}

/// A blind signature from the issuer, still tied to the holder's blinding factor.
#[derive(Clone, Debug)]
pub struct BlindSignature(SignatureG1<E>);

/// Holder-side state kept between requesting and finalising issuance.
#[derive(Clone, Debug)]
pub struct PendingIssuance {
    blinding: Fr,
    secret: Fr,
    label_scalar: Fr,
}

impl PendingIssuance {
    /// Unblind the issuer's response into a usable anonymous credential.
    pub fn finalize(self, blind: BlindSignature) -> AnonymousCredential {
        AnonymousCredential {
            signature: blind.0.unblind(&self.blinding),
            secret: self.secret,
            label_scalar: self.label_scalar,
        }
    }
}

/// A finished BBS+ credential: a signature over `(secret, label)`.
#[derive(Clone, Debug)]
pub struct AnonymousCredential {
    signature: SignatureG1<E>,
    secret: Fr,
    label_scalar: Fr,
}

impl AnonymousCredential {
    /// Holder-side check that the freshly issued signature is valid under the
    /// issuer's key. (Real presentation to a third party reveals only the label via a
    /// proof of knowledge of the signature — future work, see the module docs.)
    pub fn verify(&self, issuer: &IssuerPublic) -> bool {
        let messages = [self.secret, self.label_scalar];
        self.signature
            .verify(&messages, issuer.public_key.clone(), issuer.params.clone())
            .is_ok()
    }
}

/// The issuer failed to certify a request.
#[derive(Debug, PartialEq, Eq)]
pub enum IssuanceError {
    /// The proof of knowledge of the committed secret did not verify.
    InvalidProofOfKnowledge,
    /// BBS+ signing failed (e.g. malformed request).
    Signing,
}

/// A single blind issuer. Holds the BBS+ secret key; `public()` hands out everything
/// a holder needs to build and later verify a credential.
pub struct Issuer {
    params: SignatureParamsG1<E>,
    secret_key: SecretKey<Fr>,
    public_key: PublicKeyG2<E>,
}

impl Issuer {
    /// Derive the issuing key deterministically from `seed` (so tests and a reloaded
    /// operator agree). Production replaces this single key with a threshold t-of-n
    /// distributed key generation across the committee.
    pub fn new(seed: [u8; 32]) -> Self {
        let params = SignatureParamsG1::<E>::new::<Sha256>(PARAMS_LABEL, MSG_COUNT);
        let keypair = KeypairG2::<E>::generate_using_seed::<Sha256>(&seed, &params);
        Issuer {
            params,
            secret_key: keypair.secret_key.clone(),
            public_key: keypair.public_key.clone(),
        }
    }

    /// The public material a holder needs: the parameters and the verifying key.
    pub fn public(&self) -> IssuerPublic {
        IssuerPublic {
            params: self.params.clone(),
            public_key: self.public_key.clone(),
        }
    }

    /// Verify the request's proof of knowledge, then blind-sign `(secret, label)`,
    /// learning only the label.
    pub fn issue(&self, request: &IssuanceRequest) -> Result<BlindSignature, IssuanceError> {
        let bases = pok_bases(&self.params);
        let challenge = pok_challenge(&bases, &request.commitment, &request.t, &request.label);
        request
            .response
            .is_valid(&bases, &request.commitment, &request.t, &challenge)
            .map_err(|_| IssuanceError::InvalidProofOfKnowledge)?;

        // The label is the only message the issuer knows; the secret stays committed.
        let label = label_scalar(&request.label);
        let mut uncommitted: BTreeMap<usize, &Fr> = BTreeMap::new();
        uncommitted.insert(IDX_LABEL, &label);

        SignatureG1::new_with_committed_messages(
            &mut OsRng,
            &request.commitment,
            uncommitted,
            &self.secret_key,
            &self.params,
        )
        .map(BlindSignature)
        .map_err(|_| IssuanceError::Signing)
    }
}

/// The issuer's public material (parameters + verifying key), shareable with holders.
#[derive(Clone, Debug)]
pub struct IssuerPublic {
    params: SignatureParamsG1<E>,
    public_key: PublicKeyG2<E>,
}

#[cfg(test)]
mod tests {
    //! Properties that need to see inside the request: blindness (the commitment
    //! hides the secret) and soundness (a request whose proof does not match its
    //! transcript is refused). The round-trip is covered in `tests/bbs_credential.rs`.
    use super::*;

    fn label(byte: u8) -> Label {
        Label([byte; 32])
    }

    #[test]
    fn the_commitment_hides_the_secret() {
        let issuer = Issuer::new([1u8; 32]).public();
        let secret = [9u8; 32];
        let holder = Credential::from_secret(secret);

        let (req, _) = holder.request_issuance(&label(7), &issuer);

        // The committed point is not any trivial encoding of the secret: serialising
        // it does not reveal the secret bytes.
        let mut bytes = Vec::new();
        req.commitment.serialize_compressed(&mut bytes).unwrap();
        assert!(!bytes.windows(secret.len()).any(|w| w == secret));

        // Blinding is fresh each time, so the same (secret, label) yields a different
        // commitment on every request — the issuer cannot link two requests.
        let (req2, _) = holder.request_issuance(&label(7), &issuer);
        assert_ne!(req.commitment, req2.commitment);
    }

    #[test]
    fn a_request_whose_proof_does_not_match_is_refused() {
        let issuer = Issuer::new([1u8; 32]);
        let holder = Credential::from_secret([9u8; 32]);

        let (good, _) = holder.request_issuance(&label(7), &issuer.public());
        assert!(issuer.issue(&good).is_ok());

        // Rebind the request to a different label: the Fiat–Shamir challenge the
        // issuer recomputes no longer matches the proof, so knowledge is not shown.
        let mut tampered = good.clone();
        tampered.label = label(8);
        assert!(matches!(
            issuer.issue(&tampered),
            Err(IssuanceError::InvalidProofOfKnowledge)
        ));

        // Likewise if the commitment is swapped for an unrelated one.
        let (other, _) = holder.request_issuance(&label(7), &issuer.public());
        let mut swapped = good.clone();
        swapped.commitment = other.commitment;
        assert!(matches!(
            issuer.issue(&swapped),
            Err(IssuanceError::InvalidProofOfKnowledge)
        ));
    }
}

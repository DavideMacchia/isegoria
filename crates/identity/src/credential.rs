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
use ark_serialize::{CanonicalSerialize, SerializationError};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use ark_std::UniformRand;
use bbs_plus::prelude::{
    BBSPlusError, KeypairG2, PublicKeyG2, SecretKey, SignatureG1, SignatureParamsG1,
};
use bbs_plus::threshold::multiplication_phase::Phase2;
use bbs_plus::threshold::randomness_generation_phase::Phase1;
use bbs_plus::threshold::threshold_bbs_plus::BBSPlusSignatureShare;
use blake2::Blake2b512;
use oblivious_transfer_protocols::ot_based_multiplication::base_ot_multi_party_pairwise::{
    BaseOTOutput, Participant,
};
use oblivious_transfer_protocols::ot_based_multiplication::dkls18_mul_2p::MultiplicationOTEParams;
use oblivious_transfer_protocols::ot_based_multiplication::dkls19_batch_mul_2p::GadgetVector;
use oblivious_transfer_protocols::ParticipantId;
use rand_core::OsRng;
use schnorr_pok::pok_generalized_pedersen::{
    compute_random_oracle_challenge, SchnorrCommitment, SchnorrResponse,
};
use secret_sharing_and_dkg::shamir_ss::deal_random_secret;
use sha2::Sha256;
use sha3::Shake256;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashSet;
use std::fmt;

type E = Bls12_381;

/// Domain label for the deterministic signature parameters.
const PARAMS_LABEL: &[u8] = b"isegoria/bbs+/v1";
/// The credential signs two messages: the hidden secret and the public label.
const MSG_COUNT: u32 = 2;
const IDX_SECRET: usize = 0;
const IDX_LABEL: usize = 1;

/// The root secret a node holds. The three role nyms derive from it; it never
/// leaves the holder.
#[derive(Clone)]
pub struct Credential {
    secret: [u8; 32],
}

impl fmt::Debug for Credential {
    /// Redacted: never log the root secret (PV-4, docs/08 §8.3).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential").finish_non_exhaustive()
    }
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
        let challenge = pok_challenge(&bases, &commitment, &sc.t, label)
            .expect("serializing the holder's own commitment into memory cannot fail");
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
/// `t`, and the label the request is bound to. Fallible rather than panicking because
/// the issuer computes it over a request it received (T44).
fn pok_challenge(
    bases: &[G1Affine; 2],
    commitment: &G1Affine,
    t: &G1Affine,
    label: &Label,
) -> Result<Fr, SerializationError> {
    let mut bytes = Vec::new();
    for b in bases {
        b.serialize_compressed(&mut bytes)?;
    }
    commitment.serialize_compressed(&mut bytes)?;
    t.serialize_compressed(&mut bytes)?;
    bytes.extend_from_slice(&label.0);
    Ok(compute_random_oracle_challenge::<Fr, Sha256>(&bytes))
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
#[derive(Clone)]
pub struct PendingIssuance {
    blinding: Fr,
    secret: Fr,
    label_scalar: Fr,
}

impl fmt::Debug for PendingIssuance {
    /// Redacted: secret, blinding and label scalar (PV-4, docs/08 §8.3).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingIssuance").finish_non_exhaustive()
    }
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
#[derive(Clone)]
pub struct AnonymousCredential {
    signature: SignatureG1<E>,
    secret: Fr,
    label_scalar: Fr,
}

impl fmt::Debug for AnonymousCredential {
    /// Redacted: secret and label scalar (PV-4, docs/08 §8.3).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnonymousCredential")
            .finish_non_exhaustive()
    }
}

impl AnonymousCredential {
    pub(crate) fn signature(&self) -> &SignatureG1<E> {
        &self.signature
    }
    pub(crate) fn secret(&self) -> &Fr {
        &self.secret
    }
    pub(crate) fn label_scalar(&self) -> &Fr {
        &self.label_scalar
    }

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
    /// A credential was already issued for this label (`docs/08` ID-007): one
    /// credential per uniqueness label, so a person cannot obtain a second identity.
    AlreadyIssued,
}

/// Records which uniqueness labels have already been issued a credential, so a person —
/// who has exactly one label (`enrollment`) — gets exactly one credential (`docs/08`
/// ID-007). Mirrors [`crate::enrollment::EnrollmentRegistry`]; pair it with
/// [`Issuer::issue_once`].
#[derive(Default)]
pub struct IssuanceRegistry {
    issued: HashSet<Label>,
}

impl IssuanceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a credential has already been issued for `label`.
    pub fn contains(&self, label: &Label) -> bool {
        self.issued.contains(label)
    }
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
        verify_request(&self.params, request)?;

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

    /// One credential per label (`docs/08` ID-007): blind-sign only if `registry` has not
    /// already issued for this label, recording it on success. This is what stops
    /// whitewashing with a fresh secret (AT-ID-03): the label is fixed at enrollment, so a
    /// second request under a new secret carries the same label and is refused
    /// (AT-ID-02). The label is recorded only after a valid signature, so a bad request
    /// does not burn it.
    pub fn issue_once(
        &self,
        registry: &mut IssuanceRegistry,
        request: &IssuanceRequest,
    ) -> Result<BlindSignature, IssuanceError> {
        if registry.issued.contains(&request.label) {
            return Err(IssuanceError::AlreadyIssued);
        }
        let signature = self.issue(request)?;
        registry.issued.insert(request.label.clone());
        Ok(signature)
    }
}

/// Verify a request's Fiat–Shamir proof of knowledge of the committed secret. Shared
/// by the single [`Issuer`] and the threshold [`ThresholdIssuer`].
fn verify_request(
    params: &SignatureParamsG1<E>,
    request: &IssuanceRequest,
) -> Result<(), IssuanceError> {
    let bases = pok_bases(params);
    let challenge = pok_challenge(&bases, &request.commitment, &request.t, &request.label)
        .map_err(|_| IssuanceError::InvalidProofOfKnowledge)?;
    request
        .response
        .is_valid(&bases, &request.commitment, &request.t, &challenge)
        .map_err(|_| IssuanceError::InvalidProofOfKnowledge)
}

/// The issuer's public material (parameters + verifying key), shareable with holders.
#[derive(Clone, Debug)]
pub struct IssuerPublic {
    params: SignatureParamsG1<E>,
    public_key: PublicKeyG2<E>,
}

impl IssuerPublic {
    pub(crate) fn params(&self) -> &SignatureParamsG1<E> {
        &self.params
    }
    pub(crate) fn public_key(&self) -> &PublicKeyG2<E> {
        &self.public_key
    }
}

// Threshold BBS+ signing parameters (DKLS multiplication over OT extension).
const KAPPA: u16 = 256;
const STAT: u16 = 80;
const BASE_OT_KEY_SIZE: u16 = 128;
type Ote = MultiplicationOTEParams<KAPPA, STAT>;
type Gadget = GadgetVector<Fr, KAPPA, STAT>;

/// Threshold BBS+ issuer (`docs/03` §M2): the signing key is Shamir-shared across `n`
/// members, and a signature needs `t` of them. Issuance runs the DKLS-based MPC of
/// [`bbs_plus::threshold`] over an in-process committee and yields a standard BBS+
/// signature — so the holder's request and unblinding are identical to the single
/// [`Issuer`], and `AnonymousCredential` verifies against the committee's aggregate key.
///
/// **What is real:** threshold signing — `t-1` members cannot sign, and the committee
/// never learns the holder's committed secret. **Still modeled (future work):** a real
/// interactive distributed key generation and base-OT setup between remote members
/// (here a trusted dealer + in-process base OT, seeded for reproducibility) and network
/// transport; the whole committee runs in one process.
pub struct ThresholdIssuer {
    params: SignatureParamsG1<E>,
    public_key: PublicKeyG2<E>,
    key_shares: Vec<Fr>,
    threshold: u16,
    ote_params: Ote,
    gadget: Gadget,
    base_ot: Vec<BaseOTOutput>,
}

impl ThresholdIssuer {
    /// Set up an `n`-member committee that needs `t` to sign, deterministically from
    /// `seed`: a trusted dealer derives the Shamir shares and the one-time base-OT
    /// material. A real remote DKG and OT bootstrap are future work.
    ///
    /// # Panics
    ///
    /// Unless `1 <= t <= n`: the committee's shape is operator configuration.
    pub fn new(seed: [u8; 32], n: u16, t: u16) -> Self {
        assert!(t >= 1 && t <= n, "need 1 <= t <= n");
        let mut rng = StdRng::from_seed(seed);

        let (secret, shares, _) =
            deal_random_secret::<_, Fr>(&mut rng, t, n).expect("valid Shamir sharing");
        let key_shares: Vec<Fr> = shares.0.into_iter().map(|s| s.share).collect();

        let params = SignatureParamsG1::<E>::new::<Sha256>(PARAMS_LABEL, MSG_COUNT);
        let public_key = PublicKeyG2::generate_using_secret_key(&SecretKey(secret), &params);

        let ote_params = MultiplicationOTEParams::<KAPPA, STAT> {};
        let gadget = GadgetVector::new::<Blake2b512>(ote_params, b"isegoria/bbs+/gadget/v1");
        let all: BTreeSet<ParticipantId> = (1..=n).collect();
        let base_ot = setup_base_ot::<BASE_OT_KEY_SIZE>(&mut rng, ote_params.num_base_ot(), n, all);

        ThresholdIssuer {
            params,
            public_key,
            key_shares,
            threshold: t,
            ote_params,
            gadget,
            base_ot,
        }
    }

    /// The public material a holder needs (identical shape to the single issuer).
    pub fn public(&self) -> IssuerPublic {
        IssuerPublic {
            params: self.params.clone(),
            public_key: self.public_key.clone(),
        }
    }

    /// Verify the request's proof of knowledge, then run the threshold MPC across the
    /// first `t` members to blind-sign `(secret, label)`, learning only the label.
    pub fn issue(&self, request: &IssuanceRequest) -> Result<BlindSignature, IssuanceError> {
        verify_request(&self.params, request)?;
        let sig = self
            .threshold_sign(request)
            .map_err(|_| IssuanceError::Signing)?;
        Ok(BlindSignature(sig))
    }

    fn threshold_sign(&self, request: &IssuanceRequest) -> Result<SignatureG1<E>, BBSPlusError> {
        let t = self.threshold;
        let party_set: BTreeSet<ParticipantId> = (1..=t).collect();
        let protocol_id = b"isegoria/bbs+/threshold/v1".to_vec();
        let mut rng = OsRng;

        // Phase 1 — joint randomness (e, s) and masked key / r shares.
        let mut round1 = Vec::new();
        let mut comms = Vec::new();
        let mut comm_zeros = Vec::new();
        for i in 1..=t {
            let mut others = party_set.clone();
            others.remove(&i);
            let (r1, comm, comm_zero) = Phase1::<Fr, 256>::init_for_bbs_plus::<_, Blake2b512>(
                &mut rng,
                1,
                i,
                others,
                protocol_id.clone(),
            )?;
            round1.push(r1);
            comms.push(comm);
            comm_zeros.push(comm_zero);
        }
        for i in 1..=t {
            for j in 1..=t {
                if i != j {
                    let cz = comm_zeros[(j - 1) as usize].get(&i).unwrap().clone();
                    round1[(i - 1) as usize].receive_commitment(
                        j,
                        comms[(j - 1) as usize].clone(),
                        cz,
                    )?;
                }
            }
        }
        for i in 1..=t {
            for j in 1..=t {
                if i != j {
                    let share = round1[(j - 1) as usize].get_comm_shares_and_salts();
                    let zero = round1[(j - 1) as usize]
                        .get_comm_shares_and_salts_for_zero_sharing_protocol_with_other(&i);
                    round1[(i - 1) as usize].receive_shares::<Blake2b512>(j, share, zero)?;
                }
            }
        }
        let mut phase1 = Vec::new();
        for (idx, r1) in round1.into_iter().enumerate() {
            phase1.push(r1.finish_for_bbs_plus::<Blake2b512>(&self.key_shares[idx])?);
        }

        // Phase 2 — OT-based multiplications between every pair of signers.
        let mut round2 = Vec::new();
        let mut msg1s = Vec::new();
        for i in 1..=t {
            let mut others = party_set.clone();
            others.remove(&i);
            let (phase, u) = Phase2::init::<_, Shake256>(
                &mut rng,
                i,
                phase1[(i - 1) as usize].masked_signing_key_shares.clone(),
                phase1[(i - 1) as usize].masked_rs.clone(),
                self.base_ot[(i - 1) as usize].clone(),
                others,
                self.ote_params,
                &self.gadget,
            )?;
            round2.push(phase);
            msg1s.push((i, u));
        }
        // message1 goes sender -> receiver; the receiver replies with message2, which
        // the original sender consumes (keep the roles explicit to route correctly).
        let mut msg2s = Vec::new();
        for (sender, us) in msg1s {
            for (receiver, m) in us {
                let m2 = round2[(receiver - 1) as usize].receive_message1::<Blake2b512, Shake256>(
                    sender,
                    m,
                    &self.gadget,
                )?;
                msg2s.push((sender, receiver, m2));
            }
        }
        for (sender, receiver, m2) in msg2s {
            round2[(sender - 1) as usize].receive_message2::<Blake2b512>(
                receiver,
                m2,
                &self.gadget,
            )?;
        }
        let phase2: Vec<_> = round2.into_iter().map(|p| p.finish()).collect();

        // Phase 3 — each signer non-interactively produces its signature share over the
        // committed request; the aggregate is a standard BBS+ signature.
        let label = label_scalar(&request.label);
        let mut uncommitted: BTreeMap<usize, &Fr> = BTreeMap::new();
        uncommitted.insert(IDX_LABEL, &label);
        let mut shares = Vec::new();
        for i in 0..t as usize {
            shares.push(BBSPlusSignatureShare::new_with_committed_messages(
                &request.commitment,
                uncommitted.clone(),
                0,
                &phase1[i],
                &phase2[i],
                &self.params,
            )?);
        }
        BBSPlusSignatureShare::aggregate(shares)
    }
}

/// Bootstrap pairwise base OT between all `n` members (one-time setup). Mirrors the
/// reference `do_pairwise_base_ot` from `oblivious_transfer_protocols` (which lives in
/// that crate's own test module and so is not importable), driving every message of the
/// endemic-OT exchange in-process. Real deployments run this between remote members.
fn setup_base_ot<const KEY_SIZE: u16>(
    rng: &mut StdRng,
    num_base_ot: u16,
    n: u16,
    all: BTreeSet<ParticipantId>,
) -> Vec<BaseOTOutput> {
    let b = G1Affine::rand(rng);
    let mut base_ots = Vec::new();
    let mut sender_pks = BTreeMap::new();
    for i in 1..=n {
        let mut others = all.clone();
        others.remove(&i);
        let (base_ot, sender_pk_and_proof) =
            Participant::init::<_, Blake2b512>(rng, i, others, num_base_ot, &b).unwrap();
        base_ots.push(base_ot);
        sender_pks.insert(i, sender_pk_and_proof);
    }

    let mut receiver_pks = BTreeMap::new();
    for (sender_id, pks) in sender_pks {
        for (id, pk) in pks {
            let recv_pk = base_ots[(id - 1) as usize]
                .receive_sender_pubkey::<_, Blake2b512, Shake256, KEY_SIZE>(rng, sender_id, pk, &b)
                .unwrap();
            receiver_pks.insert((id, sender_id), recv_pk);
        }
    }

    let mut challenges = BTreeMap::new();
    for ((sender, receiver), pk) in receiver_pks {
        let chal = base_ots[(receiver - 1) as usize]
            .receive_receiver_pubkey::<Blake2b512, Shake256, KEY_SIZE>(sender, pk)
            .unwrap();
        challenges.insert((receiver, sender), chal);
    }

    let mut responses = BTreeMap::new();
    for ((sender, receiver), chal) in challenges {
        let resp = base_ots[(receiver - 1) as usize]
            .receive_challenges::<Blake2b512>(sender, chal)
            .unwrap();
        responses.insert((receiver, sender), resp);
    }

    let mut hashed_keys = BTreeMap::new();
    for ((sender, receiver), resp) in responses {
        let hk = base_ots[(receiver - 1) as usize]
            .receive_responses(sender, resp)
            .unwrap();
        hashed_keys.insert((receiver, sender), hk);
    }

    for ((sender, receiver), hk) in hashed_keys {
        base_ots[(receiver - 1) as usize]
            .receive_hashed_keys::<Blake2b512>(sender, hk)
            .unwrap();
    }

    base_ots.into_iter().map(|b| b.finish()).collect()
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

#[cfg(test)]
mod proptests {
    //! Property tests (T42) over arbitrary secrets, labels and single-byte corruptions.
    //! They live in the crate because the byte-level encoding of a credential
    //! (signature ‖ secret ‖ label scalar) is not part of the public API.
    use super::*;
    use ark_serialize::CanonicalDeserialize;
    use proptest::prelude::*;

    /// Canonical encoding of everything `verify` consumes.
    fn encode(cred: &AnonymousCredential) -> Vec<u8> {
        let mut bytes = Vec::new();
        cred.signature.serialize_compressed(&mut bytes).unwrap();
        cred.secret.serialize_compressed(&mut bytes).unwrap();
        cred.label_scalar.serialize_compressed(&mut bytes).unwrap();
        bytes
    }

    /// Decode with full validation; `None` when the bytes are not a well-formed
    /// credential (off-curve point, non-canonical scalar, trailing bytes).
    fn decode(mut bytes: &[u8]) -> Option<AnonymousCredential> {
        let signature = SignatureG1::<E>::deserialize_compressed(&mut bytes).ok()?;
        let secret = Fr::deserialize_compressed(&mut bytes).ok()?;
        let label_scalar = Fr::deserialize_compressed(&mut bytes).ok()?;
        bytes.is_empty().then_some(AnonymousCredential {
            signature,
            secret,
            label_scalar,
        })
    }

    fn issued(seed: [u8; 32], secret: [u8; 32], label: [u8; 32]) -> (Issuer, AnonymousCredential) {
        let issuer = Issuer::new(seed);
        let holder = Credential::from_secret(secret);
        let (request, pending) = holder.request_issuance(&Label(label), &issuer.public());
        let credential = pending.finalize(issuer.issue(&request).unwrap());
        (issuer, credential)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        /// Any credential issued on any (secret, label) by any issuer verifies, and the
        /// encoding round-trips to a credential that still verifies.
        #[test]
        fn any_issued_credential_verifies(
            seed in any::<[u8; 32]>(),
            secret in any::<[u8; 32]>(),
            label in any::<[u8; 32]>(),
        ) {
            let (issuer, cred) = issued(seed, secret, label);
            prop_assert!(cred.verify(&issuer.public()));
            let decoded = decode(&encode(&cred)).expect("canonical encoding decodes");
            prop_assert!(decoded.verify(&issuer.public()));
        }

        /// Altering any single byte of the encoding (signature, secret or label) either
        /// yields bytes that are no longer a credential or a credential that fails to
        /// verify: nothing in the encoding is malleable.
        #[test]
        fn any_altered_byte_fails(
            secret in any::<[u8; 32]>(),
            label in any::<[u8; 32]>(),
            at in any::<prop::sample::Index>(),
            mask in 1u8..,
        ) {
            let (issuer, cred) = issued([1u8; 32], secret, label);
            let mut bytes = encode(&cred);
            let i = at.index(bytes.len());
            bytes[i] ^= mask;
            if let Some(tampered) = decode(&bytes) {
                prop_assert!(!tampered.verify(&issuer.public()), "byte {i} ^ {mask:#04x} accepted");
            }
        }

        /// A credential verifies only under the issuer that signed it.
        #[test]
        fn a_credential_fails_under_any_other_issuer(
            seed in any::<[u8; 32]>(),
            other in any::<[u8; 32]>(),
            secret in any::<[u8; 32]>(),
        ) {
            prop_assume!(seed != other);
            let (_, cred) = issued(seed, secret, [7u8; 32]);
            prop_assert!(!cred.verify(&Issuer::new(other).public()));
        }
    }
}

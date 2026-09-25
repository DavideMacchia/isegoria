//! Per-credential proposal rate limit (`docs/08` ID-008, T11): a proposer may deposit at
//! most `quota` drafts per epoch, keyed on the Propose nullifier id (INV-9); `quota`
//! comes from the author score (`reputation::proposal_rate`), never money.

use identity::credential::{AnonymousCredential, Credential, Issuer};
use identity::enrollment::Label;
use identity::nullifier::prove;
use identity::nym::Role;
use network::log::TransparencyLog;
use protocol::admission::QuotaLedger;
use protocol::deposit::{deposit_context, deposit_with_identity, DepositRejected, Draft};
use scoring::reputation::proposal_rate;

const EPOCH: u64 = 7;

fn issued(secret: [u8; 32]) -> (Issuer, AnonymousCredential) {
    let issuer = Issuer::new([1u8; 32]);
    let holder = Credential::from_secret(secret);
    let (req, pending) = holder.request_issuance(&Label([7u8; 32]), &issuer.public());
    let cred = pending.finalize(issuer.issue(&req).unwrap());
    (issuer, cred)
}

fn propose(
    log: &mut TransparencyLog,
    ledger: &mut QuotaLedger,
    issuer: &Issuer,
    cred: &AnonymousCredential,
    tag: &str,
    quota: u32,
) -> Result<(), DepositRejected> {
    let draft = Draft {
        item: tag.as_bytes().to_vec(),
        primary_source: b"Gazzetta Ufficiale".to_vec(),
    };
    let proof = prove(
        cred,
        &issuer.public(),
        Role::Propose,
        &deposit_context(draft.content_id(), EPOCH),
    );
    deposit_with_identity(log, &draft, &proof, &issuer.public(), EPOCH, ledger, quota).map(|_| ())
}

#[test]
fn over_quota_proposals_are_rejected() {
    let (issuer, cred) = issued([9u8; 32]);
    let mut log = TransparencyLog::new();
    let mut ledger = QuotaLedger::new();
    let quota = 3u32;

    for j in 0..quota {
        assert!(
            propose(
                &mut log,
                &mut ledger,
                &issuer,
                &cred,
                &format!("draft {j}"),
                quota
            )
            .is_ok(),
            "proposal {j} within quota"
        );
    }
    assert_eq!(
        propose(&mut log, &mut ledger, &issuer, &cred, "one too many", quota),
        Err(DepositRejected::OverQuota)
    );
    assert_eq!(
        log.len(),
        quota as usize,
        "over-quota proposals never hit the log"
    );
}

#[test]
fn distinct_proposers_have_independent_quotas() {
    // A different secret is a different Propose nullifier id.
    let (issuer, alice) = issued([9u8; 32]);
    let bob = {
        let holder = Credential::from_secret([10u8; 32]);
        let (req, pending) = holder.request_issuance(&Label([8u8; 32]), &issuer.public());
        pending.finalize(issuer.issue(&req).unwrap())
    };
    let mut log = TransparencyLog::new();
    let mut ledger = QuotaLedger::new();

    assert!(propose(&mut log, &mut ledger, &issuer, &alice, "a1", 1).is_ok());
    assert_eq!(
        propose(&mut log, &mut ledger, &issuer, &alice, "a2", 1),
        Err(DepositRejected::OverQuota),
        "alice is at quota"
    );
    assert!(propose(&mut log, &mut ledger, &issuer, &bob, "b1", 1).is_ok());
}

#[test]
fn the_quota_is_metered_by_reputation_not_money() {
    // The budget comes from the author score C_a via `proposal_rate`.
    let (q_min, q_max) = (2.0, 20.0);
    let low = proposal_rate(0.1, q_min, q_max);
    let high = proposal_rate(0.9, q_min, q_max);
    assert!(
        high > low,
        "reputation buys proposal budget: {high} vs {low}"
    );
    assert!(
        low >= q_min && high <= q_max,
        "the rate stays within its band"
    );
}

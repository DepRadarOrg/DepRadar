//! End-to-end: the off-chain scanner flags a typosquat, and the on-chain
//! router carries a bounty for it from creation to payout. The bounty's
//! `issue_ref` comes from the finding via the shared format, and is parsed
//! back from what the contract stored.

use deprader::model::{parse_issue_ref, Ecosystem, Finding, FindingKind, IssueKind};
use deprader::typosquat::scanner::score_package;
use deprader_router::{BountyStatus, ClaimStatus, DepRaderRouter, DepRaderRouterClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env, String, Symbol,
};

const START: u64 = 1_000_000;
const DEADLINE: u64 = START + 30 * 24 * 60 * 60;
const AMOUNT: i128 = 400;

#[test]
fn typosquat_finding_becomes_a_released_bounty() {
    // Off-chain: score the name and turn the result into a finding.
    let scan = score_package("expresss");
    assert!(
        scan.distance_match.is_some() || scan.homoglyph_match.is_some(),
        "expresss should be flagged: {scan:?}"
    );
    let finding = Finding {
        package: scan.candidate.clone(),
        version: Some("1.0.0".to_string()),
        ecosystem: Ecosystem::Npm,
        direct: true,
        path: Vec::new(),
        kind: FindingKind::Typosquat {
            distance_match: scan.distance_match.clone(),
            homoglyph_match: scan.homoglyph_match.clone(),
        },
    };
    let issue_ref = finding.issue_ref();
    assert_eq!(issue_ref, "npm:expresss@1.0.0#typosquat:express");

    // On-chain: a local router with one allowed token.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(START);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let claimant = Address::generate(&env);

    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token = TokenClient::new(&env, &sac.address());
    StellarAssetClient::new(&env, &sac.address()).mint(&creator, &1_000);

    let contract_id = env.register(DepRaderRouter, ());
    let client = DepRaderRouterClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.allow_token(&sac.address());

    let bounty_id = client.create_bounty(
        &creator,
        &String::from_str(&env, &issue_ref),
        &Some(Symbol::new(&env, finding.ecosystem.as_ref_str())),
        &Some(String::from_str(&env, &finding.package)),
        &Some(3),
        &AMOUNT,
        &token.address,
        &DEADLINE,
    );

    // The stored ref parses back to the finding we started from.
    let bounty = client.get_bounty(&bounty_id).expect("bounty exists");
    let stored = bounty.issue_ref.to_string();
    let parsed = parse_issue_ref(&stored).expect("stored issue_ref parses");
    assert_eq!(parsed.ecosystem, Ecosystem::Npm);
    assert_eq!(parsed.package, "expresss");
    assert_eq!(parsed.version.as_deref(), Some("1.0.0"));
    assert_eq!(parsed.kind, IssueKind::Typosquat);
    assert_eq!(parsed.detail.as_deref(), Some("express"));

    // Claim and release.
    let claim_id = client.submit_claim(
        &bounty_id,
        &claimant,
        &String::from_str(&env, "https://github.com/npm/registry/issues/1"),
    );
    client.release(&admin, &bounty_id, &claim_id);

    let bounty = client.get_bounty(&bounty_id).unwrap();
    assert_eq!(bounty.status, BountyStatus::Released);
    assert_eq!(
        client.get_claim(&bounty_id, &claim_id).unwrap().status,
        ClaimStatus::Accepted
    );
    assert_eq!(token.balance(&claimant), AMOUNT);
    assert_eq!(token.balance(&contract_id), 0);
}

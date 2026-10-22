use super::{
    BountyStatus, ClaimStatus, DepRaderRouter, DepRaderRouterClient, Error,
    MAX_BOUNTY_DURATION_SECS, MAX_CLAIMS_PER_BOUNTY, MAX_ECOSYSTEM_LEN, MAX_PACKAGE_LEN,
    REVIEW_WINDOW_SECS,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env, String, Symbol, TryFromVal,
};

const START: u64 = 1_000_000;
const DEADLINE: u64 = START + 30 * 24 * 60 * 60;
const AMOUNT: i128 = 400;

struct Setup<'a> {
    env: Env,
    client: DepRaderRouterClient<'a>,
    token: TokenClient<'a>,
    admin: Address,
    creator: Address,
}

/// Fresh env at ledger time `START` with an initialized router and a token
/// in which `creator` holds 1_000 units. All auths are mocked; tests that
/// care about *who* authorized a call check `env.auths()`.
fn setup<'a>() -> Setup<'a> {
    setup_with_token_allowlist(true)
}

fn setup_with_token_allowlist<'a>(allow_token: bool) -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(START);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin);
    let token = TokenClient::new(&env, &sac.address());
    StellarAssetClient::new(&env, &sac.address()).mint(&creator, &1_000);

    let contract_id = env.register(DepRaderRouter, ());
    let client = DepRaderRouterClient::new(&env, &contract_id);
    client.initialize(&admin);
    if allow_token {
        client.allow_token(&sac.address());
    }

    Setup {
        env,
        client,
        token,
        admin,
        creator,
    }
}

impl Setup<'_> {
    fn s(&self, v: &str) -> String {
        String::from_str(&self.env, v)
    }

    fn create(&self) -> u64 {
        self.client.create_bounty(
            &self.creator,
            &self.s("org/repo#123"),
            &AMOUNT,
            &self.token.address,
            &DEADLINE,
            &Some(symbol_short!("rust")),
            &Some(self.s("serde")),
            &Some(3),
        )
    }

    fn create_with_meta(
        &self,
        issue_ref: &str,
        ecosystem: Option<Symbol>,
        package: Option<&str>,
        severity: Option<u32>,
    ) -> Result<u64, Error> {
        let package = package.map(|p| self.s(p));
        self.client
            .try_create_bounty(
                &self.creator,
                &self.s(issue_ref),
                &AMOUNT,
                &self.token.address,
                &DEADLINE,
                &ecosystem,
                &package,
                &severity,
            )
            .map_err(|e| e.unwrap())
    }

    fn add_reviewer(&self, reviewer: &Address) {
        self.client.add_reviewer(&self.admin, reviewer);
    }

    fn claim(&self, bounty_id: u64) -> (Address, u32) {
        let claimant = Address::generate(&self.env);
        let id = self
            .client
            .submit_claim(&bounty_id, &claimant, &self.s("org/repo#123-pr-456"));
        (claimant, id)
    }

    fn last_event_topic(&self) -> Symbol {
        let (_, topics, _) = self.env.events().all().last().expect("an event");
        Symbol::try_from_val(&self.env, &topics.get(0).unwrap()).unwrap()
    }

    /// The single address whose authorization the last invocation required.
    fn last_auth(&self) -> Address {
        let auths = self.env.auths();
        assert_eq!(auths.len(), 1, "expected exactly one authorizer");
        auths[0].0.clone()
    }
}

// --------------------------------------------------------------- initialize

#[test]
fn test_initialize_twice_fails() {
    let t = setup();
    let other = Address::generate(&t.env);
    assert_eq!(
        t.client.try_initialize(&other),
        Err(Ok(Error::AlreadyInitialized))
    );
    assert_eq!(t.client.get_admin(), Some(t.admin.clone()));
}

#[test]
fn test_create_requires_initialized_contract() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(START);
    let creator = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let client = DepRaderRouterClient::new(&env, &env.register(DepRaderRouter, ()));

    let result = client.try_create_bounty(
        &creator,
        &String::from_str(&env, "org/repo#1"),
        &AMOUNT,
        &token,
        &DEADLINE,
        &None,
        &None,
        &None,
    );
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
}

#[test]
fn test_admin_can_pause_and_resume_bounty_operations() {
    let t = setup();
    assert!(!t.client.is_paused());

    t.client.set_paused(&true);
    assert!(t.client.is_paused());
    assert_eq!(
        t.client.try_create_bounty(
            &t.creator,
            &t.s("org/repo#paused"),
            &AMOUNT,
            &t.token.address,
            &DEADLINE,
            &None,
            &None,
            &None,
        ),
        Err(Ok(Error::Paused))
    );

    t.client.set_paused(&false);
    assert!(!t.client.is_paused());
    assert_eq!(t.create(), 1);
}

// ------------------------------------------------------------ create_bounty

#[test]
fn test_create_bounty_locks_funds_and_assigns_ids() {
    let t = setup();

    let first = t.create();
    assert_eq!(t.last_event_topic(), symbol_short!("created"));
    let second = t.create();

    assert_eq!((first, second), (1, 2));
    assert_eq!(t.client.bounty_count(), 2);
    assert_eq!(t.token.balance(&t.creator), 1_000 - 2 * AMOUNT);
    assert_eq!(t.token.balance(&t.client.address), 2 * AMOUNT);

    let bounty = t.client.get_bounty(&first).expect("bounty should exist");
    assert_eq!(bounty.creator, t.creator);
    assert_eq!(bounty.issue_ref, t.s("org/repo#123"));
    assert_eq!(bounty.ecosystem, Some(symbol_short!("rust")));
    assert_eq!(bounty.package, Some(t.s("serde")));
    assert_eq!(bounty.severity, Some(3));
    assert_eq!(bounty.amount, AMOUNT);
    assert_eq!(bounty.token, t.token.address);
    assert_eq!(bounty.status, BountyStatus::Open);
    assert_eq!(bounty.created_at, START);
    assert_eq!(bounty.deadline, DEADLINE);
    assert_eq!(bounty.claim_count, 0);
}

#[test]
fn test_create_bounty_requires_creator_auth() {
    let t = setup();
    t.create();
    assert_eq!(t.last_auth(), t.creator);
}

#[test]
fn test_create_bounty_rejects_bad_input() {
    let t = setup();
    let create = |r: &str, amount: i128, deadline: u64| {
        t.client.try_create_bounty(
            &t.creator,
            &t.s(r),
            &amount,
            &t.token.address,
            &deadline,
            &None,
            &None,
            &None,
        )
    };

    assert_eq!(create("x", 0, DEADLINE), Err(Ok(Error::InvalidAmount)));
    assert_eq!(create("x", -5, DEADLINE), Err(Ok(Error::InvalidAmount)));
    assert_eq!(
        create("", AMOUNT, DEADLINE),
        Err(Ok(Error::InvalidReference))
    );
    let long = "a".repeat(257);
    assert_eq!(
        create(&long, AMOUNT, DEADLINE),
        Err(Ok(Error::InvalidReference))
    );
    assert_eq!(create("x", AMOUNT, START), Err(Ok(Error::InvalidDeadline)));
    assert_eq!(
        create("x", AMOUNT, START + MAX_BOUNTY_DURATION_SECS + 1),
        Err(Ok(Error::InvalidDeadline))
    );

    // Nothing was locked and no ID was consumed.
    assert_eq!(t.token.balance(&t.client.address), 0);
    assert_eq!(t.client.bounty_count(), 0);
}

// ------------------------------------------------ structured metadata (#117)

#[test]
fn test_create_bounty_stores_structured_metadata() {
    let t = setup();
    let id = t
        .create_with_meta(
            "org/repo#7",
            Some(symbol_short!("npm")),
            Some("lodash"),
            Some(9),
        )
        .unwrap();

    let bounty = t.client.get_bounty(&id).unwrap();
    assert_eq!(bounty.issue_ref, t.s("org/repo#7"));
    assert_eq!(bounty.ecosystem, Some(symbol_short!("npm")));
    assert_eq!(bounty.package, Some(t.s("lodash")));
    assert_eq!(bounty.severity, Some(9));
}

#[test]
fn test_create_bounty_metadata_is_optional() {
    let t = setup();
    let id = t.create_with_meta("org/repo#8", None, None, None).unwrap();

    let bounty = t.client.get_bounty(&id).unwrap();
    assert_eq!(bounty.ecosystem, None);
    assert_eq!(bounty.package, None);
    assert_eq!(bounty.severity, None);
}

#[test]
fn test_create_bounty_rejects_empty_metadata() {
    let t = setup();
    assert_eq!(
        t.create_with_meta("org/repo#9", Some(symbol_short!("")), None, None),
        Err(Error::InvalidMetadata)
    );
    assert_eq!(
        t.create_with_meta("org/repo#9", None, Some(""), None),
        Err(Error::InvalidMetadata)
    );
    assert_eq!(t.client.bounty_count(), 0);
}

#[test]
fn test_create_bounty_rejects_overlong_metadata() {
    let t = setup();

    let long_eco = "e".repeat(MAX_ECOSYSTEM_LEN + 1);
    let eco_sym = Symbol::new(&t.env, &long_eco);
    assert_eq!(
        t.create_with_meta("org/repo#10", Some(eco_sym), None, None),
        Err(Error::InvalidMetadata)
    );

    let long_pkg = "p".repeat(MAX_PACKAGE_LEN + 1);
    assert_eq!(
        t.create_with_meta("org/repo#10", None, Some(&long_pkg), None),
        Err(Error::InvalidMetadata)
    );
    assert_eq!(t.client.bounty_count(), 0);
}

#[test]
fn test_create_bounty_still_validates_issue_ref() {
    let t = setup();
    assert_eq!(
        t.create_with_meta("", Some(symbol_short!("go")), None, None),
        Err(Error::InvalidReference)
    );
    let long = "a".repeat(257);
    assert_eq!(
        t.create_with_meta(&long, Some(symbol_short!("go")), None, None),
        Err(Error::InvalidReference)
    );
}

// ------------------------------------------------------------- submit_claim

#[test]
fn test_submit_claim_records_without_moving_funds() {
    let t = setup();
    let bounty_id = t.create();
    let (claimant, claim_id) = t.claim(bounty_id);

    assert_eq!(claim_id, 0);
    assert_eq!(t.last_auth(), claimant);
    assert_eq!(t.last_event_topic(), symbol_short!("claimed"));
    assert_eq!(t.token.balance(&t.client.address), AMOUNT);

    let claim = t.client.get_claim(&bounty_id, &claim_id).unwrap();
    assert_eq!(claim.claimant, claimant);
    assert_eq!(claim.proof_ref, t.s("org/repo#123-pr-456"));
    assert_eq!(claim.submitted_at, START);
    assert_eq!(claim.status, ClaimStatus::Pending);
    assert_eq!(t.client.get_claim_id(&bounty_id, &claimant), Some(claim_id));

    let bounty = t.client.get_bounty(&bounty_id).unwrap();
    assert_eq!((bounty.claim_count, bounty.pending_claims), (1, 1));
}

#[test]
fn test_multiple_claimants_can_claim_one_bounty() {
    let t = setup();
    let bounty_id = t.create();
    let (_, a) = t.claim(bounty_id);
    let (_, b) = t.claim(bounty_id);
    let (_, c) = t.claim(bounty_id);

    assert_eq!((a, b, c), (0, 1, 2));
    assert_eq!(t.client.get_bounty(&bounty_id).unwrap().pending_claims, 3);
}

#[test]
fn test_submit_claim_rejects_duplicates_and_bad_input() {
    let t = setup();
    let bounty_id = t.create();
    let (claimant, _) = t.claim(bounty_id);

    assert_eq!(
        t.client
            .try_submit_claim(&bounty_id, &claimant, &t.s("again")),
        Err(Ok(Error::DuplicateClaim))
    );
    let someone = Address::generate(&t.env);
    assert_eq!(
        t.client.try_submit_claim(&bounty_id, &someone, &t.s("")),
        Err(Ok(Error::InvalidReference))
    );
    assert_eq!(
        t.client.try_submit_claim(&999, &someone, &t.s("pr")),
        Err(Ok(Error::BountyNotFound))
    );
    assert!(t.client.get_claim(&999, &0).is_none());
}

#[test]
fn test_submit_claim_after_deadline_fails() {
    let t = setup();
    let bounty_id = t.create();
    t.env.ledger().set_timestamp(DEADLINE);

    let claimant = Address::generate(&t.env);
    assert_eq!(
        t.client
            .try_submit_claim(&bounty_id, &claimant, &t.s("late")),
        Err(Ok(Error::ClaimWindowClosed))
    );
}

#[test]
fn test_submit_claim_is_capped_per_bounty() {
    let t = setup();
    let bounty_id = t.create();
    for _ in 0..MAX_CLAIMS_PER_BOUNTY {
        t.claim(bounty_id);
    }
    let one_more = Address::generate(&t.env);
    assert_eq!(
        t.client.try_submit_claim(&bounty_id, &one_more, &t.s("pr")),
        Err(Ok(Error::TooManyClaims))
    );
}

// ------------------------------------------------------- claims_of index

#[test]
fn test_claims_of_indexes_across_bounties() {
    let t = setup();
    let claimant = Address::generate(&t.env);

    let mut expected = soroban_sdk::Vec::new(&t.env);
    for i in 0..3u64 {
        let bounty_id = t.create();
        let claim_id = t
            .client
            .submit_claim(&bounty_id, &claimant, &t.s("org/repo#123-pr-456"));
        expected.push_back((bounty_id, claim_id));
        // Interleave another claimant so the index is per-claimant.
        let other = Address::generate(&t.env);
        t.client
            .submit_claim(&bounty_id, &other, &t.s("org/repo#123-pr-789"));
        assert_eq!(i, bounty_id - 1);
    }

    let claims = t.client.get_claims_of(&claimant, &0, &10);
    assert_eq!(claims, expected);

    // A claimant with no claims gets an empty vec.
    let stranger = Address::generate(&t.env);
    assert_eq!(t.client.get_claims_of(&stranger, &0, &10).len(), 0);
}

#[test]
fn test_claims_of_pagination() {
    let t = setup();
    let claimant = Address::generate(&t.env);

    let mut expected = soroban_sdk::Vec::new(&t.env);
    for _ in 0..5u64 {
        let bounty_id = t.create();
        let claim_id = t
            .client
            .submit_claim(&bounty_id, &claimant, &t.s("org/repo#123-pr-456"));
        expected.push_back((bounty_id, claim_id));
    }

    // Slice in the middle.
    let page = t.client.get_claims_of(&claimant, &1, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), expected.get(1).unwrap());
    assert_eq!(page.get(1).unwrap(), expected.get(2).unwrap());

    // Limit larger than the remainder clamps to what's left.
    let tail = t.client.get_claims_of(&claimant, &3, &10);
    assert_eq!(tail.len(), 2);
    assert_eq!(tail.get(0).unwrap(), expected.get(3).unwrap());
    assert_eq!(tail.get(1).unwrap(), expected.get(4).unwrap());

    // Out-of-range start yields an empty vec.
    assert_eq!(t.client.get_claims_of(&claimant, &5, &10).len(), 0);
    assert_eq!(t.client.get_claims_of(&claimant, &100, &10).len(), 0);

    // Zero limit yields an empty vec.
    assert_eq!(t.client.get_claims_of(&claimant, &0, &0).len(), 0);
}

#[test]
fn test_claims_of_is_capped_per_claimant() {
    let t = setup();
    let claimant = Address::generate(&t.env);

    // Spread the claims across several bounties so the per-bounty cap is
    // never the limiting factor.
    let mut bounties = soroban_sdk::Vec::new(&t.env);
    while (bounties.len() as u32) * MAX_CLAIMS_PER_BOUNTY < MAX_CLAIMS_PER_CLAIMANT {
        bounties.push_back(t.create());
    }

    let mut submitted = 0u32;
    'outer: for bounty_id in bounties.iter() {
        for _ in 0..MAX_CLAIMS_PER_BOUNTY {
            if submitted == MAX_CLAIMS_PER_CLAIMANT {
                break 'outer;
            }
            t.client
                .submit_claim(&bounty_id, &claimant, &t.s("org/repo#123-pr-456"));
            submitted += 1;
        }
    }
    assert_eq!(submitted, MAX_CLAIMS_PER_CLAIMANT);
    assert_eq!(
        t.client
            .get_claims_of(&claimant, &0, &MAX_CLAIMS_PER_CLAIMANT)
            .len(),
        MAX_CLAIMS_PER_CLAIMANT
    );

    // One more claim on a fresh bounty is rejected by the claimant cap.
    let extra_bounty = t.create();
    assert_eq!(
        t.client
            .try_submit_claim(&extra_bounty, &claimant, &t.s("org/repo#123-pr-456")),
        Err(Ok(Error::TooManyClaims))
    );
}

// ------------------------------------------------------ reject / release

#[test]
fn test_release_pays_chosen_claim() {
    let t = setup();
    let bounty_id = t.create();
    let (_, first) = t.claim(bounty_id);
    let (winner, second) = t.claim(bounty_id);

    t.client.release(&t.admin, &bounty_id, &second);
    assert_eq!(t.last_auth(), t.admin);
    assert_eq!(t.last_event_topic(), symbol_short!("released"));

    assert_eq!(t.token.balance(&winner), AMOUNT);
    assert_eq!(t.token.balance(&t.client.address), 0);
    let bounty = t.client.get_bounty(&bounty_id).unwrap();
    assert_eq!(bounty.status, BountyStatus::Released);
    assert_eq!(
        t.client.get_claim(&bounty_id, &second).unwrap().status,
        ClaimStatus::Accepted
    );
    // The other claim was never decided on, and now can't be.
    assert_eq!(
        t.client.get_claim(&bounty_id, &first).unwrap().status,
        ClaimStatus::Pending
    );
    assert_eq!(
        t.client.try_release(&t.admin, &bounty_id, &first),
        Err(Ok(Error::BountyNotOpen))
    );
}

#[test]
fn test_squatted_claim_can_be_rejected_and_real_claim_paid() {
    // The scenario the old single-claim design couldn't handle: a junk
    // claim lands first, the real finder claims second.
    let t = setup();
    let bounty_id = t.create();
    let (squatter, junk) = t.claim(bounty_id);
    let (finder, real) = t.claim(bounty_id);

    t.client.reject_claim(&t.admin, &bounty_id, &junk);
    assert_eq!(t.last_auth(), t.admin);
    assert_eq!(t.last_event_topic(), symbol_short!("rejected"));
    assert_eq!(
        t.client.get_claim(&bounty_id, &junk).unwrap().status,
        ClaimStatus::Rejected
    );
    assert_eq!(t.client.get_bounty(&bounty_id).unwrap().pending_claims, 1);

    // A rejected claim can't be paid or re-submitted.
    assert_eq!(
        t.client.try_release(&t.admin, &bounty_id, &junk),
        Err(Ok(Error::ClaimNotPending))
    );
    assert_eq!(
        t.client
            .try_submit_claim(&bounty_id, &squatter, &t.s("retry")),
        Err(Ok(Error::DuplicateClaim))
    );

    t.client.release(&t.admin, &bounty_id, &real);
    assert_eq!(t.token.balance(&finder), AMOUNT);
    assert_eq!(t.token.balance(&squatter), 0);
}

#[test]
fn test_release_works_during_review_window() {
    let t = setup();
    let bounty_id = t.create();
    let (claimant, claim_id) = t.claim(bounty_id);

    t.env.ledger().set_timestamp(DEADLINE + 60);
    t.client.release(&t.admin, &bounty_id, &claim_id);
    assert_eq!(t.token.balance(&claimant), AMOUNT);
}

#[test]
fn test_release_and_reject_errors() {
    let t = setup();
    let bounty_id = t.create();

    assert_eq!(
        t.client.try_release(&t.admin, &bounty_id, &0),
        Err(Ok(Error::ClaimNotFound))
    );
    assert_eq!(
        t.client.try_reject_claim(&t.admin, &bounty_id, &0),
        Err(Ok(Error::ClaimNotFound))
    );
    assert_eq!(
        t.client.try_release(&t.admin, &999, &0),
        Err(Ok(Error::BountyNotFound))
    );

    let (_, claim_id) = t.claim(bounty_id);
    t.client.release(&t.admin, &bounty_id, &claim_id);
    assert_eq!(
        t.client.try_release(&t.admin, &bounty_id, &claim_id),
        Err(Ok(Error::BountyNotOpen))
    );
}

#[test]
fn test_release_without_admin_auth_fails() {
    let t = setup();
    let bounty_id = t.create();
    let (claimant, claim_id) = t.claim(bounty_id);

    // Drop the blanket auth mock: nobody has signed anything.
    t.env.set_auths(&[]);
    assert!(t
        .client
        .try_release(&t.admin, &bounty_id, &claim_id)
        .is_err());
    assert!(t
        .client
        .try_reject_claim(&t.admin, &bounty_id, &claim_id)
        .is_err());

    assert_eq!(t.token.balance(&claimant), 0);
    assert_eq!(t.token.balance(&t.client.address), AMOUNT);
    assert_eq!(
        t.client.get_bounty(&bounty_id).unwrap().status,
        BountyStatus::Open
    );
}

// ------------------------------------------------------------ cancel_bounty

#[test]
fn test_admin_can_cancel_any_time() {
    let t = setup();
    let bounty_id = t.create();
    t.claim(bounty_id);

    t.client.cancel_bounty(&t.admin, &bounty_id);
    assert_eq!(t.last_event_topic(), symbol_short!("cancelled"));
    assert_eq!(t.token.balance(&t.creator), 1_000);
    assert_eq!(
        t.client.get_bounty(&bounty_id).unwrap().status,
        BountyStatus::Cancelled
    );

    // A cancelled bounty takes no more claims and can't be paid.
    let late = Address::generate(&t.env);
    assert_eq!(
        t.client.try_submit_claim(&bounty_id, &late, &t.s("pr")),
        Err(Ok(Error::BountyNotOpen))
    );
    assert_eq!(
        t.client.try_release(&t.admin, &bounty_id, &0),
        Err(Ok(Error::BountyNotOpen))
    );
    assert_eq!(
        t.client.try_cancel_bounty(&t.admin, &bounty_id),
        Err(Ok(Error::BountyNotOpen))
    );
}

#[test]
fn test_creator_cannot_cancel_before_deadline() {
    let t = setup();
    let bounty_id = t.create();
    assert_eq!(
        t.client.try_cancel_bounty(&t.creator, &bounty_id),
        Err(Ok(Error::CancelTooEarly))
    );
}

#[test]
fn test_creator_can_cancel_after_deadline_with_no_pending_claims() {
    let t = setup();
    let bounty_id = t.create();
    let (_, junk) = t.claim(bounty_id);
    t.client.reject_claim(&t.admin, &bounty_id, &junk);

    t.env.ledger().set_timestamp(DEADLINE);
    t.client.cancel_bounty(&t.creator, &bounty_id);
    assert_eq!(t.last_auth(), t.creator);
    assert_eq!(t.token.balance(&t.creator), 1_000);
}

#[test]
fn test_pending_claim_holds_funds_until_review_window_ends() {
    let t = setup();
    let bounty_id = t.create();
    t.claim(bounty_id);

    t.env.ledger().set_timestamp(DEADLINE);
    assert_eq!(
        t.client.try_cancel_bounty(&t.creator, &bounty_id),
        Err(Ok(Error::CancelTooEarly))
    );

    t.env
        .ledger()
        .set_timestamp(DEADLINE + REVIEW_WINDOW_SECS - 1);
    assert_eq!(
        t.client.try_cancel_bounty(&t.creator, &bounty_id),
        Err(Ok(Error::CancelTooEarly))
    );

    // Admin never reviewed: the creator gets the funds back.
    t.env.ledger().set_timestamp(DEADLINE + REVIEW_WINDOW_SECS);
    t.client.cancel_bounty(&t.creator, &bounty_id);
    assert_eq!(t.token.balance(&t.creator), 1_000);
}

#[test]
fn test_stranger_cannot_cancel() {
    let t = setup();
    let bounty_id = t.create();
    t.env.ledger().set_timestamp(DEADLINE + REVIEW_WINDOW_SECS);

    let stranger = Address::generate(&t.env);
    assert_eq!(
        t.client.try_cancel_bounty(&stranger, &bounty_id),
        Err(Ok(Error::NotAuthorized))
    );
    assert_eq!(t.token.balance(&t.client.address), AMOUNT);
}

// ----------------------------------------------------------- admin rotation

#[test]
fn test_admin_rotation_is_two_step() {
    let t = setup();
    let new_admin = Address::generate(&t.env);

    assert_eq!(t.client.try_accept_admin(), Err(Ok(Error::NoPendingAdmin)));

    t.client.propose_admin(&new_admin);
    assert_eq!(t.last_auth(), t.admin);
    // Proposing alone changes nothing.
    assert_eq!(t.client.get_admin(), Some(t.admin.clone()));
    assert_eq!(t.client.get_pending_admin(), Some(new_admin.clone()));

    t.client.accept_admin();
    assert_eq!(t.last_auth(), new_admin);
    assert_eq!(t.client.get_admin(), Some(new_admin.clone()));
    assert_eq!(t.client.get_pending_admin(), None);

    // Admin-gated calls now need the new admin's signature.
    let bounty_id = t.create();
    let (_, claim_id) = t.claim(bounty_id);
    t.client.release(&new_admin, &bounty_id, &claim_id);
    assert_eq!(t.last_auth(), new_admin);
}

// --------------------------------------------------------- release_split

#[test]
fn test_release_split_50_50() {
    let t = setup();
    let bounty_id = t.create();
    let (claimant1, claim1) = t.claim(bounty_id);
    let (claimant2, claim2) = t.claim(bounty_id);

    let mut shares = soroban_sdk::Vec::new(&t.env);
    shares.push_back((claim1, 5_000));
    shares.push_back((claim2, 5_000));

    t.client.release_split(&t.admin, &bounty_id, &shares);
    assert_eq!(t.last_auth(), t.admin);

    // Each gets 200 (AMOUNT=400 / 2)
    assert_eq!(t.token.balance(&claimant1), 200);
    assert_eq!(t.token.balance(&claimant2), 200);
    assert_eq!(t.token.balance(&t.client.address), 0);

    let bounty = t.client.get_bounty(&bounty_id).unwrap();
    assert_eq!(bounty.status, BountyStatus::Released);
    assert_eq!(
        t.client.get_claim(&bounty_id, &claim1).unwrap().status,
        ClaimStatus::Accepted
    );
    assert_eq!(
        t.client.get_claim(&bounty_id, &claim2).unwrap().status,
        ClaimStatus::Accepted
    );
}

#[test]
fn test_release_split_rounding_remainder() {
    let t = setup();
    let bounty_id = t.create();
    let (claimant1, claim1) = t.claim(bounty_id);
    let (claimant2, claim2) = t.claim(bounty_id);
    let (claimant3, claim3) = t.claim(bounty_id);

    // 33.33% each, with 1 bps leftover = first gets remainder
    let mut shares = soroban_sdk::Vec::new(&t.env);
    shares.push_back((claim1, 3_333));
    shares.push_back((claim2, 3_333));
    shares.push_back((claim3, 3_334));

    t.client.release_split(&t.admin, &bounty_id, &shares);

    // AMOUNT = 400
    // claim1: first share, gets remainder = 400 - 133 - 133 = 134
    // claim2: (400 * 3333) / 10000 = 133
    // claim3: (400 * 3334) / 10000 = 133
    assert_eq!(t.token.balance(&claimant1), 134);
    assert_eq!(t.token.balance(&claimant2), 133);
    assert_eq!(t.token.balance(&claimant3), 133);
    assert_eq!(t.token.balance(&t.client.address), 0);
}

#[test]
fn test_release_split_rejects_invalid_sum() {
    let t = setup();
    let bounty_id = t.create();
    let (_, claim1) = t.claim(bounty_id);
    let (_, claim2) = t.claim(bounty_id);

    let mut shares = soroban_sdk::Vec::new(&t.env);
    shares.push_back((claim1, 5_000));
    shares.push_back((claim2, 4_000));

    assert_eq!(
        t.client.try_release_split(&t.admin, &bounty_id, &shares),
        Err(Ok(Error::InvalidShares))
    );

    // No funds moved
    assert_eq!(t.token.balance(&t.client.address), AMOUNT);
}

#[test]
fn test_release_split_rejects_duplicate_claim() {
    let t = setup();
    let bounty_id = t.create();
    let (_, claim1) = t.claim(bounty_id);

    let mut shares = soroban_sdk::Vec::new(&t.env);
    shares.push_back((claim1, 5_000));
    shares.push_back((claim1, 5_000));

    assert_eq!(
        t.client.try_release_split(&t.admin, &bounty_id, &shares),
        Err(Ok(Error::InvalidShares))
    );
}

#[test]
fn test_release_split_rejects_non_pending_claim() {
    let t = setup();
    let bounty_id = t.create();
    let (_, claim1) = t.claim(bounty_id);
    let (_, claim2) = t.claim(bounty_id);

    // Reject claim2 first
    t.client.reject_claim(&t.admin, &bounty_id, &claim2);

    let mut shares = soroban_sdk::Vec::new(&t.env);
    shares.push_back((claim1, 5_000));
    shares.push_back((claim2, 5_000));

    assert_eq!(
        t.client.try_release_split(&t.admin, &bounty_id, &shares),
        Err(Ok(Error::ClaimNotPending))
    );
}

// ------------------------------------------------------------ fund_bounty

#[test]
fn test_fund_bounty_increases_amount() {
    let t = setup();
    let bounty_id = t.create();

    let funder2 = Address::generate(&t.env);
    StellarAssetClient::new(&t.env, &t.token.address).mint(&funder2, &500);

    t.client.fund_bounty(&funder2, &bounty_id, &100);
    assert_eq!(t.last_auth(), funder2);
    assert_eq!(t.last_event_topic(), symbol_short!("funded"));

    let bounty = t.client.get_bounty(&bounty_id).unwrap();
    assert_eq!(bounty.amount, AMOUNT + 100);
    assert_eq!(t.token.balance(&t.client.address), AMOUNT + 100);
    assert_eq!(t.token.balance(&funder2), 400);
}

#[test]
fn test_fund_bounty_tracks_multiple_funders() {
    let t = setup();
    let bounty_id = t.create();

    let funder2 = Address::generate(&t.env);
    let funder3 = Address::generate(&t.env);
    StellarAssetClient::new(&t.env, &t.token.address).mint(&funder2, &200);
    StellarAssetClient::new(&t.env, &t.token.address).mint(&funder3, &300);

    t.client.fund_bounty(&funder2, &bounty_id, &150);
    t.client.fund_bounty(&funder3, &bounty_id, &250);

    let bounty = t.client.get_bounty(&bounty_id).unwrap();
    assert_eq!(bounty.amount, AMOUNT + 150 + 250);
    assert_eq!(bounty.funders.len(), 3);
}

#[test]
fn test_cancel_bounty_refunds_all_funders_proportionally() {
    let t = setup();
    let bounty_id = t.create();

    let funder2 = Address::generate(&t.env);
    StellarAssetClient::new(&t.env, &t.token.address).mint(&funder2, &600);

    t.client.fund_bounty(&funder2, &bounty_id, &600);

    // Admin cancels
    t.client.cancel_bounty(&t.admin, &bounty_id);

    // Each funder gets back exactly what they put in
    assert_eq!(t.token.balance(&t.creator), 1_000 - AMOUNT + AMOUNT);
    assert_eq!(t.token.balance(&funder2), 600);
    assert_eq!(t.token.balance(&t.client.address), 0);
}

#[test]
fn test_fund_bounty_rejects_closed_bounty() {
    let t = setup();
    let bounty_id = t.create();
    let (_, claim_id) = t.claim(bounty_id);
    t.client.release(&t.admin, &bounty_id, &claim_id);

    let funder2 = Address::generate(&t.env);
    assert_eq!(
        t.client.try_fund_bounty(&funder2, &bounty_id, &100),
        Err(Ok(Error::BountyNotOpen))
    );
}

#[test]
fn test_fund_bounty_rejects_invalid_amount() {
    let t = setup();
    let bounty_id = t.create();
    let funder2 = Address::generate(&t.env);

    assert_eq!(
        t.client.try_fund_bounty(&funder2, &bounty_id, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        t.client.try_fund_bounty(&funder2, &bounty_id, &-50),
        Err(Ok(Error::InvalidAmount))
    );
}

// --------------------------------------------------------- extend_deadline

#[test]
fn test_extend_deadline_works() {
    let t = setup();
    let bounty_id = t.create();

    let new_deadline = DEADLINE + 7 * 24 * 60 * 60;
    t.client
        .extend_deadline(&t.creator, &bounty_id, &new_deadline);
    assert_eq!(t.last_auth(), t.creator);
    assert_eq!(t.last_event_topic(), symbol_short!("extended"));

    let bounty = t.client.get_bounty(&bounty_id).unwrap();
    assert_eq!(bounty.deadline, new_deadline);
}

#[test]
fn test_extend_deadline_rejects_non_creator() {
    let t = setup();
    let bounty_id = t.create();
    let stranger = Address::generate(&t.env);

    assert_eq!(
        t.client
            .try_extend_deadline(&stranger, &bounty_id, &(DEADLINE + 1000)),
        Err(Ok(Error::NotAuthorized))
    );
}

#[test]
fn test_extend_deadline_rejects_earlier_deadline() {
    let t = setup();
    let bounty_id = t.create();

    assert_eq!(
        t.client
            .try_extend_deadline(&t.creator, &bounty_id, &(DEADLINE - 1)),
        Err(Ok(Error::InvalidDeadline))
    );
}

#[test]
fn test_extend_deadline_rejects_beyond_max_duration() {
    let t = setup();
    let bounty_id = t.create();

    let too_late = START + MAX_BOUNTY_DURATION_SECS + 1;
    assert_eq!(
        t.client
            .try_extend_deadline(&t.creator, &bounty_id, &too_late),
        Err(Ok(Error::InvalidDeadline))
    );
}

#[test]
fn test_extend_deadline_rejects_closed_bounty() {
    let t = setup();
    let bounty_id = t.create();
    let (_, claim_id) = t.claim(bounty_id);
    t.client.release(&t.admin, &bounty_id, &claim_id);

    assert_eq!(
        t.client
            .try_extend_deadline(&t.creator, &bounty_id, &(DEADLINE + 1000)),
        Err(Ok(Error::BountyNotOpen))
    );
}

#[test]
fn test_extend_deadline_checks_auth() {
    let t = setup();
    let bounty_id = t.create();
    let new_deadline = DEADLINE + 1000;

    t.client
        .extend_deadline(&t.creator, &bounty_id, &new_deadline);
    assert_eq!(t.last_auth(), t.creator);
}

// ---------------------------------------------------- token allowlist

#[test]
fn test_allow_token_works() {
    let t = setup_with_token_allowlist(false);

    assert!(!t.client.is_token_allowed(&t.token.address));

    t.client.allow_token(&t.token.address);
    assert_eq!(t.last_auth(), t.admin);
    assert_eq!(t.last_event_topic(), symbol_short!("tkn_allow"));

    assert!(t.client.is_token_allowed(&t.token.address));
}

#[test]
fn test_disallow_token_works() {
    let t = setup();
    t.client.allow_token(&t.token.address);
    assert!(t.client.is_token_allowed(&t.token.address));

    t.client.disallow_token(&t.token.address);
    assert_eq!(t.last_auth(), t.admin);
    assert_eq!(t.last_event_topic(), symbol_short!("tkn_deny"));

    assert!(!t.client.is_token_allowed(&t.token.address));
}

#[test]
fn test_create_bounty_rejects_disallowed_token() {
    let t = setup_with_token_allowlist(false);

    let result = t.client.try_create_bounty(
        &t.creator,
        &t.s("org/repo#1"),
        &AMOUNT,
        &t.token.address,
        &DEADLINE,
        &None,
        &None,
        &None,
    );
    assert_eq!(result, Err(Ok(Error::TokenNotAllowed)));
}

#[test]
fn test_create_bounty_works_with_allowed_token() {
    let t = setup();
    t.client.allow_token(&t.token.address);

    let bounty_id = t.create();
    assert_eq!(bounty_id, 1);
}

#[test]
fn test_non_admin_cannot_change_allowlist() {
    let t = setup();

    t.env.set_auths(&[]);
    assert!(t.client.try_allow_token(&t.token.address).is_err());
    assert!(t.client.try_disallow_token(&t.token.address).is_err());
}

// ------------------------------------------------- reviewer role (#116)

#[test]
fn test_admin_can_add_and_remove_reviewer() {
    let t = setup();
    let reviewer = Address::generate(&t.env);

    assert!(!t.client.is_reviewer(&reviewer));
    t.add_reviewer(&reviewer);
    assert_eq!(t.last_auth(), t.admin);
    assert!(t.client.is_reviewer(&reviewer));

    t.client.remove_reviewer(&t.admin, &reviewer);
    assert_eq!(t.last_auth(), t.admin);
    assert!(!t.client.is_reviewer(&reviewer));
}

#[test]
fn test_reviewer_can_reject_and_release() {
    let t = setup();
    let reviewer = Address::generate(&t.env);
    t.add_reviewer(&reviewer);

    let bounty_id = t.create();
    let (_, junk) = t.claim(bounty_id);
    let (winner, real) = t.claim(bounty_id);

    t.client.reject_claim(&reviewer, &bounty_id, &junk);
    assert_eq!(t.last_auth(), reviewer);
    assert_eq!(
        t.client.get_claim(&bounty_id, &junk).unwrap().status,
        ClaimStatus::Rejected
    );

    t.client.release(&reviewer, &bounty_id, &real);
    assert_eq!(t.last_auth(), reviewer);
    assert_eq!(t.token.balance(&winner), AMOUNT);
}

#[test]
fn test_removed_reviewer_can_no_longer_review() {
    let t = setup();
    let reviewer = Address::generate(&t.env);
    t.add_reviewer(&reviewer);
    t.client.remove_reviewer(&t.admin, &reviewer);

    let bounty_id = t.create();
    let (_, claim_id) = t.claim(bounty_id);
    assert_eq!(
        t.client.try_release(&reviewer, &bounty_id, &claim_id),
        Err(Ok(Error::NotAuthorized))
    );
    assert_eq!(
        t.client.try_reject_claim(&reviewer, &bounty_id, &claim_id),
        Err(Ok(Error::NotAuthorized))
    );
}

#[test]
fn test_permission_matrix() {
    let t = setup();
    let reviewer = Address::generate(&t.env);
    let stranger = Address::generate(&t.env);
    t.add_reviewer(&reviewer);

    // --- admin-only functions: admin ok, reviewer/stranger NotAuthorized.
    assert!(t.client.try_set_paused(&reviewer, &true).is_err());
    assert!(t.client.try_set_paused(&stranger, &true).is_err());
    assert!(t.client.try_allow_token(&reviewer, &t.token.address).is_err());
    assert!(t
        .client
        .try_allow_token(&stranger, &t.token.address)
        .is_err());
    assert!(t
        .client
        .try_disallow_token(&reviewer, &t.token.address)
        .is_err());
    assert!(t
        .client
        .try_disallow_token(&stranger, &t.token.address)
        .is_err());
    assert!(t.client.try_add_reviewer(&reviewer, &stranger).is_err());
    assert!(t.client.try_add_reviewer(&stranger, &stranger).is_err());
    assert!(t.client.try_remove_reviewer(&reviewer, &reviewer).is_err());
    assert!(t.client.try_remove_reviewer(&stranger, &reviewer).is_err());
    assert!(t.client.try_propose_admin(&reviewer, &stranger).is_err());
    assert!(t.client.try_propose_admin(&stranger, &stranger).is_err());

    // Admin succeeds on the same calls.
    t.client.set_paused(&t.admin, &true);
    t.client.set_paused(&t.admin, &false);
    t.client.allow_token(&t.admin, &t.token.address);
    t.client.disallow_token(&t.admin, &t.token.address);
    t.client.allow_token(&t.admin, &t.token.address);

    // --- review functions: admin and reviewer ok, stranger NotAuthorized.
    let bounty_id = t.create();
    let (_, junk) = t.claim(bounty_id);
    let (_, real) = t.claim(bounty_id);

    assert_eq!(
        t.client.try_reject_claim(&stranger, &bounty_id, &junk),
        Err(Ok(Error::NotAuthorized))
    );
    assert_eq!(
        t.client.try_release(&stranger, &bounty_id, &real),
        Err(Ok(Error::NotAuthorized))
    );
    t.client.reject_claim(&reviewer, &bounty_id, &junk);
    t.client.release(&t.admin, &bounty_id, &real);

    // release_split is admin-only.
    let bounty_id = t.create();
    let (_, c1) = t.claim(bounty_id);
    let (_, c2) = t.claim(bounty_id);
    let mut shares = soroban_sdk::Vec::new(&t.env);
    shares.push_back((c1, 5_000));
    shares.push_back((c2, 5_000));
    assert_eq!(
        t.client.try_release_split(&reviewer, &bounty_id, &shares),
        Err(Ok(Error::NotAuthorized))
    );
    assert_eq!(
        t.client.try_release_split(&stranger, &bounty_id, &shares),
        Err(Ok(Error::NotAuthorized))
    );
    t.client.release_split(&t.admin, &bounty_id, &shares);

    // --- creator-only functions: reviewer/stranger are not the creator.
    let bounty_id = t.create();
    assert_eq!(
        t.client
            .try_extend_deadline(&reviewer, &bounty_id, &(DEADLINE + 1000)),
        Err(Ok(Error::NotAuthorized))
    );
    assert_eq!(
        t.client
            .try_extend_deadline(&stranger, &bounty_id, &(DEADLINE + 1000)),
        Err(Ok(Error::NotAuthorized))
    );

    // --- cancel: admin always, stranger never.
    assert_eq!(
        t.client.try_cancel_bounty(&stranger, &bounty_id),
        Err(Ok(Error::NotAuthorized))
    );
    t.client.cancel_bounty(&t.admin, &bounty_id);
}

// ------------------------------------------- balance invariant (#118)

/// Deterministic LCG so failures are reproducible from the printed seed.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[test]
fn test_balance_invariant_random_sequences() {
    const SEED: u64 = 0xDEAD_BEEF_1234_5678;
    const STEPS: usize = 300;

    let t = setup();
    let reviewer = Address::generate(&t.env);
    t.add_reviewer(&reviewer);

    // Fund the creator generously so creates never fail for lack of funds.
    StellarAssetClient::new(&t.env, &t.token.address).mint(&t.creator, &1_000_000);

    let mut rng = Lcg(SEED);
    let mut open: std::vec::Vec<(u64, i128)> = std::vec::Vec::new();
    let mut next_id: u64 = 1;

    let expected: i128 = |open: &std::vec::Vec<(u64, i128)>| open.iter().map(|(_, a)| *a).sum();

    for step in 0..STEPS {
        let action = rng.below(5);
        match action {
            // create
            0 => {
                let amount = 10 + (rng.below(90) as i128);
                let id = t
                    .client
                    .create_bounty(
                        &t.creator,
                        &t.s("org/repo#inv"),
                        &amount,
                        &t.token.address,
                        &DEADLINE,
                        &None,
                        &None,
                        &None,
                    );
                assert_eq!(id, next_id, "seed {SEED} step {step}");
                next_id += 1;
                open.push((id, amount));
            }
            // claim
            1 => {
                if !open.is_empty() {
                    let idx = rng.below(open.len() as u64) as usize;
                    let (id, _) = open[idx];
                    let claimant = Address::generate(&t.env);
                    let _ = t
                        .client
                        .try_submit_claim(&id, &claimant, &t.s("org/repo#inv-pr"));
                }
            }
            // reject a claim (reviewer)
            2 => {
                if !open.is_empty() {
                    let idx = rng.below(open.len() as u64) as usize;
                    let (id, _) = open[idx];
                    let bounty = t.client.get_bounty(&id).unwrap();
                    if bounty.claim_count > 0 {
                        let cid = rng.below(bounty.claim_count as u64) as u32;
                        let _ = t.client.try_reject_claim(&reviewer, &id, &cid);
                    }
                }
            }
            // release a claim (reviewer)
            3 => {
                if !open.is_empty() {
                    let idx = rng.below(open.len() as u64) as usize;
                    let (id, _) = open[idx];
                    let bounty = t.client.get_bounty(&id).unwrap();
                    if bounty.pending_claims > 0 {
                        // Find a pending claim id.
                        let mut chosen = None;
                        for cid in 0..bounty.claim_count {
                            if let Some(c) = t.client.get_claim(&id, &cid) {
                                if c.status == ClaimStatus::Pending {
                                    chosen = Some(cid);
                                    break;
                                }
                            }
                        }
                        if let Some(cid) = chosen {
                            t.client.release(&reviewer, &id, &cid);
                            open.remove(idx);
                        }
                    }
                }
            }
            // cancel (admin)
            _ => {
                if !open.is_empty() {
                    let idx = rng.below(open.len() as u64) as usize;
                    let (id, _) = open[idx];
                    t.client.cancel_bounty(&t.admin, &id);
                    open.remove(idx);
                }
            }
        }

        let balance = t.token.balance(&t.client.address);
        assert_eq!(
            balance,
            expected(&open),
            "balance invariant broken at seed {SEED} step {step}"
        );
    }
}

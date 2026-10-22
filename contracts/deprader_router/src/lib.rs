#![no_std]

//! # deprader_router
//!
//! On-chain bounty router for DepRadar. A creator locks funds against a
//! security finding, researchers submit claims with a reference to their
//! proof, and the admin either releases the bounty to one specific claim or
//! rejects bad claims. If nobody is paid, the creator can reclaim the funds
//! once the bounty's deadline (plus a review window) has passed.
//!
//! ```text
//! create_bounty ──► Open ──submit_claim──► claim: Pending
//!                    │                        ├─ reject_claim → Rejected (bounty stays Open)
//!                    │                        └─ release      → Accepted, bounty Released
//!                    └─ cancel_bounty ──► Cancelled (funds back to creator)
//! ```

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env,
    IntoVal, String, Symbol, TryFromVal, Val,
};

/// Ledgers per day at Stellar's ~5s close time.
const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_TTL_THRESHOLD: u32 = 29 * DAY_IN_LEDGERS;
const INSTANCE_TTL_EXTEND_TO: u32 = 30 * DAY_IN_LEDGERS;
const PERSISTENT_TTL_THRESHOLD: u32 = 60 * DAY_IN_LEDGERS;
const PERSISTENT_TTL_EXTEND_TO: u32 = 90 * DAY_IN_LEDGERS;

/// Seconds after the deadline during which the admin can still review
/// pending claims before the creator may reclaim the funds.
pub const REVIEW_WINDOW_SECS: u64 = 7 * 24 * 60 * 60;
/// Longest a bounty may stay open for claims.
pub const MAX_BOUNTY_DURATION_SECS: u64 = 365 * 24 * 60 * 60;
/// Upper bound on claims per bounty, so one bounty can't be spammed into
/// unbounded storage.
pub const MAX_CLAIMS_PER_BOUNTY: u32 = 50;
/// Upper bound on the number of claims indexed per claimant. The per-claimant
/// index is a single persistent vector, so this cap keeps it bounded in size;
/// once a claimant reaches it, further `submit_claim` calls fail with
/// [`Error::TooManyClaims`].
pub const MAX_CLAIMS_PER_CLAIMANT: u32 = 200;
/// Upper bound (in bytes) for `issue_ref` / `proof_ref`.
pub const MAX_REF_LEN: u32 = 256;
/// Upper bound (in bytes) for the optional `package` metadata field.
pub const MAX_PACKAGE_LEN: u32 = 128;
/// Upper bound (in bytes) for the optional `ecosystem` metadata field.
pub const MAX_ECOSYSTEM_LEN: u32 = 32;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BountyStatus {
    Open,
    Released,
    Cancelled,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClaimStatus {
    Pending,
    Rejected,
    Accepted,
    Withdrawn,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bounty {
    pub creator: Address,
    pub issue_ref: String,
    /// Optional ecosystem the finding belongs to (e.g. `soroban`).
    pub ecosystem: Option<Symbol>,
    /// Optional package name the finding affects.
    pub package: Option<String>,
    /// Optional severity score for the finding.
    pub severity: Option<u32>,
    pub amount: i128,
    pub token: Address,
    pub status: BountyStatus,
    /// Ledger timestamp the bounty was created at.
    pub created_at: u64,
    /// Ledger timestamp after which no new claims are accepted.
    pub deadline: u64,
    /// Total claims ever submitted; claim IDs are `0..claim_count`.
    pub claim_count: u32,
    /// Claims still awaiting an admin decision.
    pub pending_claims: u32,
    /// Track contributions per funder for proportional refunds.
    pub funders: soroban_sdk::Vec<(Address, i128)>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Claim {
    pub claimant: Address,
    pub proof_ref: String,
    pub submitted_at: u64,
    pub status: ClaimStatus,
}

#[contracttype]
enum DataKey {
    Admin,
    PendingAdmin,
    NextBountyId,
    Bounty(u64),
    Claim(u64, u32),
    ClaimBy(u64, Address),
    /// Per-claimant index of `(bounty_id, claim_id)` pairs, in submission
    /// order, capped at [`MAX_CLAIMS_PER_CLAIMANT`].
    ClaimsOf(Address),
    AllowedToken(Address),
    Reviewer(Address),
    Paused,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Error {
    InvalidAmount = 1,
    BountyNotFound = 3,
    AlreadyInitialized = 5,
    NotInitialized = 6,
    NotAuthorized = 7,
    InvalidDeadline = 10,
    InvalidReference = 11,
    BountyNotOpen = 12,
    ClaimWindowClosed = 13,
    DuplicateClaim = 14,
    TooManyClaims = 15,
    ClaimNotFound = 16,
    ClaimNotPending = 17,
    CancelTooEarly = 18,
    NoPendingAdmin = 19,
    InvalidShares = 20,
    TokenNotAllowed = 21,
    Paused = 22,
}

#[contract]
pub struct DepRaderRouter;

#[contractimpl]
impl DepRaderRouter {
    // ---------------------------------------------------------------- admin

    /// Pauses or resumes operations that can lock or distribute bounty funds.
    pub fn set_paused(env: Env, paused: bool) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &paused);
        bump_instance(&env);
        env.events()
            .publish((symbol_short!("pause"),), (paused, admin));
        Ok(())
    }

    /// Returns whether bounty operations are paused. New deployments start
    /// unpaused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Sets the contract admin. Must be called once before any bounty can be
    /// created, so funds are never locked in a contract nobody can resolve.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        bump_instance(&env);

        env.events().publish((symbol_short!("admin_set"),), admin);
        Ok(())
    }

    /// First step of an admin rotation: the current admin nominates a
    /// successor. Nothing changes until the successor accepts.
    pub fn propose_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);

        env.events()
            .publish((symbol_short!("admin_prp"), admin), new_admin);
        Ok(())
    }

    /// Second step of an admin rotation: the nominee accepts and becomes the
    /// admin. Requiring the nominee's signature means a mistyped address
    /// can't lock everyone out of the contract.
    pub fn accept_admin(env: Env) -> Result<(), Error> {
        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(Error::NoPendingAdmin)?;
        pending.require_auth();

        env.storage().instance().set(&DataKey::Admin, &pending);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        bump_instance(&env);

        env.events().publish((symbol_short!("admin_set"),), pending);
        Ok(())
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Admin)
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PendingAdmin)
    }

    // ------------------------------------------------------------ reviewers

    /// Admin-only: grants `reviewer` permission to reject claims and release
    /// bounties without handing over full admin control.
    pub fn add_reviewer(env: Env, reviewer: Address) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        set_persistent(&env, &DataKey::Reviewer(reviewer.clone()), &true);
        env.events()
            .publish((symbol_short!("rev_added"),), (reviewer, admin));
        Ok(())
    }

    /// Admin-only: revokes a reviewer's permission.
    pub fn remove_reviewer(env: Env, reviewer: Address) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        env.storage()
            .persistent()
            .remove(&DataKey::Reviewer(reviewer.clone()));
        env.events()
            .publish((symbol_short!("rev_rmvd"),), (reviewer, admin));
        Ok(())
    }

    /// Returns whether `reviewer` is currently registered as a reviewer.
    pub fn is_reviewer(env: Env, reviewer: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Reviewer(reviewer))
            .unwrap_or(false)
    }

    // --------------------------------------------------- token allowlist

    /// Admin-only: adds a token to the allowlist.
    pub fn allow_token(env: Env, token: Address) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        set_persistent(&env, &DataKey::AllowedToken(token.clone()), &true);
        env.events()
            .publish((symbol_short!("tkn_allow"),), (token, admin));
        Ok(())
    }

    /// Admin-only: removes a token from the allowlist.
    pub fn disallow_token(env: Env, token: Address) -> Result<(), Error> {
        let admin = require_admin(&env)?;
        env.storage()
            .persistent()
            .remove(&DataKey::AllowedToken(token.clone()));
        env.events()
            .publish((symbol_short!("tkn_deny"),), (token, admin));
        Ok(())
    }

    /// Checks if a token is on the allowlist.
    pub fn is_token_allowed(env: Env, token: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::AllowedToken(token))
            .unwrap_or(false)
    }

    // ------------------------------------------------------------- bounties

    /// Locks `amount` of `token` from `creator` against a new bounty and
    /// returns its contract-assigned ID. Claims are accepted until
    /// `deadline` (a ledger timestamp, in seconds).
    ///
    /// `ecosystem`, `package` and `severity` are optional structured finding
    /// metadata; when provided they are validated and stored alongside the
    /// free-text `issue_ref`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_bounty(
        env: Env,
        creator: Address,
        issue_ref: String,
        ecosystem: Option<Symbol>,
        package: Option<String>,
        severity: Option<u32>,
        amount: i128,
        token: Address,
        deadline: u64,
    ) -> Result<u64, Error> {
        creator.require_auth();

        if Self::is_paused(env.clone()) {
            return Err(Error::Paused);
        }

        stored_admin(&env)?;
        if !Self::is_token_allowed(env.clone(), token.clone()) {
            return Err(Error::TokenNotAllowed);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        check_ref(&issue_ref)?;
        check_metadata(&ecosystem, &package)?;
        let now = env.ledger().timestamp();
        if deadline <= now || deadline - now > MAX_BOUNTY_DURATION_SECS {
            return Err(Error::InvalidDeadline);
        }

        let bounty_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextBountyId)
            .unwrap_or(1);
        env.storage()
            .instance()
            .set(&DataKey::NextBountyId, &(bounty_id + 1));
        bump_instance(&env);

        let mut funders = soroban_sdk::Vec::new(&env);
        funders.push_back((creator.clone(), amount));

        let bounty = Bounty {
            creator: creator.clone(),
            issue_ref,
            ecosystem,
            package,
            severity,
            amount,
            token: token.clone(),
            status: BountyStatus::Open,
            created_at: now,
            deadline,
            claim_count: 0,
            pending_claims: 0,
            funders,
        };
        set_persistent(&env, &DataKey::Bounty(bounty_id), &bounty);

        token::Client::new(&env, &token).transfer(
            &creator,
            &env.current_contract_address(),
            &amount,
        );

        env.events().publish(
            (symbol_short!("created"), bounty_id),
            (creator, token, amount, deadline),
        );
        Ok(bounty_id)
    }

    /// Reads back the stored state for a bounty, if one exists.
    pub fn get_bounty(env: Env, bounty_id: u64) -> Option<Bounty> {
        env.storage().persistent().get(&DataKey::Bounty(bounty_id))
    }

    /// Number of bounties created so far. Bounty IDs are `1..=bounty_count`.
    pub fn bounty_count(env: Env) -> u64 {
        let next: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextBountyId)
            .unwrap_or(1);
        next - 1
    }

    /// Adds funds to an existing open bounty in the same token.
    pub fn fund_bounty(
        env: Env,
        funder: Address,
        bounty_id: u64,
        amount: i128,
    ) -> Result<(), Error> {
        funder.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let key = DataKey::Bounty(bounty_id);
        let mut bounty: Bounty = get_persistent(&env, &key).ok_or(Error::BountyNotFound)?;
        if bounty.status != BountyStatus::Open {
            return Err(Error::BountyNotOpen);
        }

        // Track this contribution for proportional refunds
        let mut found = false;
        for i in 0..bounty.funders.len() {
            let (addr, existing) = bounty.funders.get(i).unwrap();
            if addr == funder {
                bounty.funders.set(i, (addr, existing + amount));
                found = true;
                break;
            }
        }
        if !found {
            bounty.funders.push_back((funder.clone(), amount));
        }

        bounty.amount += amount;
        set_persistent(&env, &key, &bounty);

        token::Client::new(&env, &bounty.token).transfer(
            &funder,
            &env.current_contract_address(),
            &amount,
        );

        env.events()
            .publish((symbol_short!("funded"), bounty_id), (funder, amount));
        Ok(())
    }

    /// Extends the deadline of an open bounty. Only the creator can extend,
    /// and only to a later time within MAX_BOUNTY_DURATION_SECS of created_at.
    pub fn extend_deadline(
        env: Env,
        creator: Address,
        bounty_id: u64,
        new_deadline: u64,
    ) -> Result<(), Error> {
        creator.require_auth();

        let key = DataKey::Bounty(bounty_id);
        let mut bounty: Bounty = get_persistent(&env, &key).ok_or(Error::BountyNotFound)?;
        if bounty.status != BountyStatus::Open {
            return Err(Error::BountyNotOpen);
        }
        if bounty.creator != creator {
            return Err(Error::NotAuthorized);
        }
        if new_deadline <= bounty.deadline {
            return Err(Error::InvalidDeadline);
        }
        if new_deadline - bounty.created_at > MAX_BOUNTY_DURATION_SECS {
            return Err(Error::InvalidDeadline);
        }

        bounty.deadline = new_deadline;
        set_persistent(&env, &key, &bounty);

        env.events().publish(
            (symbol_short!("extended"), bounty_id),
            (creator, new_deadline),
        );
        Ok(())
    }

    /// Refunds an open bounty to its creator.
    ///
    /// - The admin may cancel at any time (e.g. the finding was invalid).
    /// - The creator may cancel once the deadline has passed with no pending
    ///   claims, or once the review window after the deadline has also
    ///   passed. Funds can never be locked forever, but a creator also can't
    ///   pull them out from under a claimant who is still awaiting review.
    pub fn cancel_bounty(env: Env, caller: Address, bounty_id: u64) -> Result<(), Error> {
        caller.require_auth();

        let key = DataKey::Bounty(bounty_id);
        let mut bounty: Bounty = get_persistent(&env, &key).ok_or(Error::BountyNotFound)?;
        if bounty.status != BountyStatus::Open {
            return Err(Error::BountyNotOpen);
        }

        let is_admin = stored_admin(&env)? == caller;
        // Admin can cancel even when paused to return funds
        if !is_admin && Self::is_paused(env.clone()) {
            return Err(Error::Paused);
        }

        if !is_admin {
            if caller != bounty.creator {
                return Err(Error::NotAuthorized);
            }
            let now = env.ledger().timestamp();
            let deadline_passed = now >= bounty.deadline;
            let review_over = now >= bounty.deadline.saturating_add(REVIEW_WINDOW_SECS);
            if !(review_over || (deadline_passed && bounty.pending_claims == 0)) {
                return Err(Error::CancelTooEarly);
            }
        }

        // State first, transfer last.
        bounty.status = BountyStatus::Cancelled;
        set_persistent(&env, &key, &bounty);

        // Refund each funder proportionally
        let token_client = token::Client::new(&env, &bounty.token);
        for i in 0..bounty.funders.len() {
            let (funder, amount) = bounty.funders.get(i).unwrap();
            token_client.transfer(&env.current_contract_address(), &funder, &amount);
        }

        env.events().publish(
            (symbol_short!("cancelled"), bounty_id),
            (caller, bounty.amount),
        );
        Ok(())
    }

    // --------------------------------------------------------------- claims

    /// Records a claim against an open bounty and returns its claim ID.
    /// Moves no funds. Any number of researchers may claim the same bounty
    /// (up to [`MAX_CLAIMS_PER_BOUNTY`], one claim per address), so a junk
    /// claim submitted first can't block a real one.
    pub fn submit_claim(
        env: Env,
        bounty_id: u64,
        claimant: Address,
        proof_ref: String,
    ) -> Result<u32, Error> {
        claimant.require_auth();

        if Self::is_paused(env.clone()) {
            return Err(Error::Paused);
        }

        check_ref(&proof_ref)?;

        let key = DataKey::Bounty(bounty_id);
        let mut bounty: Bounty = get_persistent(&env, &key).ok_or(Error::BountyNotFound)?;
        if bounty.status != BountyStatus::Open {
            return Err(Error::BountyNotOpen);
        }
        let now = env.ledger().timestamp();
        if now >= bounty.deadline {
            return Err(Error::ClaimWindowClosed);
        }
        let by_key = DataKey::ClaimBy(bounty_id, claimant.clone());
        if env.storage().persistent().has(&by_key) {
            return Err(Error::DuplicateClaim);
        }
        if bounty.claim_count >= MAX_CLAIMS_PER_BOUNTY {
            return Err(Error::TooManyClaims);
        }

        let claim_id = bounty.claim_count;
        let claim = Claim {
            claimant: claimant.clone(),
            proof_ref,
            submitted_at: now,
            status: ClaimStatus::Pending,
        };
        set_persistent(&env, &DataKey::Claim(bounty_id, claim_id), &claim);
        set_persistent(&env, &by_key, &claim_id);

        bounty.claim_count += 1;
        bounty.pending_claims += 1;
        set_persistent(&env, &key, &bounty);

        // Index the claim under its claimant for profile lookups.
        let claims_key = DataKey::ClaimsOf(claimant.clone());
        let mut claims: soroban_sdk::Vec<(u64, u32)> =
            get_persistent(&env, &claims_key).unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        if claims.len() >= MAX_CLAIMS_PER_CLAIMANT {
            return Err(Error::TooManyClaims);
        }
        claims.push_back((bounty_id, claim_id));
        set_persistent(&env, &claims_key, &claims);

        env.events()
            .publish((symbol_short!("claimed"), bounty_id), (claim_id, claimant));
        Ok(claim_id)
    }

    /// Reads back one claim on a bounty, if it exists.
    pub fn get_claim(env: Env, bounty_id: u64, claim_id: u32) -> Option<Claim> {
        env.storage()
            .persistent()
            .get(&DataKey::Claim(bounty_id, claim_id))
    }

    /// Looks up the claim ID `claimant` holds on a bounty, if any.
    pub fn get_claim_id(env: Env, bounty_id: u64, claimant: Address) -> Option<u32> {
        env.storage()
            .persistent()
            .get(&DataKey::ClaimBy(bounty_id, claimant))
    }

    /// Returns a page of the `(bounty_id, claim_id)` pairs claimed by
    /// `claimant`, in submission order.
    ///
    /// The per-claimant index is capped at [`MAX_CLAIMS_PER_CLAIMANT`]
    /// entries; once a claimant hits the cap, new claims are rejected with
    /// [`Error::TooManyClaims`]. Pagination is zero-based: `start` is the
    /// index of the first entry to return and `limit` is the maximum number
    /// of entries to return. If `start` is past the end of the index or
    /// `limit` is zero, an empty vector is returned. The result is the slice
    /// `[start, min(start + limit, len))`.
    pub fn get_claims_of(
        env: Env,
        claimant: Address,
        start: u32,
        limit: u32,
    ) -> soroban_sdk::Vec<(u64, u32)> {
        let claims: soroban_sdk::Vec<(u64, u32)> = env
            .storage()
            .persistent()
            .get(&DataKey::ClaimsOf(claimant))
            .unwrap_or_else(|| soroban_sdk::Vec::new(&env));

        let len = claims.len();
        let mut out = soroban_sdk::Vec::new(&env);
        if limit == 0 || start >= len {
            return out;
        }

        let end = start.saturating_add(limit).min(len);
        for i in start..end {
            out.push_back(claims.get(i).unwrap());
        }
        out
    }

    /// Allows a claimant to withdraw their own pending claim.
    pub fn withdraw_claim(
        env: Env,
        claimant: Address,
        bounty_id: u64,
        claim_id: u32,
    ) -> Result<(), Error> {
        claimant.require_auth();

        let key = DataKey::Bounty(bounty_id);
        let mut bounty: Bounty = get_persistent(&env, &key).ok_or(Error::BountyNotFound)?;

        let claim_key = DataKey::Claim(bounty_id, claim_id);
        let mut claim: Claim = get_persistent(&env, &claim_key).ok_or(Error::ClaimNotFound)?;

        if claim.claimant != claimant {
            return Err(Error::NotAuthorized);
        }
        if claim.status != ClaimStatus::Pending {
            return Err(Error::ClaimNotPending);
        }

        claim.status = ClaimStatus::Withdrawn;
        set_persistent(&env, &claim_key, &claim);
        bounty.pending_claims -= 1;
        set_persistent(&env, &key, &bounty);

        env.events().publish(
            (symbol_short!("withdrawn"), bounty_id),
            (claim_id, claimant),
        );
        Ok(())
    }

    /// Admin- or reviewer-only: marks a pending claim as rejected. The bounty
    /// stays open for other claims.
    pub fn reject_claim(
        env: Env,
        caller: Address,
        bounty_id: u64,
        claim_id: u32,
    ) -> Result<(), Error> {
        let caller = require_admin_or_reviewer(&env, caller)?;
        let (mut bounty, mut claim) = load_open_pending(&env, bounty_id, claim_id)?;

        claim.status = ClaimStatus::Rejected;
        set_persistent(&env, &DataKey::Claim(bounty_id, claim_id), &claim);
        bounty.pending_claims -= 1;
        set_persistent(&env, &DataKey::Bounty(bounty_id), &bounty);

        env.events()
            .publish((symbol_short!("rejected"), bounty_id), (claim_id, caller));
        Ok(())
    }

    /// Admin- or reviewer-only: pays the locked funds to the chosen claim's
    /// claimant and closes the bounty. Works after the deadline too, so claims
    /// submitted in time can still be paid during the review window.
    pub fn release(
        env: Env,
        caller: Address,
        bounty_id: u64,
        claim_id: u32,
    ) -> Result<(), Error> {
        let caller = require_admin_or_reviewer(&env, caller)?;

        if Self::is_paused(env.clone()) {
            return Err(Error::Paused);
        }

        let (mut bounty, mut claim) = load_open_pending(&env, bounty_id, claim_id)?;

        // State first, transfer last.
        claim.status = ClaimStatus::Accepted;
        set_persistent(&env, &DataKey::Claim(bounty_id, claim_id), &claim);
        bounty.status = BountyStatus::Released;
        bounty.pending_claims -= 1;
        set_persistent(&env, &DataKey::Bounty(bounty_id), &bounty);

        token::Client::new(&env, &bounty.token).transfer(
            &env.current_contract_address(),
            &claim.claimant,
            &bounty.amount,
        );

        env.events().publish(
            (symbol_short!("released"), bounty_id),
            (claim_id, claim.claimant, bounty.amount, caller),
        );
        Ok(())
    }

    /// Admin-only: splits the bounty payout across multiple claimants.
    /// `shares` is a vector of (claim_id, basis_points) that must sum to 10_000.
    /// Rounding remainder goes to the first share.
    pub fn release_split(
        env: Env,
        bounty_id: u64,
        shares: soroban_sdk::Vec<(u32, u32)>,
    ) -> Result<(), Error> {
        let admin = require_admin(&env)?;

        let key = DataKey::Bounty(bounty_id);
        let mut bounty: Bounty = get_persistent(&env, &key).ok_or(Error::BountyNotFound)?;
        if bounty.status != BountyStatus::Open {
            return Err(Error::BountyNotOpen);
        }

        // Validate shares sum to 10_000 and are distinct pending claims
        let mut total_bps: u32 = 0;
        let mut seen_claims = soroban_sdk::Vec::new(&env);
        for i in 0..shares.len() {
            let (claim_id, bps) = shares.get(i).unwrap();
            total_bps = total_bps.checked_add(bps).ok_or(Error::InvalidShares)?;

            // Check for duplicates
            for j in 0..seen_claims.len() {
                if seen_claims.get(j).unwrap() == claim_id {
                    return Err(Error::InvalidShares);
                }
            }
            seen_claims.push_back(claim_id);

            // Verify claim is pending
            let claim: Claim = get_persistent(&env, &DataKey::Claim(bounty_id, claim_id))
                .ok_or(Error::ClaimNotFound)?;
            if claim.status != ClaimStatus::Pending {
                return Err(Error::ClaimNotPending);
            }
        }

        if total_bps != 10_000 {
            return Err(Error::InvalidShares);
        }

        // Calculate and distribute payouts
        let token_client = token::Client::new(&env, &bounty.token);
        let mut total_base: i128 = 0;
        for i in 0..shares.len() {
            let (_, bps) = shares.get(i).unwrap();
            total_base += (bounty.amount * (bps as i128)) / 10_000;
        }
        let rounding_remainder = bounty.amount - total_base;

        for i in 0..shares.len() {
            let (claim_id, bps) = shares.get(i).unwrap();
            let mut claim: Claim =
                get_persistent(&env, &DataKey::Claim(bounty_id, claim_id)).unwrap();

            let mut payout = (bounty.amount * (bps as i128)) / 10_000;
            if i == 0 {
                // The first share receives the rounding remainder only.
                payout += rounding_remainder;
            }

            claim.status = ClaimStatus::Accepted;
            set_persistent(&env, &DataKey::Claim(bounty_id, claim_id), &claim);

            token_client.transfer(&env.current_contract_address(), &claim.claimant, &payout);

            env.events().publish(
                (symbol_short!("released"), bounty_id),
                (claim_id, claim.claimant.clone(), payout, admin.clone()),
            );
        }

        bounty.status = BountyStatus::Released;
        bounty.pending_claims -= shares.len();
        set_persistent(&env, &key, &bounty);

        Ok(())
    }
}

// ------------------------------------------------------------------ helpers

fn stored_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn require_admin(env: &Env) -> Result<Address, Error> {
    let admin = stored_admin(env)?;
    admin.require_auth();
    bump_instance(env);
    Ok(admin)
}

/// Requires `caller`'s auth and checks it is either the stored admin or a
/// registered reviewer. Returns the caller on success.
fn require_admin_or_reviewer(env: &Env, caller: Address) -> Result<Address, Error> {
    caller.require_auth();
    let admin = stored_admin(env)?;
    if caller == admin {
        bump_instance(env);
        return Ok(caller);
    }
    let is_reviewer: bool = env
        .storage()
        .persistent()
        .get(&DataKey::Reviewer(caller.clone()))
        .unwrap_or(false);
    if !is_reviewer {
        return Err(Error::NotAuthorized);
    }
    bump_instance(env);
    Ok(caller)
}

/// Loads a bounty that must be `Open` and one of its claims that must be
/// `Pending` — the precondition shared by `reject_claim` and `release`.
fn load_open_pending(env: &Env, bounty_id: u64, claim_id: u32) -> Result<(Bounty, Claim), Error> {
    let bounty: Bounty =
        get_persistent(env, &DataKey::Bounty(bounty_id)).ok_or(Error::BountyNotFound)?;
    if bounty.status != BountyStatus::Open {
        return Err(Error::BountyNotOpen);
    }
    let claim: Claim =
        get_persistent(env, &DataKey::Claim(bounty_id, claim_id)).ok_or(Error::ClaimNotFound)?;
    if claim.status != ClaimStatus::Pending {
        return Err(Error::ClaimNotPending);
    }
    Ok((bounty, claim))
}

fn check_ref(r: &String) -> Result<(), Error> {
    if r.is_empty() || r.len() > MAX_REF_LEN {
        return Err(Error::InvalidReference);
    }
    Ok(())
}

/// Validates the optional structured finding metadata on a bounty.
fn check_metadata(ecosystem: &Option<Symbol>, package: &Option<String>) -> Result<(), Error> {
    if let Some(sym) = ecosystem {
        if sym.len() == 0 || sym.len() > MAX_ECOSYSTEM_LEN {
            return Err(Error::InvalidReference);
        }
    }
    if let Some(pkg) = package {
        if pkg.is_empty() || pkg.len() > MAX_PACKAGE_LEN {
            return Err(Error::InvalidReference);
        }
    }
    Ok(())
}

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
}

/// Writes a persistent entry and extends its TTL so live bounties and
/// claims don't get archived mid-lifecycle.
fn set_persistent<V: IntoVal<Env, Val>>(env: &Env, key: &DataKey, value: &V) {
    env.storage().persistent().set(key, value);
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND_TO);
}

/// Reads a persistent entry inside a state-changing call, extending its TTL
/// if present. Public getters read without extending.
fn get_persistent<V: TryFromVal<Env, Val>>(env: &Env, key: &DataKey) -> Option<V> {
    let value = env.storage().persistent().get(key);
    if value.is_some() {
        env.storage().persistent().extend_ttl(
            key,
            PERSISTENT_TTL_THRESHOLD,
            PERSISTENT_TTL_EXTEND_TO,
        );
    }
    value
}

#[cfg(test)]
mod test;

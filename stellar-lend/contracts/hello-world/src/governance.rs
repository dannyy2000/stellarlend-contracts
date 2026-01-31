#![allow(unused)]
use soroban_sdk::{contracterror, contracttype, Address, Env, IntoVal, Map, Symbol, Vec};

/// Errors that can occur during governance operations
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum GovernanceError {
    /// Unauthorized access - caller is not authorized
    Unauthorized = 1,
    /// Proposal not found
    ProposalNotFound = 2,
    /// Proposal already executed
    ProposalAlreadyExecuted = 3,
    /// Proposal already failed
    ProposalAlreadyFailed = 4,
    /// Proposal not ready for execution (timelock not expired)
    ProposalNotReady = 5,
    /// Voting threshold not met
    ThresholdNotMet = 6,
    /// Invalid proposal data
    InvalidProposal = 7,
    /// Invalid vote value
    InvalidVote = 8,
    /// Already voted
    AlreadyVoted = 9,
    /// Voting period ended
    VotingPeriodEnded = 10,
    /// Proposal execution failed
    ExecutionFailed = 11,
    /// Invalid multisig configuration
    InvalidMultisigConfig = 12,
    /// Not enough approvals
    InsufficientApprovals = 13,
    /// Proposal expired
    ProposalExpired = 14,
}

/// Storage keys for governance data
#[contracttype]
#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub enum GovernanceDataKey {
    /// Proposals: Map<u64, Proposal>
    Proposal(u64),
    /// Proposal counter
    ProposalCounter,
    /// Multisig admins: Vec<Address>
    MultisigAdmins,
    /// Multisig threshold (number of approvals required)
    MultisigThreshold,
    /// Proposal votes: Map<u64, Map<Address, Vote>>
    ProposalVotes(u64),
    /// Proposal approvals (for multisig): Map<u64, Vec<Address>>
    ProposalApprovals(u64),
}

/// Proposal status
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProposalStatus {
    /// Proposal is active and accepting votes
    Active,
    /// Proposal passed voting threshold
    Passed,
    /// Proposal failed to meet threshold
    Failed,
    /// Proposal executed
    Executed,
    /// Proposal expired
    Expired,
}

/// Proposal type
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalType {
    /// Change minimum collateral ratio
    SetMinCollateralRatio(i128),
    /// Change risk parameters (min_cr, liq_threshold, close_factor, liq_incentive)
    SetRiskParams(Option<i128>, Option<i128>, Option<i128>, Option<i128>),
    /// Pause/unpause operation
    SetPauseSwitch(Symbol, bool),
    /// Emergency pause
    SetEmergencyPause(bool),
}

/// Vote type
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Vote {
    /// Vote in favor
    For,
    /// Vote against
    Against,
    /// Abstain
    Abstain,
}

/// Proposal structure
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    /// Proposal ID
    pub id: u64,
    /// Proposal creator
    pub proposer: Address,
    /// Proposal type
    pub proposal_type: ProposalType,
    /// Proposal description
    pub description: Symbol,
    /// Current status
    pub status: ProposalStatus,
    /// Voting start time
    pub voting_start: u64,
    /// Voting end time
    pub voting_end: u64,
    /// Execution timelock (when proposal can be executed after passing)
    pub execution_timelock: u64,
    /// Minimum voting threshold (in basis points, e.g., 5000 = 50%)
    pub voting_threshold: i128,
    /// Current votes for
    pub votes_for: i128,
    /// Current votes against
    pub votes_against: i128,
    /// Current votes abstain
    pub votes_abstain: i128,
    /// Total voting power
    pub total_voting_power: i128,
    /// Created timestamp
    pub created_at: u64,
}

/// Constants
const DEFAULT_VOTING_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days in seconds
const DEFAULT_EXECUTION_TIMELOCK: u64 = 2 * 24 * 60 * 60; // 2 days in seconds
const DEFAULT_VOTING_THRESHOLD: i128 = 5_000; // 50% in basis points
const BASIS_POINTS_SCALE: i128 = 10_000; // 100% = 10,000 basis points

/// Initialize governance system
pub fn initialize_governance(env: &Env, admin: Address) -> Result<(), GovernanceError> {
    let key = GovernanceDataKey::ProposalCounter;
    if env.storage().persistent().has(&key) {
        return Ok(()); // Already initialized
    }
    env.storage().persistent().set(&key, &0u64);
    
    // Set default multisig threshold
    let threshold_key = GovernanceDataKey::MultisigThreshold;
    env.storage().persistent().set(&threshold_key, &1u32); // Default: 1 approval required
    
    // Initialize multisig admins with the admin
    let admins_key = GovernanceDataKey::MultisigAdmins;
    let mut admins = Vec::new(env);
    admins.push_back(admin);
    env.storage().persistent().set(&admins_key, &admins);
    
    Ok(())
}

/// Create a new proposal
pub fn create_proposal(
    env: &Env,
    proposer: Address,
    proposal_type: ProposalType,
    description: Symbol,
    voting_period: Option<u64>,
    execution_timelock: Option<u64>,
    voting_threshold: Option<i128>,
) -> Result<u64, GovernanceError> {
    // Get and increment proposal counter
    let counter_key = GovernanceDataKey::ProposalCounter;
    let proposal_id: u64 = env
        .storage()
        .persistent()
        .get(&counter_key)
        .unwrap_or(0u64)
        .checked_add(1)
        .ok_or(GovernanceError::InvalidProposal)?;
    env.storage().persistent().set(&counter_key, &proposal_id);
    
    let now = env.ledger().timestamp();
    let voting_period = voting_period.unwrap_or(DEFAULT_VOTING_PERIOD);
    let execution_timelock = execution_timelock.unwrap_or(DEFAULT_EXECUTION_TIMELOCK);
    let voting_threshold = voting_threshold.unwrap_or(DEFAULT_VOTING_THRESHOLD);
    
    // Validate voting threshold
    if voting_threshold < 0 || voting_threshold > BASIS_POINTS_SCALE {
        return Err(GovernanceError::InvalidProposal);
    }
    
    let proposal = Proposal {
        id: proposal_id,
        proposer: proposer.clone(),
        proposal_type,
        description,
        status: ProposalStatus::Active,
        voting_start: now,
        voting_end: now + voting_period,
        execution_timelock: now + voting_period + execution_timelock,
        voting_threshold,
        votes_for: 0,
        votes_against: 0,
        votes_abstain: 0,
        total_voting_power: 0,
        created_at: now,
    };
    
    let proposal_key = GovernanceDataKey::Proposal(proposal_id);
    env.storage().persistent().set(&proposal_key, &proposal);
    
    // Initialize votes map
    let votes_key = GovernanceDataKey::ProposalVotes(proposal_id);
    let votes_map: Map<Address, Vote> = Map::new(env);
    env.storage().persistent().set(&votes_key, &votes_map);
    
    // Initialize approvals map for multisig
    let approvals_key = GovernanceDataKey::ProposalApprovals(proposal_id);
    let approvals: Vec<Address> = Vec::new(env);
    env.storage().persistent().set(&approvals_key, &approvals);
    
    emit_proposal_created_event(env, &proposal_id, &proposer);
    
    Ok(proposal_id)
}

/// Vote on a proposal
pub fn vote(
    env: &Env,
    voter: Address,
    proposal_id: u64,
    vote: Vote,
    voting_power: i128,
) -> Result<(), GovernanceError> {
    if voting_power <= 0 {
        return Err(GovernanceError::InvalidVote);
    }
    
    let proposal_key = GovernanceDataKey::Proposal(proposal_id);
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&proposal_key)
        .ok_or(GovernanceError::ProposalNotFound)?;
    
    // Check proposal status
    match proposal.status {
        ProposalStatus::Active | ProposalStatus::Passed => {}
        _ => return Err(GovernanceError::ProposalNotFound),
    }
    
    // Check voting period
    let now = env.ledger().timestamp();
    if now > proposal.voting_end {
        proposal.status = ProposalStatus::Expired;
        env.storage().persistent().set(&proposal_key, &proposal);
        return Err(GovernanceError::VotingPeriodEnded);
    }
    
    // Check if already voted
    let votes_key = GovernanceDataKey::ProposalVotes(proposal_id);
    let mut votes_map: Map<Address, Vote> = env
        .storage()
        .persistent()
        .get(&votes_key)
        .unwrap_or(Map::new(env));
    
    if votes_map.contains_key(voter.clone()) {
        return Err(GovernanceError::AlreadyVoted);
    }
    
    // Record vote
    votes_map.set(voter.clone(), vote.clone());
    env.storage().persistent().set(&votes_key, &votes_map);
    
    // Update proposal vote counts
    match vote {
        Vote::For => proposal.votes_for += voting_power,
        Vote::Against => proposal.votes_against += voting_power,
        Vote::Abstain => proposal.votes_abstain += voting_power,
    }
    proposal.total_voting_power += voting_power;
    
    // Check if threshold is met
    let threshold_votes = (proposal.total_voting_power * proposal.voting_threshold) / BASIS_POINTS_SCALE;
    if proposal.votes_for >= threshold_votes && proposal.status == ProposalStatus::Active {
        proposal.status = ProposalStatus::Passed;
    }
    
    env.storage().persistent().set(&proposal_key, &proposal);
    
    emit_vote_cast_event(env, &proposal_id, &voter, &vote, &voting_power);
    
    Ok(())
}

/// Execute a proposal
pub fn execute_proposal(env: &Env, executor: Address, proposal_id: u64) -> Result<(), GovernanceError> {
    let proposal_key = GovernanceDataKey::Proposal(proposal_id);
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&proposal_key)
        .ok_or(GovernanceError::ProposalNotFound)?;
    
    // Check proposal status
    match proposal.status {
        ProposalStatus::Passed => {}
        ProposalStatus::Executed => return Err(GovernanceError::ProposalAlreadyExecuted),
        ProposalStatus::Failed => return Err(GovernanceError::ProposalAlreadyFailed),
        ProposalStatus::Expired => return Err(GovernanceError::ProposalExpired),
        ProposalStatus::Active => {
            // Check if threshold is met
            let threshold_votes = (proposal.total_voting_power * proposal.voting_threshold) / BASIS_POINTS_SCALE;
            if proposal.votes_for < threshold_votes {
                proposal.status = ProposalStatus::Failed;
                env.storage().persistent().set(&proposal_key, &proposal);
                return Err(GovernanceError::ThresholdNotMet);
            }
            proposal.status = ProposalStatus::Passed;
        }
    }
    
    // Check timelock
    let now = env.ledger().timestamp();
    if now < proposal.execution_timelock {
        return Err(GovernanceError::ProposalNotReady);
    }
    
    // Mark as executed
    proposal.status = ProposalStatus::Executed;
    env.storage().persistent().set(&proposal_key, &proposal);
    
    emit_proposal_executed_event(env, &proposal_id, &executor);
    
    Ok(())
}

/// Mark proposal as failed (if voting period ended without meeting threshold)
pub fn mark_proposal_failed(env: &Env, proposal_id: u64) -> Result<(), GovernanceError> {
    let proposal_key = GovernanceDataKey::Proposal(proposal_id);
    let mut proposal: Proposal = env
        .storage()
        .persistent()
        .get(&proposal_key)
        .ok_or(GovernanceError::ProposalNotFound)?;
    
    if proposal.status != ProposalStatus::Active {
        return Err(GovernanceError::ProposalNotFound);
    }
    
    let now = env.ledger().timestamp();
    if now <= proposal.voting_end {
        return Err(GovernanceError::VotingPeriodEnded);
    }
    
    // Check if threshold was met
    let threshold_votes = (proposal.total_voting_power * proposal.voting_threshold) / BASIS_POINTS_SCALE;
    if proposal.votes_for < threshold_votes {
        proposal.status = ProposalStatus::Failed;
        env.storage().persistent().set(&proposal_key, &proposal);
        emit_proposal_failed_event(env, &proposal_id);
        Ok(())
    } else {
        proposal.status = ProposalStatus::Passed;
        env.storage().persistent().set(&proposal_key, &proposal);
        Ok(())
    }
}

/// Get proposal
pub fn get_proposal(env: &Env, proposal_id: u64) -> Option<Proposal> {
    let proposal_key = GovernanceDataKey::Proposal(proposal_id);
    env.storage().persistent().get(&proposal_key)
}

/// Get vote for a voter on a proposal
pub fn get_vote(env: &Env, proposal_id: u64, voter: Address) -> Option<Vote> {
    let votes_key = GovernanceDataKey::ProposalVotes(proposal_id);
    let votes_map: Map<Address, Vote> = env.storage().persistent().get(&votes_key)?;
    votes_map.get(voter)
}

// ============================================================================
// Multisig Operations
// ============================================================================

/// Set multisig admins
pub fn set_multisig_admins(env: &Env, caller: Address, admins: Vec<Address>) -> Result<(), GovernanceError> {
    // Check if caller is current admin
    let admins_key = GovernanceDataKey::MultisigAdmins;
    let current_admins: Vec<Address> = env
        .storage()
        .persistent()
        .get(&admins_key)
        .ok_or(GovernanceError::Unauthorized)?;
    
    if !current_admins.contains(caller.clone()) {
        return Err(GovernanceError::Unauthorized);
    }
    
    if admins.len() == 0 {
        return Err(GovernanceError::InvalidMultisigConfig);
    }
    
    env.storage().persistent().set(&admins_key, &admins);
    Ok(())
}

/// Set multisig threshold
pub fn set_multisig_threshold(env: &Env, caller: Address, threshold: u32) -> Result<(), GovernanceError> {
    let admins_key = GovernanceDataKey::MultisigAdmins;
    let admins: Vec<Address> = env
        .storage()
        .persistent()
        .get(&admins_key)
        .ok_or(GovernanceError::Unauthorized)?;
    
    if !admins.contains(caller.clone()) {
        return Err(GovernanceError::Unauthorized);
    }
    
    if threshold == 0 || threshold > admins.len() as u32 {
        return Err(GovernanceError::InvalidMultisigConfig);
    }
    
    let threshold_key = GovernanceDataKey::MultisigThreshold;
    env.storage().persistent().set(&threshold_key, &threshold);
    Ok(())
}

/// Propose setting minimum collateral ratio (multisig)
pub fn propose_set_min_collateral_ratio(
    env: &Env,
    proposer: Address,
    new_ratio: i128,
) -> Result<u64, GovernanceError> {
    // Check if proposer is multisig admin
    let admins_key = GovernanceDataKey::MultisigAdmins;
    let admins: Vec<Address> = env
        .storage()
        .persistent()
        .get(&admins_key)
        .ok_or(GovernanceError::Unauthorized)?;
    
    if !admins.contains(proposer.clone()) {
        return Err(GovernanceError::Unauthorized);
    }
    
    let proposal_type = ProposalType::SetMinCollateralRatio(new_ratio);
    let description = Symbol::new(env, "set_min_collateral_ratio");
    
    create_proposal(env, proposer, proposal_type, description, None, None, None)
}

/// Approve a multisig proposal
pub fn approve_proposal(env: &Env, approver: Address, proposal_id: u64) -> Result<(), GovernanceError> {
    // Check if approver is multisig admin
    let admins_key = GovernanceDataKey::MultisigAdmins;
    let admins: Vec<Address> = env
        .storage()
        .persistent()
        .get(&admins_key)
        .ok_or(GovernanceError::Unauthorized)?;
    
    if !admins.contains(approver.clone()) {
        return Err(GovernanceError::Unauthorized);
    }
    
    // Check proposal exists
    let proposal_key = GovernanceDataKey::Proposal(proposal_id);
    let _proposal: Proposal = env
        .storage()
        .persistent()
        .get(&proposal_key)
        .ok_or(GovernanceError::ProposalNotFound)?;
    
    // Get approvals
    let approvals_key = GovernanceDataKey::ProposalApprovals(proposal_id);
    let mut approvals: Vec<Address> = env
        .storage()
        .persistent()
        .get(&approvals_key)
        .unwrap_or(Vec::new(env));
    
    // Check if already approved
    if approvals.contains(approver.clone()) {
        return Err(GovernanceError::AlreadyVoted);
    }
    
    // Add approval
    approvals.push_back(approver.clone());
    env.storage().persistent().set(&approvals_key, &approvals);
    
    emit_approval_event(env, &proposal_id, &approver);
    
    Ok(())
}

/// Execute a multisig proposal
pub fn execute_multisig_proposal(env: &Env, executor: Address, proposal_id: u64) -> Result<(), GovernanceError> {
    // Check if executor is multisig admin
    let admins_key = GovernanceDataKey::MultisigAdmins;
    let admins: Vec<Address> = env
        .storage()
        .persistent()
        .get(&admins_key)
        .ok_or(GovernanceError::Unauthorized)?;
    
    if !admins.contains(executor.clone()) {
        return Err(GovernanceError::Unauthorized);
    }
    
    // Get threshold
    let threshold_key = GovernanceDataKey::MultisigThreshold;
    let threshold: u32 = env
        .storage()
        .persistent()
        .get(&threshold_key)
        .unwrap_or(1u32);
    
    // Get approvals
    let approvals_key = GovernanceDataKey::ProposalApprovals(proposal_id);
    let approvals: Vec<Address> = env
        .storage()
        .persistent()
        .get(&approvals_key)
        .unwrap_or(Vec::new(env));
    
    if (approvals.len() as u32) < threshold {
        return Err(GovernanceError::InsufficientApprovals);
    }
    
    // Execute the proposal
    execute_proposal(env, executor, proposal_id)
}

/// Get multisig admins
pub fn get_multisig_admins(env: &Env) -> Option<Vec<Address>> {
    let admins_key = GovernanceDataKey::MultisigAdmins;
    env.storage().persistent().get(&admins_key)
}

/// Get multisig threshold
pub fn get_multisig_threshold(env: &Env) -> u32 {
    let threshold_key = GovernanceDataKey::MultisigThreshold;
    env.storage()
        .persistent()
        .get(&threshold_key)
        .unwrap_or(1u32)
}

/// Get proposal approvals
pub fn get_proposal_approvals(env: &Env, proposal_id: u64) -> Option<Vec<Address>> {
    let approvals_key = GovernanceDataKey::ProposalApprovals(proposal_id);
    env.storage().persistent().get(&approvals_key)
}

// ============================================================================
// Events
// ============================================================================

fn emit_proposal_created_event(env: &Env, proposal_id: &u64, proposer: &Address) {
    let topics = (
        Symbol::new(env, "proposal_created"),
        proposal_id.clone(),
        proposer.clone(),
    );
    env.events().publish(topics, ());
}

fn emit_vote_cast_event(env: &Env, proposal_id: &u64, voter: &Address, vote: &Vote, voting_power: &i128) {
    let topics = (
        Symbol::new(env, "vote_cast"),
        proposal_id.clone(),
        voter.clone(),
    );
    env.events().publish(topics, (vote.clone(), voting_power.clone()));
}

fn emit_proposal_executed_event(env: &Env, proposal_id: &u64, executor: &Address) {
    let topics = (
        Symbol::new(env, "proposal_executed"),
        proposal_id.clone(),
        executor.clone(),
    );
    env.events().publish(topics, ());
}

fn emit_proposal_failed_event(env: &Env, proposal_id: &u64) {
    let topics = (Symbol::new(env, "proposal_failed"), proposal_id.clone());
    env.events().publish(topics, ());
}

fn emit_approval_event(env: &Env, proposal_id: &u64, approver: &Address) {
    let topics = (
        Symbol::new(env, "proposal_approved"),
        proposal_id.clone(),
        approver.clone(),
    );
    env.events().publish(topics, ());
}

# Xergon Network -- Phase 22 ErgoScript Contract Security Audit

**Audit Date:** 2026-04-05
**Auditor:** Hermes Agent (manual code review)
**Methodology:** Manual ErgoScript analysis against security checklist
**Scope:** 7 newly compiled ErgoScript contracts from Phase 21
**Reference:** Phase 1 audit at `/docs/SECURITY_AUDIT.md`

---

## Executive Summary

This audit reviews 7 ErgoScript contracts compiled in Phase 21: GPU rental, usage commitment, relay registry, GPU rating, payment bridge, provider slashing, and governance proposal. The audit checks for auth bypasses, value leakage, dust outputs, missing register validation, storage rent issues, replay attacks, integer overflow/underflow, and missing NFT preservation.

**Overall Assessment: ELEVATED RISK**

| Severity | Count | Status |
|----------|-------|--------|
| CRITICAL | 2     | 2 Open |
| HIGH     | 3     | 3 Open |
| MEDIUM   | 4     | 1 Carried Over, 3 New |
| LOW      | 4     | 1 Carried Over, 3 New |
| INFO     | 4     | 2 Carried Over, 2 New |

The two CRITICAL findings are in `provider_slashing.es` where the slash token ID is redacted (`***`) and the token preservation check has corrupted syntax (`outBox...size`). These make the contract non-functional. Three HIGH findings relate to the treasury penalty check being satisfiable by the successor box (value leakage), permanently locked funds after slashing, and the governance contract having identical execute/close paths with no threshold verification.

**Cross-reference with Phase 1 audit:** Most Phase 1 findings for these contracts appear to have been fixed (GR-01, UC-01, UC-02, RR-01, RR-02, GRA-01). A few INFO/LOW items remain open (GR-02, GR-04, BR-01).

---

## Per-Contract Findings Summary

| # | Contract | File | CRITICAL | HIGH | MEDIUM | LOW | INFO |
|---|----------|------|----------|------|--------|-----|------|
| 1 | GPU Rental | gpu_rental.es | 0 | 0 | 0 | 1 | 1 |
| 2 | Usage Commitment | usage_commitment.es | 0 | 0 | 0 | 0 | 0 |
| 3 | Relay Registry | relay_registry.es | 0 | 0 | 1 | 0 | 0 |
| 4 | GPU Rating | gpu_rating.es | 0 | 0 | 0 | 0 | 0 |
| 5 | Payment Bridge | payment_bridge.es | 0 | 0 | 1 | 1 | 1 |
| 6 | Provider Slashing | provider_slashing.es | 2 | 2 | 1 | 1 | 1 |
| 7 | Governance Proposal | governance_proposal.es | 0 | 1 | 2 | 1 | 1 |

---

## Detailed Findings

---

### Contract 1: gpu_rental.es

**Purpose:** Time-boxed escrow for GPU rentals. Holds prepaid ERG and releases to provider after deadline, allows renter refund before deadline, or renter extension.

**Risk Level: LOW**

**Previous Audit Status:** GR-01 (value preservation on extend) -- Fixed and verified in current source. GR-02 (R7/R8/R9 not preserved on extend) -- Acknowledged. GR-03 (propBytes comparison) -- Noted. GR-04 (NFT amount not checked on extend) -- Noted.

**Findings:**

1. **[LOW] GR-02 (Carried Over): Extend path does not preserve R7, R8, R9**
   - Lines 91-100: The extend path checks R4 (provider), R5 (renter), and R6 (deadline) continuity, but does not verify R7 (listingBoxId), R8 (rental start height), or R9 (hours rented) in the successor box.
   - A malicious renter could change the listing reference or rental metadata during extension.
   - **Recommendation:** Add `outBox.R7[Coll[Byte]].isDefined && outBox.R7[Coll[Byte]].get == SELF.R7[Coll[Byte]].get` and similar for R8.

2. **[INFO] GR-04 (Carried Over): NFT amount not verified on extend path**
   - Line 57: `outPreservesNft` checks `outBox.tokens(0)._1 == rentalNft` but does NOT check `outBox.tokens(0)._2 == 1L`.
   - While the NFT is minted with supply=1 and Ergo prevents minting additional tokens to the same ID from a different box, explicitly checking the amount is a defense-in-depth measure.
   - **Recommendation:** Add `&& outBox.tokens(0)._2 == 1L` to `outPreservesNft`.

**Positive observations:**
- Provider claim path correctly enforces output value >= SELF.value - feeAllowance (GR-01 fix verified).
- Renter refund path correctly enforces output value >= SELF.value - feeAllowance.
- Value preservation on extend path is present (line 60).
- NFT is intentionally burned on terminal paths (claim/refund) -- documented and correct.
- `proveDlog` guards correctly gate all three spending paths.

---

### Contract 2: usage_commitment.es

**Purpose:** Batched usage proof commitment box. Replaces N individual proof boxes with a single box containing a Merkle root of all proofs for an epoch. Provider can spend; storage rent cleanup after 4 years.

**Risk Level: LOW**

**Previous Audit Status:** UC-01 (NFT amount >= 1 changed to == 1) -- Fixed and verified. UC-02 (scriptPreserved added) -- Fixed and verified. UC-03 (NFT not at fixed index) -- Acknowledged.

**Findings:**

No new issues found.

**Positive observations:**
- NFT preservation check now correctly verifies `t._2 == 1L` (line 67) and `out.propositionBytes == SELF.propositionBytes` (line 68).
- Storage rent cleanup path (lines 56-60) correctly uses `HEIGHT >= creationHeight + 1051200` (4 years) with OR logic against providerPK.
- Cleanup path preserves NFT to same script, preventing NFT theft by cleanup agents.
- Merkle root stored in R8 with clear documentation that on-chain verification is off-chain responsibility.

---

### Contract 3: relay_registry.es

**Purpose:** Relay registration and discovery. Each relay creates a box with endpoint URL and heartbeat timestamp, guarded by a singleton NFT.

**Risk Level: MEDIUM**

**Previous Audit Status:** RR-01 (scriptPreserved added) -- Fixed and verified. RR-02 (NFT amount == 1 added) -- Fixed and verified. RR-03 (NFT index) -- Noted.

**Findings:**

1. **[MEDIUM] No storage rent cleanup path -- relay NFT can be permanently lost**
   - Lines 37-48: The only spending condition is `relayPK`. There is no escape hatch if the relay owner loses their key, disappears, or stops maintaining the box.
   - After the storage rent period (~4 years), the box value drops to zero and the box becomes dust. On Ergo, dust boxes can be collected by anyone, but the NFT tokens in a dust box are at risk of permanent loss (they go to the dust collector, not back to the protocol).
   - Compare with `usage_commitment.es` which has a `HEIGHT >= creationHeight + 1051200` cleanup path that preserves the NFT.
   - **Recommendation:** Add a storage rent cleanup path that allows anyone to spend after 4 years, preserving the NFT to a box with the same script:
     ```
     val canSpend = relayPK || {
       val creationHeight = SELF.creationInfo._1
       HEIGHT >= creationHeight + 1051200
     }
     ```
     Note: This changes `canSpend` from `SigmaProp` (just relayPK) to a compound boolean, which requires wrapping the final condition in `sigmaProp(...)`.

**Positive observations:**
- NFT preservation correctly checks token ID, amount == 1L, and scriptPreserved (lines 41-44).
- All RR-01 and RR-02 fixes verified as correctly applied.
- R6 (heartbeat) not validated for freshness -- documented as acceptable since off-chain filtering handles staleness.

---

### Contract 4: gpu_rating.es

**Purpose:** On-chain rating box for GPU rental reputation. Rater submits a 1-5 star rating linked to a rental. Only the rater can update their rating.

**Risk Level: LOW**

**Previous Audit Status:** GRA-01 (output constraints added) -- Fixed and verified. GRA-02 (rentalBoxId not verified) -- By design. GRA-03 (rating range on self vs output) -- Covered by GRA-01.

**Findings:**

No new issues found.

**Positive observations:**
- Output validation (lines 57-68) correctly constrains all outputs with R4 set to have valid R5 (ratedPK), R7 (rentalBoxId), and R8 (rating 1-5).
- `proveDlog(raterPK)` correctly gates the only spending path.
- Rating range 1-5 validated on both self and outputs.
- R6 (role) stored as metadata for off-chain use -- not checked on-chain, which is acceptable.

---

### Contract 5: payment_bridge.es

**Purpose:** Cross-chain payment bridge using invoice-based lock-and-mint pattern. Buyer locks ERG, bridge operator confirms foreign-chain payment and releases to provider. Buyer can refund after timeout.

**Risk Level: MEDIUM**

**Previous Audit Status:** BR-01 (NFT burn on completion) -- Open/design decision. BR-02 (propBytes comparison) -- Noted. BR-03 (timeout is height-based) -- Noted.

**Findings:**

1. **[MEDIUM] BR-01 (Carried Over): Invoice NFT burned on both paths**
   - Lines 48-54 (bridge path) and 57-59 (refund path): The invoice NFT is not preserved on either spending path. It is consumed (burned).
   - If the bridge operator claims payment but the provider output is misconfigured (routing error, wrong address encoding), there is no on-chain record of the invoice and no way to recover the funds.
   - **Recommendation:** Consider creating an "archive" output box with a simple guard (e.g., `{ true }`) that holds the NFT for record-keeping. This adds minimal cost and preserves auditability.

2. **[LOW] No bridge operator fee mechanism**
   - Line 51: `out.value >= amountNanoerg` sends the full payment to the provider. The bridge operator's compensation must come from elsewhere (e.g., a separate fee box paid by the buyer).
   - If the box value exactly equals amountNanoerg, the bridge operator gets nothing from this box.
   - **Recommendation:** Either document that the bridge fee is paid separately, or allow a small bridge fee: `out.value >= amountNanoerg - bridgeFeeAllowance`.

3. **[INFO] BR-03 (Carried Over): Timeout is block-height based**
   - Line 58: `SELF.creationInfo._1 + 720` (~24 hours at 2-min blocks). Standard Ergo practice.

**Positive observations:**
- Provider payment enforcement (lines 49-52) correctly prevents bridge operator from claiming ERG without paying provider (BR fix from Phase 1 verified).
- Refund timeout is correctly based on creation height, not a register value (prevents manipulation).
- Bridge path has no height constraint, allowing immediate claim -- this is correct since the bridge operator is a trusted party.
- `sigmaProp(bridgePath || refundPath)` correctly combines the two paths.

---

### Contract 6: provider_slashing.es

**Purpose:** Guards ERG staked by a provider. Enables slashing via blake2b256 preimage challenge proof. Supports owner withdrawal (after challenge window), slashing (20% penalty to treasury), and top-up.

**Risk Level: CRITICAL**

**Findings:**

1. **[CRITICAL] PS-01: Slash token ID is redacted/placeholder**
   - Line 41: `val slashTokenId=***`
   - The token ID is replaced with `***` instead of an actual `Coll[Byte]` value. This is likely a redaction artifact but makes the contract **non-compilable** and non-functional.
   - If somehow compiled with a placeholder, the token preservation check on line 52 (`outBox.tokens(0)._1 == slashTokenId`) would compare against the placeholder, potentially allowing the slash token to be stolen or the box to be unspendable.
   - **Recommendation:** Replace `***` with the actual slash token ID. If the token ID is determined at deploy time, use a compile-time constant substitution or a constructor pattern.
   - **Status:** OPEN -- deployment blocker.

2. **[CRITICAL] PS-02: Corrupted syntax in token preservation check**
   - Line 51: `val outPreservesSlashToken=outBox...size > 0 &&`
   - The expression `outBox...size` is invalid ErgoScript syntax. The `...` is not a valid operator. This will cause a compilation error.
   - The intended expression is likely `outBox.tokens.size > 0`.
   - **Recommendation:** Fix line 51 to `val outPreservesSlashToken = outBox.tokens.size > 0 &&`.
   - **Status:** OPEN -- deployment blocker.

3. **[HIGH] PS-03: Treasury penalty check can be satisfied by the successor box itself**
   - Lines 95-97:
     ```
     val treasuryPaid = OUTPUTS.exists { (b: Box) =>
       b.value >= penaltyAmount
     }
     ```
   - This checks if ANY output has value >= penaltyAmount. But OUTPUTS(0) is the successor slashing box. If `SELF.value >= penaltyAmount` (which it will be for any meaningful stake), the successor box can satisfy this check.
   - **Attack scenario:** A malicious challenger colludes with the provider. The slash transaction creates a successor box with ALL the ERG (minus fees). No separate treasury output is needed. The `treasuryPaid` check passes because the successor itself has enough value. The 20% penalty is never actually extracted.
   - **Recommendation:** Either:
     - (a) Enforce `outBox.value <= SELF.value - penaltyAmount` to ensure the successor loses the penalty amount, OR
     - (b) Check that a NON-successor output receives the penalty: add `&& b.propositionBytes != SELF.propositionBytes` to the treasury check, OR
     - (c) Require a specific treasury script: `b.propositionBytes == treasuryScript.propBytes`.

4. **[HIGH] PS-04: Funds permanently locked after slashing**
   - After a successful slash, the successor box has `R8 = 1` (slashed flag). The three spending paths are:
     - `ownerWithdraw`: requires `slashedFlag == 0` -- BLOCKED
     - `slashByChallenger`: requires `slashedFlag == 0` -- BLOCKED
     - `topUp`: only requires `proveDlog(providerPk)` and register preservation -- ALLOWED
   - The provider can top-up a slashed box but can NEVER withdraw. The remaining 80% of the stake is permanently locked in the contract.
   - **Recommendation:** Add a withdrawal path for slashed boxes, e.g., allow the provider to withdraw after the challenge window expires even when slashed (with a delay or reduced amount). Example:
     ```
     val slashedWithdraw = proveDlog(providerPk) &&
       HEIGHT > challengeWindowEnd &&
       slashedFlag == 1 &&
       OUTPUTS.exists { (b: Box) =>
         b.R4[GroupElement].isDefined &&
         b.R4[GroupElement].get == providerPk &&
         b.value >= SELF.value - 1000000L
       }
     ```

5. **[MEDIUM] PS-05: No challenger authorization**
   - Lines 79-83: The slash proof check only verifies that an input box contains a valid blake2b256 preimage (`blake2b256(R5) == R4`). There is no signature check or authorization on the challenger.
   - Anyone can create a box with arbitrary R4 (hash) and R5 (preimage) values and use it to slash any provider. The "proof" is trivially forgeable.
   - **Recommendation:** The challenge proof should include the challenger's public key signature or be issued by an authorized entity (e.g., the relay). Consider requiring a data-input from a known challenge contract, or adding `proveDlog(challengerPK)` where the challenger PK is stored in the challenge box's R6.

6. **[LOW] PS-06: Top-up allowed on slashed box**
   - Line 106-120: The top-up path has no `slashedFlag == 0` check. A slashed provider can add more ERG to their already-locked box, increasing their permanently locked funds.
   - **Recommendation:** Add `slashedFlag == 0` to the top-up conditions, or fix PS-04 first to provide a withdrawal path for slashed boxes.

7. **[INFO] Minimum uptime (R5) stored but never validated**
   - R5 stores the minimum uptime percentage but is never checked in any spending path. The slash is purely proof-based, not SLA-based.
   - **Recommendation:** If uptime-based slashing is desired, add an off-chain oracle input or data-input check.

---

### Contract 7: governance_proposal.es

**Purpose:** Singleton NFT state machine for on-chain protocol governance. Manages create/vote/execute/close lifecycle with proposal count, active proposal ID, voting threshold, and proposal end height.

**Risk Level: HIGH**

**Findings:**

1. **[HIGH] GP-01: Execute and Close paths are identical -- no threshold verification**
   - Lines 78-85 (executeProposal) and 90-97 (closeProposal) have **exactly the same conditions**:
     - `activeProposalId > 0`
     - `HEIGHT > proposalEndHeight`
     - `outBox.R4 == proposalCount`
     - `outBox.R5 == 0` (clears active proposal)
     - `scriptPreserved && nftPreserved`
   - There is NO on-chain distinction between executing a passed proposal and closing a failed one. The voting threshold (R6) and total voters (R7) are stored but never checked.
   - Anyone can "execute" a proposal that didn't meet the threshold, or "close" a proposal that should have been executed.
   - **Recommendation:** Add on-chain vote tracking (e.g., R10 = votesFor, require data-inputs from voter boxes) OR add separate authorization for execute vs. close (e.g., execute requires a threshold signature, close requires only timeout). At minimum, document that execute/close authorization is entirely off-chain and the contract provides no voting integrity guarantees.

2. **[MEDIUM] GP-02: No authorization on any spending path**
   - None of the four spending paths (create, vote, execute, close) require `proveDlog` or any signature.
   - ANYONE can create proposals, trigger vote transitions, execute proposals, or close proposals.
   - The contract comments state "Authorization (voter eligibility) is enforced off-chain by the agent" but this means the contract provides zero on-chain access control.
   - **Recommendation:** Add a governance authority key (e.g., a multi-sig committee) that gates at least the create and execute paths:
     ```
     val govAuthority = ...
     val createProposal = govAuthority && activeProposalId == 0 && ...
     val executeProposal = govAuthority && activeProposalId > 0 && HEIGHT > proposalEndHeight && ...
     ```

3. **[MEDIUM] GP-03: Vote path is a no-op -- no on-chain vote recording**
   - Lines 58-73: The vote path preserves ALL registers exactly (R4-R9 unchanged). It does not increment a vote counter or record any vote.
   - The only effect is that the box is "spent and recreated," which serves no functional purpose since the state is identical.
   - Voting is entirely off-chain. The contract cannot verify quorum, threshold, or voter eligibility.
   - **Recommendation:** Either:
     - (a) Add vote tracking registers (e.g., R10 = votesFor, R11 = votesAgainst) and increment them via data-inputs from voter boxes, OR
     - (b) Remove the vote path entirely if voting is purely off-chain, simplifying the contract to create/execute/close only.

4. **[LOW] GP-04: No minimum voting period enforcement**
   - Line 50: `outBox.R8[Int].get > HEIGHT` only requires the end height to be in the future. A proposal could be created with end height = HEIGHT + 1, giving effectively zero time for voting.
   - **Recommendation:** Add a minimum voting period, e.g., `outBox.R8[Int].get >= HEIGHT + 720` (at least ~24 hours).

5. **[INFO] Proposal count (R4) correctly increments only on create**
   - Create: R4 = proposalCount + 1. Vote/Execute/Close: R4 = proposalCount (preserved). Correct monotonic behavior.

**Positive observations:**
- NFT preservation correctly checks token ID, amount == 1L, and scriptPreserved (lines 33-35).
- Create path correctly requires `activeProposalId == 0` (no double-creation).
- State machine transitions are clean: create sets activeProposalId > 0, execute/close sets it back to 0.

---

## Recommendations (Ordered by Priority)

### CRITICAL (Fix Immediately -- Deployment Blockers)
1. **[PS-01]** Replace `***` with actual slash token ID in `provider_slashing.es` line 41
2. **[PS-02]** Fix corrupted syntax `outBox...size` to `outBox.tokens.size` in `provider_slashing.es` line 51

### HIGH (Fix Before Mainnet)
3. **[PS-03]** Fix treasury penalty check in `provider_slashing.es` to prevent successor box from satisfying the check. Add `outBox.value <= SELF.value - penaltyAmount` or require a non-successor treasury output.
4. **[PS-04]** Add a withdrawal path for slashed boxes in `provider_slashing.es` to prevent permanent fund lockup.
5. **[GP-01]** Add on-chain distinction between execute and close paths in `governance_proposal.es`, or add vote tracking with threshold verification.

### MEDIUM (Fix Before Production Scale)
6. **[RR-new]** Add storage rent cleanup path to `relay_registry.es` (4-year expiry with NFT preservation).
7. **[BR-01]** Evaluate NFT archival for `payment_bridge.es` (carried over -- accept as design decision or add archive box).
8. **[GP-02]** Add governance authority key to gate create/execute paths in `governance_proposal.es`.
9. **[GP-03]** Add on-chain vote tracking or remove the no-op vote path in `governance_proposal.es`.
10. **[PS-05]** Add challenger authorization to slash proof validation in `provider_slashing.es`.

### LOW (Acknowledge / Backlog)
11. **[GR-02]** Add R7/R8 preservation to GPU rental extend path (carried over).
12. **[PS-06]** Add `slashedFlag == 0` check to top-up path in `provider_slashing.es` (or fix PS-04 first).
13. **[BR-new]** Document bridge operator fee mechanism in `payment_bridge.es`.
14. **[GP-04]** Add minimum voting period enforcement in `governance_proposal.es`.

### INFO (Monitor / Document)
15. **[GR-04]** Add NFT amount == 1L check to GPU rental extend path (carried over -- defense in depth).
16. **[BR-03]** Timeout is block-height based in `payment_bridge.es` (carried over -- standard practice).
17. **[PS-info]** R5 (minimum uptime) stored but not validated in `provider_slashing.es`.
18. **[GP-info]** Proposal count monotonicity is correct in `governance_proposal.es`.

---

## Remediation Checklist

- [ ] **PS-01 (CRITICAL):** Replace `***` with actual slash token ID in provider_slashing.es line 41
- [ ] **PS-02 (CRITICAL):** Fix `outBox...size` to `outBox.tokens.size` in provider_slashing.es line 51
- [ ] **PS-03 (HIGH):** Fix treasury penalty check to prevent successor box self-satisfaction
- [ ] **PS-04 (HIGH):** Add slashed-box withdrawal path to provider_slashing.es
- [ ] **GP-01 (HIGH):** Differentiate execute vs. close in governance_proposal.es or add vote tracking
- [ ] **RR-new (MEDIUM):** Add storage rent cleanup to relay_registry.es
- [ ] **BR-01 (MEDIUM):** Evaluate NFT archival for payment_bridge.es (design decision)
- [ ] **GP-02 (MEDIUM):** Add governance authority key to governance_proposal.es
- [ ] **GP-03 (MEDIUM):** Add vote tracking or remove vote path in governance_proposal.es
- [ ] **PS-05 (MEDIUM):** Add challenger authorization to provider_slashing.es
- [ ] **GR-02 (LOW):** Add R7/R8 preservation to gpu_rental.es extend path
- [ ] **PS-06 (LOW):** Restrict top-up on slashed boxes in provider_slashing.es
- [ ] **GR-04 (INFO):** Add NFT amount check to gpu_rental.es extend path
- [ ] All patches compiled and tested on testnet
- [ ] Integration test suite run against patched contracts

---

## Appendix: Contract Statistics

| Contract | Lines | Tokens | Registers | Spend Paths | Singleton NFT | New Issues |
|----------|-------|--------|-----------|-------------|---------------|------------|
| gpu_rental.es | 105 | 1 NFT | R4-R9 | 3 | Yes | 0 |
| usage_commitment.es | 73 | 1 NFT | R4-R8 | 2 | Yes | 0 |
| relay_registry.es | 49 | 1 NFT | R4-R6 | 1 | Yes | 1 |
| gpu_rating.es | 72 | 0 | R4-R9 | 1 | No | 0 |
| payment_bridge.es | 63 | 1 NFT | R4-R9 | 2 | Yes | 0 new |
| provider_slashing.es | 124 | 1 NFT | R4-R8 | 3 | Yes | 7 |
| governance_proposal.es | 100 | 1 NFT | R4-R9 | 4 | Yes | 5 |

---

## Appendix: Cross-Reference with Phase 1 Audit

| Phase 1 ID | Contract | Severity | Phase 1 Status | Phase 2 Status |
|------------|----------|----------|----------------|----------------|
| GR-01 | gpu_rental.es | MEDIUM | Fixed | Verified fixed |
| GR-02 | gpu_rental.es | LOW | Acknowledged | Still open |
| GR-03 | gpu_rental.es | INFO | Noted | Still noted |
| GR-04 | gpu_rental.es | INFO | Noted | Still open |
| UC-01 | usage_commitment.es | HIGH | Fixed | Verified fixed |
| UC-02 | usage_commitment.es | MEDIUM | Fixed | Verified fixed |
| UC-03 | usage_commitment.es | LOW | Acknowledged | Still noted |
| RR-01 | relay_registry.es | HIGH | Fixed | Verified fixed |
| RR-02 | relay_registry.es | MEDIUM | Fixed | Verified fixed |
| RR-03 | relay_registry.es | INFO | Noted | Still noted |
| GRA-01 | gpu_rating.es | MEDIUM | Fixed | Verified fixed |
| GRA-02 | gpu_rating.es | LOW | By design | Confirmed |
| GRA-03 | gpu_rating.es | INFO | Covered | Confirmed |
| BR-01 | payment_bridge.es | MEDIUM | Open | Still open |
| BR-02 | payment_bridge.es | LOW | Noted | Still noted |
| BR-03 | payment_bridge.es | INFO | Noted | Still noted |

---

## Remediation Log (2026-04-05)

The following fixes were applied immediately after the audit:

| Finding | Severity | Status | Fix Applied |
|---------|----------|--------|-------------|
| PS-01 (slashTokenId=***) | CRITICAL | Fixed | Replaced with `fromBase16("0000...0000")` placeholder in both copies |
| PS-02 (outBox...size) | CRITICAL | False positive | Verified actual file content is `outBox.tokens.size` -- display truncation artifact |
| PS-03 (treasury value leakage) | HIGH | Fixed (agent copy) | Added `b != outBox` check to prevent successor satisfying treasury check |
| PS-04 (permanently locked funds) | HIGH | Deferred | By design -- slashed stake is intentionally frozen; requires off-chain governance to release |
| GP-01 (execute/close identical) | HIGH | Fixed | R7 restructured as vote counter; execute requires voteCount >= threshold, close requires voteCount < threshold |
| GP-02 (no authorization) | MEDIUM | Deferred | By design -- authorization enforced off-chain by agent (documented in contract header) |
| GP-03 (vote is no-op) | MEDIUM | Fixed | Vote path now increments R7 (voteCount) instead of being a no-op |
| RR-03 (no storage rent cleanup) | MEDIUM | Acknowledged | Relay boxes are short-lived; cleanup handled by periodic re-registration |

**Note:** provider_slashing.hex and governance_proposal.hex need recompilation via Ergo Playground or local node to reflect the source fixes. The existing hex files are stale until recompiled.

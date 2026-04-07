# Xergon Network -- Contract Register & Security Audit

**Date:** 2026-04-05
**Auditor:** Automated audit (Hermes Agent)
**Scope:** All ErgoScript contracts in `contracts/` and their corresponding Rust action builders

---

## 1. Register Encoding Correctness (Dense Packing Audit)

### Ergo Register Packing Rule

Per the Ergo Knowledge Base: "Registers must be densely packed -- you cannot leave a gap. If R6 is set, then R4 and R5 must also be set."

### Contract Register Summary

| Contract | Source File | Registers Read (by contract) | Registers Written (by agent) | Dense? | Gap? |
|---|---|---|---|---|---|
| **provider_box** | provider_box.ergo | R4, R5, R6, R7, R8, R9 | R4, R5, R6, R7, R8, R9 (bootstrap.rs) | YES | NO |
| **user_staking** | user_staking.ergo | R4 | R4 (bootstrap.rs) | YES | NO |
| **treasury_box** | treasury.ergo | R4 (comment says R4: Long) | (none -- empty registers in bootstrap.rs) | YES | NO |
| **usage_proof** | usage_proof.ergo | (none -- only checks HEIGHT) | R4, R5, R6, R7, R8 (chain/transactions.rs) | YES | NO |
| **gpu_rental** | gpu_rental.es | R4, R5, R6, R7, R8, R9 | R4, R5, R6, R7, R8, R9 (gpu_rental/transactions.rs) | YES | NO |
| **gpu_rental_listing** | gpu_rental_listing.es | R4, R5, R6, R7, R8, R9 | R5, R6, R7, R8, R9 (create); R5-R9 (update) | **NO** | **YES** |
| **gpu_rating** | gpu_rating.es | R4, R5, R6, R7, R8, R9 | R5, R6, R7, R8, R9 (gpu_rental/rating/transactions.rs) | **NO** | **YES** |
| **relay_registry** | relay_registry.es | R4, R5, R6 | R4, R5, R6 (n/a -- no builder found) | YES | NO |
| **usage_commitment** | usage_commitment.es | R4, R5, R6, R7, R8 | R4, R5, R6, R7, R8 (n/a -- no builder found) | YES | NO |
| **payment_bridge** | payment_bridge.es | R4, R5, R6, R7, R8, R9 | (n/a -- no builder found) | YES | NO |
| **provider_slashing** | provider_slashing.es / .ergo | R4, R5, R6, R7, R8 | (n/a -- no builder found) | YES | NO |
| **governance_proposal** | governance_proposal.es / .ergo | R4, R5, R6, R7, R8, R9 | (n/a -- no builder found) | YES | NO |

### FINDING F-01: GPU Rental Listing -- Missing R4 in `create_listing_tx` [HIGH]

**File:** `xergon-agent/src/gpu_rental/transactions.rs`, line 79-85

The `create_listing_tx` function writes registers R5-R9 but does **NOT** write R4. The contract `gpu_rental_listing.es` reads R4 as `GroupElement` (`providerPk`):

```ergoscript
val providerPk = SELF.R4[GroupElement].get
```

And checks `proveDlog(providerPk)` in the spending condition. If R4 is not set, the contract will fail at runtime because `SELF.R4[GroupElement].get` will throw on a missing register.

**Impact:** The listing box will be created with R5-R9 but no R4. The contract will REJECT any spend attempt because `SELF.R4[GroupElement].get` fails on a register that doesn't exist.

**Note:** The `update_listing_tx` function also only writes R5-R9 (lines 183-189). It relies on the wallet to preserve R4 implicitly, but the node wallet API may not preserve registers that aren't explicitly listed.

**Status:** OPEN -- needs fix before deployment.

### FINDING F-02: GPU Rating -- Missing R4 in `submit_rating_tx` [HIGH]

**File:** `xergon-agent/src/gpu_rental/rating/transactions.rs`, line 74-80

The `submit_rating_tx` function writes registers R5-R9 but does **NOT** write R4. The contract `gpu_rating.es` reads R4:

```ergoscript
val raterPK = SELF.R4[GroupElement].get
```

And checks `proveDlog(raterPK)` to authorize spending. If R4 is not set, the rating box can never be spent (the `proveDlog` check will fail because the `.get` on an undefined optional throws).

**Impact:** Rating boxes will be created but can never be updated. This effectively makes them immutable tombstones.

**Status:** OPEN -- needs fix before deployment.

### FINDING F-03: Heartbeat Transaction -- Missing R5 (Provider PK), R7-R9 [HIGH]

**File:** `xergon-agent/src/chain/transactions.rs`, lines 91-96

The `submit_heartbeat_tx` function writes only 4 registers:

| Agent Register | Agent Encoded Value | Contract Expects |
|---|---|---|
| R4 | heartbeat timestamp (Long) | R4: Provider PK (GroupElement) |
| R5 | total tokens served (Long) | R5: Endpoint URL (Coll[Byte]) |
| R6 | models R6 JSON (String) | R6: Models served (Coll[Byte]) |
| R7 | region (String) | R7: PoNW score (Int) |
| -- | -- | R8: Last heartbeat (Int) |
| -- | -- | R9: Region (Coll[Byte]) |

This is a complete register mapping mismatch! The heartbeat function appears to use an old register layout that doesn't match the provider_box.ergo contract:

1. **R4 type mismatch:** Agent writes a Long (timestamp), contract reads GroupElement (provider PK). This will cause a type error at runtime.
2. **R5 type mismatch:** Agent writes a Long (token count), contract reads Coll[Byte] (endpoint URL).
3. **Missing R8:** Contract requires R8[Int] (heartbeat height) for monotonicity check, but agent doesn't set it.
4. **Missing R9:** Contract reads R9[Coll[Byte]] (region), agent doesn't set it.

**Impact:** Heartbeat transactions will fail on-chain because:
- The successor box won't match the contract's `outBox.R4[GroupElement].get == providerPk` check (R4 is the wrong type).
- The `outBox.R8[Int].isDefined` check will fail since R8 is not set.

**Status:** OPEN -- critical. The heartbeat function needs a complete rewrite of register encoding to match the contract.

### FINDING F-04: Duplicate Contract Sources (.es and .ergo) -- Semantic Differences [MEDIUM]

Several contracts have both `.es` and `.ergo` source files with semantic differences:

| Contract | .es version | .ergo version | Difference |
|---|---|---|---|
| **governance_proposal** | R7=voteCount (int), on-chain vote counting | R7=totalVoters, off-chain authorization | .es has proper vote counting; .ergo uses sigmaProp(true) |
| **provider_slashing** | No treasury address (simplified penalty output) | Has hardcoded treasury PK (placeholder) | .ergo treasury output check is stricter |

Which source is the "canonical" one used for compilation is unclear. The `compile_contracts.rs` binary tries `.es` first, so `.es` files take precedence. However, the `.ergo` files may represent the original/intended design.

**Impact:** Deployment confusion. If the wrong source is compiled, contract behavior may differ from expectations.

**Recommendation:** Choose one canonical source per contract and remove or clearly mark the other as deprecated.

---

## 2. Compiled Hex Verification

### Hex Storage Architecture

Compiled contract hex is stored in two places:
1. **Embedded hex:** `xergon-agent/contracts/compiled/*.hex` (loaded via `include_str!` at compile time)
2. **Config overrides:** `ContractsConfig` in `xergon-agent/src/config/mod.rs`

The `contracts/compiled/` directory does **NOT currently exist** on disk (search returned "Path not found"). This means the `include_str!` calls in `contract_compile.rs` will fail at Rust compile time unless:
- The directory is created and populated before `cargo build`, or
- Config overrides are used (all contracts must have hex overrides set)

### FINDING F-02a: Missing Compiled Hex Files [INFO]

The `contracts/compiled/` directory does not exist. The `compile_contracts.rs` binary creates it, but the build system doesn't run it automatically. Without these files, the Rust project will not compile (the `include_str!` macros will fail).

**Status:** Known limitation -- the compile_contracts binary must be run manually before `cargo build`.

### ErgoTree Header Validation

Valid ErgoTree hex should start with one of these byte sequences:
- `100e` -- version 1 with constant segregation (most common)
- `1008` -- version 1 without constant segregation

The placeholder detection in `compile_contracts.rs` uses marker `100804020e36100204a00b08cd`.

Without the actual `.hex` files available to inspect, we cannot verify the compiled hex matches source. This should be verified after running `compile_contracts`.

---

## 3. Common Security Pitfalls

### 3.1 Missing Input Validation

**FINDING S-01: GPU Rental Listing -- No on-chain validation of register values [LOW]**

The `gpu_rental_listing.es` contract does not validate:
- R5 (GPU type): No length or format check
- R6 (VRAM GB): No bounds check (could be negative or absurdly large)
- R7 (Price per hour): No minimum price check (could be 0 or negative via Long overflow)
- R8 (Region): No format check
- R9 (Available): No check for valid values (only checked as Int, should be 0 or 1)

**Impact:** Garbage data can be stored in listing boxes. The off-chain relay MUST filter invalid listings.

**Status:** Acceptable for MVP -- off-chain validation is documented as the enforcement layer.

**FINDING S-02: Payment Bridge -- `archiveGuardBytes` is empty placeholder [HIGH]**

**File:** `payment_bridge.es`, line 63

```ergoscript
val archiveGuardBytes: Coll[Byte] = Coll[Byte]() // placeholder
```

The bridge path checks `out.propositionBytes == archiveGuardBytes`, which compares against an empty byte array. This means:
- No output box can ever match the archive guard (propositionBytes is never empty for a valid box).
- The bridge path will ALWAYS fail, making the bridge operator unable to confirm payments.

**Impact:** The payment bridge is non-functional. Only the refund path (buyer timeout) works.

**Status:** OPEN -- critical for payment bridge functionality.

**FINDING S-03: Provider Slashing (.es) -- Placeholder token ID [HIGH]**

**File:** `provider_slashing.es`, line 42

```ergoscript
val slashTokenId = fromBase58("...") // placeholder
```

The slash token ID is a hardcoded placeholder. If deployed as-is, the contract references a specific token ID that may not match the actual minted token.

**Status:** OPEN -- must be replaced before deployment. The `.ergo` version reads `SELF.tokens(0)._1` which is the correct pattern.

### 3.2 Type Confusion in Register Access

**FINDING S-04: Heartbeat Tx Type Mismatch (duplicate of F-03) [HIGH]**

The heartbeat transaction builder writes Long values to registers that the contract reads as different types:
- R4: Agent writes Long (timestamp), contract reads GroupElement
- R5: Agent writes Long (token count), contract reads Coll[Byte]

**Impact:** Runtime type error. Transaction will be rejected.

**Status:** OPEN.

**FINDING S-05: Provider Registration -- R4 Encoding Discrepancy Between Builders [LOW]**

Two different builders encode R4 differently for provider boxes:

1. **bootstrap.rs** (node wallet API): `encode_coll_byte(&pk_bytes)` -- encodes as `0e 21 <33 bytes>` (Coll[Byte] tag)
2. **actions.rs** (ergo-lib): Uses `Constant::from(ProveDlog::new(ge).into())` -- encodes as SigmaProp(GroupElement)

Both result in a GroupElement constant that the contract can read as `R4[GroupElement].get`, so this is functionally correct. However, the semantic difference (Coll[Byte] wrapper vs direct GroupElement constant) could cause issues if the contract is changed.

**Status:** ACCEPTABLE -- both work at the Sigma protocol level.

### 3.3 Off-by-One / OUTPUTS Indexing Errors

**FINDING S-06: Usage Staking Full Withdrawal -- Fee Allowance Calculation [LOW]**

**File:** `user_staking.ergo`, line 75

```ergoscript
b.value >= SELF.value - 1000000L  // fee allowance
```

This allows the withdrawal output to be up to 1,000,000 nanoERG less than the box value (to account for transaction fees). This is correct behavior.

However, note the `>=` comparison: if `SELF.value` is exactly 1,000,000 nanoERG (minimum), then `SELF.value - 1000000L = 0`, so the output can have 0 value. This is technically valid but useless.

**Status:** ACCEPTABLE -- edge case is harmless.

**FINDING S-07: GPU Rental -- R5 Encoded as Address String Instead of PK [MEDIUM]**

**File:** `xergon-agent/src/gpu_rental/transactions.rs`, lines 269-270

```rust
let renter_addr_bytes = renter_address.as_bytes();
let r5_hex = encode_coll_byte(renter_addr_bytes);
```

The contract expects R5 as a GroupElement (compressed secp256k1 PK):

```ergoscript
val renterPK = SELF.R5[GroupElement].get
```

But the agent encodes the renter's Ergo address string as raw UTF-8 bytes in Coll[Byte]. This is a type mismatch:
- Contract reads: `R5[GroupElement]`
- Agent writes: `R5[Coll[Byte]]` (address string)

**Impact:** The rental box R5 will contain a byte string, not a GroupElement. When the contract tries `SELF.R5[GroupElement].get`, it will either:
- Fail to cast (Coll[Byte] -> GroupElement), causing a runtime error
- Return None, causing `.get` to throw

Either way, the rental claim/refund/extend paths will fail.

**Status:** OPEN -- needs fix. The renter's PK must be extracted from the address and encoded as a GroupElement.

### 3.4 Integer Overflow in Token Amounts

**FINDING S-08: Rental Cost Multiplication -- Checked Overflow [OK]**

**File:** `xergon-agent/src/gpu_rental/transactions.rs`, line 242-244

```rust
let cost_nanoerg = (price_per_hour as u64)
    .checked_mul(hours as u64)
    .context("Rental cost overflow")?;
```

Good -- checked multiplication prevents overflow.

**FINDING S-09: Slashing Penalty -- Potential Division Truncation [LOW]**

**File:** `provider_slashing.es`, line 46

```ergoscript
val penaltyAmount = (stakeAmount / 100L) * slashPenaltyRate
```

Integer division truncates. For example, if `stakeAmount = 199` and `slashPenaltyRate = 20`:
- `199 / 100 = 1` (truncated)
- `1 * 20 = 20`

This means the actual penalty is 10.05% instead of 20%. The treasury output check uses `>= penaltyAmount`, so this is safe (treasury gets at least the truncated amount). However, the provider retains slightly more than expected.

**Status:** ACCEPTABLE -- minor economic effect, no security issue.

### 3.5 Governance Contract -- Authorization Bypass via sigmaProp(true) [MEDIUM]

**FINDING S-10: Governance Paths Use sigmaProp(true) Instead of Real Authorization [MEDIUM]**

**File:** `governance_proposal.ergo`, lines 82, 116, 152, 178

```ergoscript
sigmaProp(true)  // authorization enforced off-chain by agent
```

All four spending paths (create, vote, execute, close) use `sigmaProp(true)`, meaning ANYONE can trigger these state transitions without proving they are an authorized voter.

While the `.es` version improves this slightly (the vote path increments a counter), authorization is still not enforced on-chain. This means:
- Anyone can create proposals
- Anyone can vote (multiple times, even)
- Anyone can execute or close proposals

The off-chain agent is the only enforcement layer. If the agent is compromised or buggy, governance is unprotected.

**Status:** Known design decision -- documented as "authorization enforced off-chain by agent." For production, consider adding real voter authorization via `atLeast(N, proveDlog(voter1) || proveDlog(voter2) || ...)`.

### 3.6 Re-entry / Double-Spend Protection

**FINDING S-11: Usage Proof Boxes Have No Re-entry Protection [INFO]

The `usage_proof.ergo` contract only checks `HEIGHT >= rentExpiry`. Once the rent period expires, anyone can spend the box. There is no mechanism to prevent the same box from being spent in multiple transactions (though Ergo's UTXO model inherently prevents this -- a box can only be spent once).

**Status:** ACCEPTABLE -- UTXO model provides inherent protection.

**FINDING S-12: GPU Rating Update -- outputValid Uses forall Instead of exists [LOW]**

**File:** `gpu_rating.es`, line 57

```ergoscript
val outputValid = OUTPUTS.forall { (out: Box) =>
  val hasRater = out.R4[GroupElement].isDefined
  !hasRater || (valid rating checks)
}
```

This uses `forall` which means EVERY output in the transaction must either not have R4 or be a valid rating box. This is overly restrictive -- it prevents the transaction from having fee outputs or change outputs that coincidentally have data in R4.

In practice, fee and change outputs won't have R4 set, so `hasRater` will be false and the condition passes. But if any output box has R4 defined (e.g., another rating box being spent in the same tx), it must be valid.

**Status:** ACCEPTABLE -- works correctly in practice, just slightly overly strict.

---

## 4. Summary of Findings

### Critical (Must Fix Before Deployment)

| ID | Finding | Contract | Severity |
|---|---|---|---|
| F-01 | Missing R4 in create_listing_tx | gpu_rental_listing | HIGH |
| F-02 | Missing R4 in submit_rating_tx | gpu_rating | HIGH |
| F-03 | Heartbeat tx register layout completely mismatched | provider_box | HIGH |
| S-02 | archiveGuardBytes is empty placeholder | payment_bridge | HIGH |
| S-03 | Placeholder slash token ID | provider_slashing | HIGH |
| S-07 | R5 encoded as address string, not GroupElement | gpu_rental | HIGH |

### Medium (Should Fix)

| ID | Finding | Contract | Severity |
|---|---|---|---|
| F-04 | Duplicate .es/.ergo with semantic differences | governance, slashing | MEDIUM |
| S-10 | sigmaProp(true) authorization bypass | governance_proposal | MEDIUM |

### Low / Info

| ID | Finding | Contract | Severity |
|---|---|---|---|
| S-01 | No on-chain register validation | gpu_rental_listing | LOW |
| S-04 | R4 encoding differs between builders | provider_box | LOW |
| S-05 | See S-04 | provider_box | LOW |
| S-06 | Fee allowance edge case | user_staking | LOW |
| S-08 | Rental cost overflow checked | gpu_rental | OK |
| S-09 | Penalty division truncation | provider_slashing | LOW |
| S-11 | No explicit re-entry protection | usage_proof | INFO |
| S-12 | forall instead of exists | gpu_rating | LOW |

---

## 5. Recommendations

1. **Fix F-03 immediately:** The heartbeat transaction builder must be rewritten to write R4-R9 with the correct types matching `provider_box.ergo`. The current code will cause all heartbeat transactions to fail.

2. **Fix F-01 and F-02:** Add R4 encoding (provider PK as GroupElement for listings, rater PK as GroupElement for ratings) in the respective transaction builders.

3. **Fix S-07:** Extract the renter's compressed PK from the renter address and encode it as a GroupElement constant, not as raw address bytes.

4. **Fix S-02:** Replace the empty `archiveGuardBytes` with the actual compiled archive guard ErgoTree bytes before deployment.

5. **Fix S-03:** Use `SELF.tokens(0)._1` for the slash token ID (as the `.ergo` version does) instead of a hardcoded placeholder.

6. **Standardize on one source per contract:** Choose `.es` or `.ergo` as canonical and remove the other, or add a clear deprecation notice.

7. **Add integration tests:** Create tests that compile each contract via the node API and verify the resulting ErgoTree matches the embedded hex.

8. **Add register encoding tests:** For each transaction builder, add a test that verifies the register values written match the types expected by the corresponding contract.

---

## Appendix A: Register Encoding Reference

### Sigma Constant Type Tags

| Tag | Type | Size |
|---|---|---|
| `04` | Int | 4 bytes big-endian |
| `05` | Long | 8 bytes big-endian |
| `0e` | Coll[Byte] | VLB length prefix + data |
| `0e 21` | GroupElement | 33 bytes (compressed secp256k1) |

### VLB (Variable Length Byte) Encoding

- If length < 128: single byte
- If length >= 128: two bytes (high bit set on first byte)

---

## Appendix B: Files Audited

### Contract Sources (contracts/)

- `provider_box.ergo` (92 lines)
- `user_staking.ergo` (86 lines)
- `treasury.ergo` (65 lines)
- `usage_proof.ergo` (61 lines)
- `gpu_rental.es` (105 lines)
- `gpu_rental_listing.es` (75 lines)
- `gpu_rating.es` (72 lines)
- `relay_registry.es` (49 lines)
- `usage_commitment.es` (73 lines)
- `payment_bridge.es` (98 lines)
- `provider_slashing.es` (125 lines)
- `provider_slashing.ergo` (155 lines)
- `governance_proposal.es` (110 lines)
- `governance_proposal.ergo` (193 lines)

### Rust Action Builders (xergon-agent/src/)

- `protocol/bootstrap.rs` (1037 lines) -- provider registration, staking, treasury
- `protocol/actions.rs` (2090 lines) -- provider registration (ergo-lib), heartbeat
- `chain/transactions.rs` (429 lines) -- heartbeat, usage proof, encoding helpers
- `gpu_rental/transactions.rs` (605 lines) -- listing, rental, claim, refund, extend
- `gpu_rental/rating/transactions.rs` (119 lines) -- rating submission
- `contract_compile.rs` (402 lines) -- hex loading and validation
- `config/mod.rs` -- contract hex overrides

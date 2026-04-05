# Xergon Network -- ErgoScript Smart Contracts

## Overview

Xergon Network is a decentralized AI compute network built on the Ergo blockchain. These smart contracts implement the core on-chain logic using the eUTXO (extended Unspent Transaction Output) model. State is maintained in boxes that are spent and recreated with updated registers in each transaction.

All contracts follow the **EIP-4** register convention (R4-R9, densely packed, typed).

---

## Contracts

### 1. Provider State Box (`provider_box.ergo`)

**Purpose:** The central on-chain identity and state for each AI compute provider. Providers register by minting a Singleton NFT (supply=1) and creating their Provider Box. On every heartbeat, the box is spent and recreated with updated metadata.

**Singleton NFT Pattern (EKB best practices):**
- A unique token with supply=1 is minted per provider at registration.
- This NFT travels with the state box across all spend/recreate cycles.
- The NFT ID serves as the provider's permanent on-chain identifier.
- The contract enforces that only one box can hold a given NFT.
- **OUTPUTS(0) convention:** The successor state box must be OUTPUTS(0), following the standard singleton NFT state machine pattern.
- **scriptPreserved check:** The successor must have the same ErgoTree, preventing NFT hijacking by moving it to a weaker guard.
- **Value preservation:** The successor box must hold >= SELF value.

**Register Layout:**

| Register | Type            | Description                          |
|----------|-----------------|--------------------------------------|
| R4       | GroupElement    | Provider public key (authorization)  |
| R5       | Coll[Byte]      | Endpoint URL (UTF-8)                 |
| R6       | Coll[Byte]      | Models served (JSON array string)    |
| R7       | Int             | PoNW score (0-1000)                  |
| R8       | Int             | Last heartbeat block height          |
| R9       | Coll[Byte]      | Region (UTF-8)                       |

**Spending Conditions:**
1. `proveDlog(providerPk)` -- only the provider can spend.
2. `OUTPUTS(0)` must carry the same Provider NFT.
3. `OUTPUTS(0)` must be guarded by the same ErgoTree (`scriptPreserved`).
4. `OUTPUTS(0)` must have the same provider public key in R4.
5. `OUTPUTS(0)` value must be >= SELF value (value preservation).
6. `OUTPUTS(0)` R8 (heartbeat height) must be >= current R8 (monotonicity).

---

### 2. User Staking Box (`user_staking.ergo`)

**Purpose:** Holds ERG as a prepaid balance for inference requests. The box value IS the balance -- no separate ledger or credit system is needed. This is a pure eUTXO approach.

**Register Layout:**

| Register | Type         | Description                        |
|----------|--------------|------------------------------------|
| R4       | GroupElement | User public key (authorization)    |

**Spending Conditions:**
- **Path 1 (Normal):** User spends via `proveDlog(userPk)` for inference payment or withdrawal. At least one output must carry the same R4 pk, enforcing the balance-tracking staking box convention on-chain.
- **Path 2 (Cleanup):** Anyone can spend after 4 years (1,051,200 blocks) for storage rent cleanup.

---

### 3. Usage Proof Box (`usage_proof.ergo`)

**Purpose:** On-chain receipt created after each inference request. These are one-way records -- they are created but never spent during normal operation. They serve as an immutable audit trail.

**Register Layout:**

| Register | Type         | Description                        |
|----------|--------------|------------------------------------|
| R4       | Coll[Byte]   | User pk hash (blake2b256)          |
| R5       | Coll[Byte]   | Provider NFT ID                    |
| R6       | Coll[Byte]   | Model name (UTF-8)                 |
| R7       | Int          | Token count (response tokens)      |
| R8       | Long         | Request timestamp (Unix ms)        |

**Spending Conditions:**
- Only after storage rent expiry (4 years / 1,051,200 blocks from creation).

---

### 4. Treasury Box (`treasury.ergo`)

**Purpose:** Holds the protocol treasury ERG reserve and the Xergon Network NFT. Only the deployer can spend this box (for airdrops, funding, etc.).

**Register Layout:**

| Register | Type | Description                        |
|----------|------|------------------------------------|
| R4       | Long | Total airdropped amount (nanoERGs) |

**Tokens:**
- Xergon Network NFT (supply=1) at index 0 -- protocol identity.

**Spending Conditions:**
1. `proveDlog(deployerPk)` -- only the deployer can spend.
2. The output carrying the Xergon Network NFT must also preserve the same ErgoTree (`scriptPreserved`), preventing NFT hijacking.

> **Note:** The deployer address is a placeholder (`DEPLOYER_ADDRESS_HERE`). Replace it with the actual Ergo address before deployment.

---

## Contract Interaction Flow

```
                          Atomic Transaction
                    ┌─────────────────────────┐
                    │                         │
    User Staking    │  1. Spend User Staking  │   Usage Proof
    Box (balance)   │     Box (deduct ERG)    │   Box (receipt)
         ──────►    │                         │   ◄──────
                    │  2. Create Usage Proof  │
                    │     Box (new receipt)   │
                    │                         │
                    │  3. Update Provider Box │   Provider
                    │     (heartbeat + pay)   │   State Box
                    │                         │   ◄──────
                    └─────────────────────────┘
```

**Inference Payment Flow (single atomic transaction):**

1. **Spend** the User's Staking Box (value reduced by inference cost).
2. **Create** a new Usage Proof Box with receipt data (user hash, provider NFT, model, tokens, timestamp).
3. **Spend and recreate** the Provider Box with updated R8 (heartbeat) and receive payment ERG.
4. **Create** a new User Staking Box with the remaining balance.
5. Optionally, create fee outputs for miners.

All of this happens in a single Ergo transaction, ensuring atomicity -- either all steps succeed or none do.

---

## Singleton NFT Pattern

The Singleton NFT pattern is central to Xergon's design:

1. **Minting:** When a provider first registers, a new token with `supply = 1` is minted. The token ID becomes the provider's permanent on-chain identifier.

2. **State Binding:** The NFT is placed in the Provider Box at token index 0. The contract enforces that the NFT must be preserved in the output box.

3. **Uniqueness:** Because supply=1, only one box can hold a given NFT at any time. This prevents double-registration attacks.

4. **Identity:** The NFT ID is referenced by Usage Proof Boxes (R5) to link receipts to specific providers, even without knowing the provider's current public key.

5. **Protocol NFT:** The Treasury Box holds the Xergon Network NFT (supply=1), which serves as the protocol's on-chain identity.

6. **OUTPUTS(0) Convention:** The successor state box must be `OUTPUTS(0)` in every transaction. This follows the EKB singleton NFT state machine pattern and simplifies box discovery.

7. **scriptPreserved Check:** The successor must carry the same ErgoTree as SELF (`OUTPUTS(0).propositionBytes == SELF.propositionBytes`). This prevents NFT hijacking -- an attacker who temporarily gains access to a private key cannot move the NFT to a weaker guard script.

8. **Value Preservation:** The successor box must hold at least as much ERG as the current box (`OUTPUTS(0).value >= SELF.value`). This prevents draining value from state boxes.

---

## Storage Rent Considerations

Ergo implements a storage rent mechanism where boxes that remain unspent for too long can be claimed by anyone, with the box value going to the claimer. This affects Xergon contracts in several ways:

| Contract         | Rent Protection | Rationale                                      |
|------------------|-----------------|-------------------------------------------------|
| Provider Box     | N/A             | Actively spent on every heartbeat (~every 30 blocks). Never expires. |
| User Staking     | 4 years         | Inactive users' boxes can be cleaned after 4 years. Balance is recoverable. |
| Usage Proof      | 4 years         | Receipts persist for 4 years before cleanup. Sufficient for auditing. |
| Treasury Box     | N/A             | Actively spent for airdrops. Never expires.     |

**Key design decisions:**
- **Provider Boxes** are self-maintaining because heartbeats reset the creation height.
- **Usage Proof accumulation** is the main UTXO bloat concern. At high volume, proof boxes will accumulate. The 4-year expiry ensures eventual cleanup. Off-chain indexing (e.g., Explorer API) is recommended for querying historical proofs.
- **Minimum box value** (~0.001 ERG) means users must maintain at least this much in their Staking Box. If balance drops below, the box cannot be created.

---

## Deployment Notes

1. **Compile contracts** using the Ergo Scala compiler or AppKit with the appropriate ErgoScript compiler version.
2. **Replace** `DEPLOYER_ADDRESS_HERE` in `treasury.ergo` with the actual deployer Ergo address.
3. **Parameterization:** The deployer public key in the Treasury contract is the only hardcoded value. All other contracts read authorization from box registers (R4), making them fully dynamic.
4. **Testing:** Use the Ergo Testnet (or local node with `--regtest`) to validate contract behavior before mainnet deployment.
5. **ErgoTree serialization:** After compilation, each contract produces an ErgoTree (serialized bytecode). This is what gets embedded in box guard scripts on-chain.

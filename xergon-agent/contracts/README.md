# Xergon Network -- ErgoScript Contracts

## Overview

This directory contains the 5 core ErgoScript contracts that power the Xergon
decentralized AI inference network on Ergo. Together they implement:

- Provider registration and state management (heartbeat / singleton NFT pattern)
- User ERG staking for prepaid inference credits
- Immutable usage-proof receipts (per-request)
- Protocol treasury with deployer-only access
- NFT minting for new provider onboarding

---

## Contract Summary

| # | Contract File | Purpose | Spending Guard |
|---|---------------|---------|----------------|
| 1 | `provider_registration.es` | One-time bootstrap: mint Provider NFT + create Provider Box | Anyone (open registration) |
| 2 | `provider_box.es` | Per-provider state box with heartbeat | Provider PK (R4) |
| 3 | `user_staking.es` | User ERG deposit for prepaid inference | User PK (R4), or anyone after rent period |
| 4 | `usage_proof.es` | Per-request receipt box | None (create-only, `sigmaProp(true)`) |
| 5 | `treasury_box.es` | Protocol treasury for airdrops | Deployer PK (R4) |

---

## Register Layouts

### 1. provider_box.es -- Provider State Box

| Register | Type | Content |
|----------|------|---------|
| R4 | GroupElement | Provider public key (used for `proveDlog` guard) |
| R5 | Coll[Byte] | Endpoint URL (UTF-8 encoded) |
| R6 | Coll[Byte] | Models served (JSON array, UTF-8) |
| R7 | Int | PoNW (Proof-of-Network-Work) score |
| R8 | Int | Last heartbeat block height |
| R9 | Coll[Byte] | Region code (UTF-8) |

**Tokens:** One Provider NFT (token ID = `blake2b256(first_input_box_id)`, amount = 1)

**Spending logic:**
- Must prove knowledge of the private key corresponding to R4.
- An output box must exist that carries the same Provider NFT (singleton preservation).
- The provider recreates this box with updated R7/R8 on each heartbeat.

### 2. user_staking.es -- User Staking Box

| Register | Type | Content |
|----------|------|---------|
| R4 | GroupElement | User public key |

**Tokens:** None

**Value:** ERG balance = prepaid inference credit.

**Spending logic:**
- Only the user (proveDlog of R4) can spend.
- After 1,051,200 blocks (~4 years), anyone can claim storage rent.

### 3. usage_proof.es -- Usage Proof (Receipt)

| Register | Type | Content |
|----------|------|---------|
| R4 | Coll[Byte] | User PK hash (blake2b256 of public key bytes) |
| R5 | Coll[Byte] | Provider NFT ID (32 bytes) |
| R6 | Coll[Byte] | Model name (UTF-8) |
| R7 | Int | Token count (inference tokens consumed) |
| R8 | Long | Request timestamp (epoch milliseconds) |

**Tokens:** None

**Spending logic:** None (`sigmaProp(true)`). This is a create-only receipt box.
It accumulates in the UTXO set and is eventually cleaned up by storage rent
after ~4 years.

### 4. treasury_box.es -- Protocol Treasury

| Register | Type | Content |
|----------|------|---------|
| R4 | GroupElement | Deployer / governance public key |
| R5 | Long | Total ERG airdropped so far |

**Tokens:** One Xergon Network NFT (amount = 1, minted at protocol genesis)

**Spending logic:**
- Only the deployer (proveDlog of R4) can spend.
- An output box must exist that carries the Xergon Network NFT.

### 5. provider_registration.es -- Provider Registration (Bootstrap)

No registers on the registration box itself. This contract is placed on a
**temporary funding box** that is spent once to create the Provider Box.

**Output validation (enforced on the transaction):**
- `OUTPUTS(0)` must contain a newly minted NFT whose ID = `INPUTS(0).id`.
- `OUTPUTS(0)` must hold >= 1,000,000 nanoERG (0.001 ERG minimum box value).
- `OUTPUTS(0).R4` must be a defined GroupElement (the provider's public key).

After spending, the new Provider Box is protected by `provider_box.es`.

---

## Interaction Flow

```
 1. Provider Registration
    provider_registration.es  --spend-->  provider_box.es (new NFT minted)

 2. User Staking
    User sends ERG  -->  user_staking.es box created (user PK in R4)

 3. Inference Request
    User spends from user_staking.es  -->  pays provider  -->  usage_proof.es created

 4. Provider Heartbeat
    Provider spends provider_box.es  -->  recreates with updated R7/R8 (same NFT)

 5. Treasury Airdrop
    Deployer spends treasury_box.es  -->  distributes ERG  -->  recreates treasury
```

---

## ErgoScript Conventions Used

- **proveDlog(pk)** -- Schnorr signature proof; proves knowledge of the private
  key for the given `GroupElement`.
- **SELF** -- The box whose script is currently being evaluated.
- **OUTPUTS** -- All output boxes in the transaction.
- **INPUTS(0).id** -- The box ID of the first input (used for token minting rule).
- **HEIGHT** -- Current block height (used for storage-rent expiry).
- **SELF.creationHeight** -- Block height when the box was created.
- **sigmaProp(...)** -- Every contract's entry point; wraps a boolean condition
  into a `SigmaProp` type.

---

## Deployment Notes

1. **Treasury Box** is created at protocol genesis with the Xergon Network NFT.
2. **Provider Registration** boxes are deployed for each new provider during
   bootstrap.
3. **Provider Boxes** are long-lived singleton boxes that follow the Oracle Pool
   heartbeat pattern (spend-and-recreate each heartbeat interval).
4. **User Staking Boxes** and **Usage Proof Boxes** are created dynamically by
   the Xergon agent software and/or user wallets.
5. All contracts use densely-packed registers starting at R4 (no gaps).

---

## License

See top-level LICENSE file.

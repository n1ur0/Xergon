# Compiled Contract Hex Values

This directory contains pre-compiled ErgoTree hex for each ErgoScript contract.

## How contracts are compiled

Contracts are compiled from the `.es` source files in the parent `contracts/` directory
using the Ergo Playground or AppKit compiler. The compiled ErgoTree hex is the
serialized bytecode that gets embedded in box guard scripts on-chain.

Alternative: the project includes `src/bin/compile_contracts.rs` which compiles
contracts via a running Ergo node's REST API (`POST /script/p2sAddress`). This
requires a local Ergo node at http://127.0.0.1:9053.

## Recompiling a contract

1. Open the Ergo Playground: https://wallet.ergoplatform.com/playground
2. Paste the contract source from the corresponding `../{contract}.es` file
3. Compile (the playground shows the compiled ErgoTree hex)
4. Copy the hex string and save to `{contract}.hex` in this directory
5. Verify the hex is valid by running: `xergon-agent run --validate-contracts`

Or with a local Ergo node:

```bash
cargo run --bin compile_contracts -- --contracts-dir contracts --output-dir contracts/compiled
```

Note: `compile_contracts.rs` currently only handles the original 11 contracts.
`provider_slashing` and `governance_proposal` must be compiled via the Playground.

## File format

Each `.hex` file contains a single line: the base16-encoded ErgoTree hex string.
No comments, no whitespace, no headers -- just the raw hex.

## Validation

The contract_compile module in xergon-agent validates hex files on startup:
- Checks the hex string is valid base16
- Checks minimum length (at least 64 hex chars / 32 bytes)
- Logs warnings for any contracts that fail validation

---

## Stale / Needs Recompilation

The following hex files are **STALE** -- the corresponding `.es` source files have
been fixed/updated since the hex was last compiled. These files need to be
recompiled using the Ergo Playground or a local Ergo node before deployment:

| Hex File | Source File | Status |
|----------|-------------|--------|
| `provider_slashing.hex` | `../provider_slashing.es` | **STALE** -- source fixed (syntax corrections). Recompile needed. |
| `governance_proposal.hex` | `../governance_proposal.es` | **STALE** -- source fixed. Recompile needed. |

### Why they can't be recompiled locally right now

1. **No local Ergo node running**: The `compile_contracts.rs` binary requires a
   running Ergo node at localhost:9053 to compile ErgoScript via the node's
   `/script/p2sAddress` endpoint. No node is currently available.

2. **No offline compiler available**: ErgoScript requires the Appkit/Sigma compiler
   (a JVM-based tool) to produce ErgoTree bytecode. The `ergo-lib-python` and
   `ergo-lib` (Rust) libraries can only *parse and evaluate* existing ErgoTree,
   not compile from ErgoScript source.

3. **No public compiler API**: The Ergo Playground (wallet.ergoplatform.com) is a
   browser-only tool with no public HTTP API for programmatic compilation.

### What to do

- Use the **Ergo Playground** (https://wallet.ergoplatform.com/playground) to
  manually compile `provider_slashing.es` and `governance_proposal.es`, then
  paste the resulting hex into the corresponding `.hex` files.

- Or start a local Ergo node and add `provider_slashing` and `governance_proposal`
  to the `CONTRACT_NAMES` list in `src/bin/compile_contracts.rs`, then run:
  `cargo run --bin compile_contracts`

### Important: Placeholder values

- `provider_slashing.es` uses `PK("TREASURY_ADDRESS_HERE")` as a placeholder
  treasury address. The compiled hex with this placeholder is NOT deployable.
  Replace with the actual treasury address before final compilation.

- Both contracts use placeholder token IDs (zero hashes). The compiled hex is
  structurally valid for testing but must be recompiled with real token IDs
  before mainnet deployment.

---

## Up-to-date contracts

The following hex files are current and match their source:

- `provider_box.hex`
- `provider_registration.hex`
- `treasury_box.hex`
- `usage_proof.hex`
- `user_staking.hex`
- `gpu_rental.hex`
- `usage_commitment.hex`
- `relay_registry.hex`
- `gpu_rating.hex`
- `gpu_rental_listing.hex`
- `payment_bridge.hex`

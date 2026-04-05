# Compiled Contract Hex Values

This directory contains pre-compiled ErgoTree hex for each ErgoScript contract.

## How contracts are compiled

Contracts are compiled from the `.es` source files in the parent `contracts/` directory
using the Ergo Playground or AppKit compiler. The compiled ErgoTree hex is the
serialized bytecode that gets embedded in box guard scripts on-chain.

## Recompiling a contract

1. Open the Ergo Playground: https://wallet.ergoplatform.com/playground
2. Paste the contract source from the corresponding `../{contract}.es` file
3. Compile (the playground shows the compiled ErgoTree hex)
4. Copy the hex string and save to `{contract}.hex` in this directory
5. Verify the hex is valid by running: `xergon-agent run --validate-contracts`

## File format

Each `.hex` file contains a single line: the base16-encoded ErgoTree hex string.
No comments, no whitespace, no headers -- just the raw hex.

## Validation

The contract_compile module in xergon-agent validates hex files on startup:
- Checks the hex string is valid base16
- Checks minimum length (at least 64 hex chars / 32 bytes)
- Logs warnings for any contracts that fail validation

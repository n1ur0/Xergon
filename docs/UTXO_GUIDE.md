# UTXO Guide for Xergon

## What is UTXO?
Unspent Transaction Output (UTXO) is the transaction model used by Ergo and Xergon.

## Key Concepts
1. **Boxes**: UTXOs on Ergo blockchain
2. **Spending Conditions**: ErgoTree scripts
3. **Tokens**: Custom assets on Ergo

## Xergon UTXOs

### Provider Registration Box
- Contains: Provider ID, GPU capacity, bond
- Spending: Provider can update or withdraw

### Inference Request Box
- Contains: Request details, payment amount
- Spending: Provider can claim after completion

### Settlement Box
- Contains: Completed proofs, rewards
- Spending: Provider can withdraw rewards

## Implementation
See `xergon-relay/src/utxo_builder.rs` for box construction.

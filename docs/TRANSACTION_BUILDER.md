# Transaction Builder

## Overview
Xergon's transaction builder constructs Ergo transactions for:
- Provider registration
- Inference requests
- Settlement payments

## Usage
```rust
use xergon_sdk::transaction_builder::TransactionBuilder;

let builder = TransactionBuilder::new();
let tx = builder
    .add_registration_box(provider_id, gpu_capacity, bond)
    .add_fee(0.1) // ERG
    .build()?;
```

## Features
- Automatic fee calculation
- Box selection optimization
- ErgoTree compilation

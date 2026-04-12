# Smart Contracts

## Overview
Xergon uses ErgoScript smart contracts for:
- Provider registration
- Inference requests
- Settlement logic

## Key Contracts

### Provider Contract
```ergo
{
  val pk = INPUTS(0).R4[GroupElement]
  pk == OUTPUTS(0).id
}
```

### Inference Contract
```ergo
{
  val proof = OUTPUTS(0).R4[GroupElement]
  verifyProof(proof)
}
```

## Testing
All contracts tested in `contracts/tests/`.

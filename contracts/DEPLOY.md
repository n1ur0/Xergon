# Xergon Network -- Contract Deployment Guide

Step-by-step guide for deploying Xergon Network smart contracts to the Ergo testnet.

---

## Prerequisites

| Requirement | Details |
|---|---|
| Ergo node 5.0+ | Fully synced, testnet mode, wallet unlocked |
| curl | For API calls to the node |
| jq | For parsing JSON responses |
| Testnet ERG | Minimum 0.1 ERG for full deployment |
| Deployer address | A valid Ergo testnet address (P2PK) |

### Install tools

```bash
# macOS
brew install curl jq

# Ubuntu/Debian
sudo apt install curl jq
```

---

## 1. Get Testnet ERG

You need testnet ERG to pay for transaction fees and fund contract boxes.

1. Install **Nautilus Wallet** browser extension
2. Switch to **Testnet** mode: Settings > Advanced > "Use Testnet"
3. Get testnet ERG from a faucet:
   - https://testnet.ergofaucet.org/
   - https://faucet.ergo-platform.com
   - https://t.me/ErgoTestnetFaucetBot (Telegram)

You need at least **0.1 ERG** for deploying all contracts.

---

## 2. Set Up an Ergo Testnet Node

### Option A: Run your own (recommended)

```bash
# Download latest Ergo release
wget https://github.com/ergoplatform/ergo/releases/latest/download/ergo-5.0.12.jar

# Run in testnet mode
java -jar ergo-5.0.12.jar --testnet -c /dev/null
```

Wait for "Node is synced" in the logs (1-2 hours on first sync).

### Option B: Use an existing node

If you have access to a synced testnet node, point the deploy script at it:

```bash
export ERGO_NODE_URL="http://NODE_IP:9053"
export ERGO_API_KEY="your-api-key-if-required"
```

### Verify node is ready

```bash
# Check connectivity and sync status
curl http://127.0.0.1:9053/info | jq '{height: .fullHeight, network: .network, syncing: .syncStatus}'

# Check wallet is unlocked
curl http://127.0.0.1:9053/wallet/status | jq '{initialized: .isInitialized, unlocked: .isUnlocked}'
```

---

## 3. Configure the Deployer

The deployer address is required for contracts that have hardcoded authorization:

- **treasury.ergo** -- only the deployer can spend the treasury box
- **provider_slashing.ergo** -- slash penalties route to the treasury address

```bash
# Set your deployer address (Nautilus testnet address)
export DEPLOYER_ADDRESS="3WwxnK...your-testnet-address"

# If treasury address differs from deployer, set it separately
# (defaults to DEPLOYER_ADDRESS if not set)
export TREASURY_ADDRESS="3WwxnK...your-treasury-address"
```

---

## 4. Dry Run (Compile Only)

Before deploying, verify all contracts compile successfully:

```bash
cd contracts
./scripts/deploy-testnet.sh --dry-run --verbose
```

This compiles each contract via the node's `/script/p2sAddress` endpoint and prints the resulting P2S addresses. No transactions are sent.

Expected output:

```
[INFO]  Compiling provider_box.ergo...
[OK]    provider_box.ergo -> 2r6o...P2S-address
[OK]    user_staking.ergo -> 3kF7...P2S-address
[WARN]  DRY RUN: skipping box funding for provider_box
...
[OK]    Deployment manifest written to: deployment-manifest.json
```

---

## 5. Deploy All Contracts

Once dry run succeeds, deploy for real:

```bash
# Review what will be deployed
./scripts/deploy-testnet.sh --dry-run

# Deploy (with confirmation prompt)
./scripts/deploy-testnet.sh

# Or skip the prompt
./scripts/deploy-testnet.sh --force
```

The script will:
1. Compile each contract via `/script/p2sAddress`
2. Substitute placeholder addresses (DEPLOYER_ADDRESS_HERE, TREASURY_ADDRESS_HERE)
3. Fund each contract box via `/wallet/payment/send`
4. Write a deployment manifest to `deployment-manifest.json`

### Deploy only the treasury

```bash
./scripts/deploy-testnet.sh --treasury-only
```

---

## 6. Verify Deployment

### Check the deployment manifest

```bash
cat deployment-manifest.json | jq '.contracts[] | {name, status, p2s_address, tx_id}'
```

### Verify on testnet explorer

For each deployed contract, open the P2S address on the explorer:

```
https://testnet.ergoplatform.com/en/addresses/<P2S_ADDRESS>
```

You should see the funded box with the correct value and guard script.

### Verify box contents via node API

```bash
# Replace with a P2S address from your manifest
P2S="2r6o...your-address"

# Search for boxes at this address
curl "http://127.0.0.1:9053/utxo/byErgoTree/${P2S}" | jq '.'
```

---

## 7. Contract Register Layout Reference

All contracts follow the **EIP-4** register convention (R4-R9, densely packed, typed).

For the full register layout of each contract, see:

- [docs/CONTRACT_REGISTER_AUDIT.md](../docs/CONTRACT_REGISTER_AUDIT.md) -- Complete audit of all register layouts
- [contracts/README.md](README.md) -- Contract overview and interaction flow

### Quick reference

| Contract | R4 | R5 | R6 | R7 | R8 | R9 | Tokens |
|---|---|---|---|---|---|---|---|
| provider_box | ProviderPK (GE) | EndpointURL (Bytes) | Models+Pricing (Bytes) | PoNW (Int) | HeartbeatH (Int) | Region (Bytes) | ProviderNFT (1) |
| user_staking | UserPK (GE) | | | | | | |
| usage_proof | UserPkHash (Bytes) | ProviderNFT ID (Bytes) | Model (Bytes) | TokenCount (Int) | Timestamp (Long) | | |
| treasury | AirdroppedTotal (Long) | | | | | | XergonNFT (1) |
| governance_proposal | ProposalCount (Int) | ActiveID (Int) | VoteThreshold (Int) | TotalVoters (Int) | EndHeight (Int) | DataHash (Bytes) | GovNFT (1) |
| payment_bridge | BuyerPK (SigmaProp) | ProviderPK (SigmaProp) | Amount (Long) | ForeignTxId (Bytes) | Chain (Int) | BridgePK (SigmaProp) | InvoiceNFT (1) |
| gpu_rental | ProviderPK (GE) | RenterPK (GE) | Deadline (Int) | ListingBoxId (Bytes) | StartHeight (Int) | HoursRented (Int) | RentalNFT (1) |
| gpu_rental_listing | ProviderPK (GE) | GPUType (Bytes) | VRAM (Int) | PricePerHour (Long) | Region (Bytes) | Available (Int) | ListingNFT (1) |
| gpu_rating | RaterPK (GE) | RatedPK (GE) | Role (Bytes) | RentalBoxId (Bytes) | Rating (Int) | CommentHash (Bytes) | |
| provider_slashing | ProviderPK (GE) | UptimePct (Int) | StakeAmt (Long) | WindowEnd (Int) | Slashed (Int) | | SlashToken (1) |
| relay_registry | RelayPK (SigmaProp) | Endpoint (Bytes) | Heartbeat (Int) | | | | RelayNFT (1) |
| usage_commitment | ProviderPK (SigmaProp) | EpochStart (Int) | EpochEnd (Int) | ProofCount (Int) | MerkleRoot (Bytes) | | CommitmentNFT (1) |

---

## 8. Security Checklist (Before Mainnet)

Run through this checklist before any mainnet deployment.

### Pre-deployment

- [ ] All contracts pass dry-run compilation on mainnet-equivalent node
- [ ] DEPLOYER_ADDRESS is a **hardware wallet** or multi-sig (not a hot key)
- [ ] TREASURY_ADDRESS is set correctly for provider_slashing.ergo
- [ ] `DEPLOYER_ADDRESS_HERE` and `TREASURY_ADDRESS_HERE` placeholders are replaced
- [ ] Reviewed [docs/CONTRACT_REGISTER_AUDIT.md](../docs/CONTRACT_REGISTER_AUDIT.md) for correctness
- [ ] Reviewed [docs/SECURITY_AUDIT.md](../docs/SECURITY_AUDIT.md) for known issues

### Deployment

- [ ] Deploying to mainnet (not testnet) -- verified node `network` field
- [ ] Treasury box funded with sufficient ERG for operations
- [ ] Xergon Network NFT minted with supply=1 in the treasury box
- [ ] All P2S addresses verified on mainnet explorer
- [ ] Deployment manifest saved and backed up

### Post-deployment

- [ ] All box IDs and TX IDs recorded in a secure location
- [ ] Provider NFT token ID saved (if bootstrapping)
- [ ] Treasury NFT token ID saved
- [ ] Relay config updated with compiled contract hex values
- [ ] Agent config updated with contract references
- [ ] Monitoring/alerting set up for treasury box value

### Known security considerations

1. **Single-key deployer** (treasury.ergo): The deployer has full control over the treasury. Consider migrating to a multi-sig committee for production.
2. **Provider PK immutability**: Provider public keys (R4) cannot be rotated on-chain. Key compromise requires new registration.
3. **Usage proof fabrication**: Anyone can create proof boxes (by design -- proofs are created atomically with staking box spends).
4. **Governance vote counting**: Vote threshold is enforced off-chain, not on-chain.
5. **GPU rating validation**: Rental box IDs (R7) are not validated on-chain.

See [docs/SECURITY_AUDIT.md](../docs/SECURITY_AUDIT.md) for the complete security analysis.

---

## Troubleshooting

### "Cannot connect to Ergo node"

```bash
# Verify the node is running
curl http://127.0.0.1:9053/info

# Check if the port is correct (9053 for REST, 9052 for P2P)
export ERGO_NODE_URL="http://127.0.0.1:9053"
```

### "Compilation failed" with placeholder error

The `DEPLOYER_ADDRESS_HERE` placeholder in treasury.ergo causes a deliberate compilation error. Set the environment variable:

```bash
export DEPLOYER_ADDRESS="3WwxnK...your-address"
```

### "Wallet not initialized"

The deploy script needs the node wallet to fund contract boxes. Unlock it:

```bash
# Via node API
curl -X POST http://127.0.0.1:9053/wallet/init -d '{"pass": "your-password"}'
curl -X POST http://127.0.0.1:9053/wallet/unlock -d '{"pass": "your-password"}'
```

### "Insufficient balance"

Check your wallet balance:

```bash
curl http://127.0.0.1:9053/wallet/balances | jq '.nanoErgs'
```

Get more testnet ERG from the faucet (see Step 1).

### API key authentication

If the node requires an API key:

```bash
export ERGO_API_KEY="your-api-key"
```

---

## Environment Variable Reference

| Variable | Default | Description |
|---|---|---|
| `DEPLOYER_ADDRESS` | (none) | Deployer Ergo address (required for treasury.ergo) |
| `TREASURY_ADDRESS` | = DEPLOYER_ADDRESS | Treasury address (for provider_slashing.ergo) |
| `ERGO_NODE_URL` | `http://127.0.0.1:9053` | Ergo node REST API URL |
| `ERGO_API_KEY` | (none) | Node API authentication key |

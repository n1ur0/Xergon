# Xergon Network -- Mainnet Deployment Guide

Comprehensive guide for deploying the Xergon Network protocol to Ergo mainnet.
This is a **production deployment** -- real ERG is at stake.

---

## Table of Contents

1. [Pre-Deployment Checklist](#1-pre-deployment-checklist)
2. [Contract Compilation](#2-contract-compilation)
3. [Running the Bootstrap Script](#3-running-the-bootstrap-script)
4. [Post-Deployment Verification](#4-post-deployment-verification)
5. [Configuration for Mainnet](#5-configuration-for-mainnet)
6. [Provider Onboarding](#6-provider-onboarding)
7. [Security Considerations](#7-security-considerations)
8. [Rollback Procedure](#8-rollback-procedure)
9. [Monitoring](#9-monitoring)
10. [Mainnet vs Testnet Differences](#10-mainnet-vs-testnet-differences)

---

## 1. Pre-Deployment Checklist

Complete every item before proceeding with mainnet deployment.

### 1.1 Ergo Node

| Item | Requirement | How to Verify |
|------|------------|---------------|
| Node version | 5.0+ | `curl $NODE_URL/info \| jq .name` |
| Fully synced | `fullHeight == headersHeight` | `curl $NODE_URL/info \| jq '.fullHeight, .headersHeight'` |
| Peer count | >= 3 | `curl $NODE_URL/info \| jq .peersCount` |
| Network type | Mainnet | `curl $NODE_URL/info \| jq .networkType` |
| Wallet unlocked | Yes | `curl $NODE_URL/wallet/status \| jq .isUnlocked` |

### 1.2 ERG Funding

| Item | Recommended | Minimum |
|------|-------------|---------|
| Treasury Box deposit | 1.0 ERG | 0.001 ERG (min box value) |
| Transaction fee | 0.001 ERG per tx | 0.001 ERG |
| Provider registration | 0.002 ERG | 0.001 ERG |
| Total for full deploy | 2.0 ERG | 1.5 ERG |
| Buffer (recommended) | +0.5 ERG | -- |

> **NOTE:** On mainnet, ERG has real monetary value. Budget accordingly. The
> bootstrap script checks your balance and warns if it's below the recommended
> amount.

### 1.3 Key Management

- [ ] **Back up your Ergo wallet mnemonic** (12-24 words) in a secure offline location
- [ ] **Store the deployer private key** securely (hardware wallet recommended)
- [ ] **Never share** the wallet password or mnemonic
- [ ] **Use a dedicated deployer address** -- not your personal spending address
- [ ] **Write down the NFT token ID** after bootstrap -- it's your protocol identity

### 1.4 Infrastructure

- [ ] Server with at least 4 GB RAM, 20 GB disk
- [ ] Ergo node fully synced (1-2 hours on fast connection, longer on slow ones)
- [ ] Firewall configured: only ports 80/443 (TLS) and 9030 (Ergo P2P) exposed
- [ ] TLS certificate for your relay domain (Let's Encrypt via certbot or Caddy)
- [ ] Monitoring stack ready (Prometheus + Grafana)
- [ ] `jq` and `curl` installed on the deployment machine

### 1.5 Code

- [ ] Built from a tagged release (not a random commit)
- [ ] All unit tests passing: `make test`
- [ ] Integration tests passing: `make check`
- [ ] Reviewed the diff since last testnet deployment
- [ ] Contracts compiled against a mainnet node (not testnet)

---

## 2. Contract Compilation

ErgoScript contracts (`.ergo` and `.es` files) must be compiled to ErgoTree hex
before deployment. The compiled hex is what gets embedded in box guard scripts
on-chain.

### 2.1 Contracts Overview

```
contracts/
  treasury.ergo           # Treasury Box guard (holds protocol NFT + ERG reserve)
  provider_box.ergo       # Provider state box guard (heartbeat, metadata)
  user_staking.ergo       # User staking box guard (ERG balance for inference)
  usage_proof.ergo        # Usage proof box guard (audit trail receipts)
  usage_commitment.es     # Rollup commitment box guard (Merkle root batching)
  gpu_rental.es           # GPU rental contract
  gpu_rating.es           # GPU rental reputation
  relay_registry.es       # Multi-relay registration
  payment_bridge.es       # Cross-chain bridge invoices
  gpu_rental_listing.es   # GPU listing contract
```

### 2.2 Compile via Node API

The Ergo node provides a `/script/p2sAddress` endpoint for compiling
ErgoScript source code to ErgoTree.

```bash
# Ensure node is running on mainnet and synced
export ERGO_NODE_URL=http://127.0.0.1:9053

# Verify node is on mainnet
curl -s $ERGO_NODE_URL/info | jq .networkType
# Expected: "Mainnet"

# Compile all contracts
make compile-contracts

# Or compile individually:
CONTRACTS_DIR="contracts"
COMPILED_DIR="contracts/compiled"
mkdir -p "$COMPILED_DIR"

# Example: compile treasury.ergo
# NOTE: Replace DEPLOYER_ADDRESS_HERE in treasury.ergo with your actual
# deployer address BEFORE compiling.
for contract in "$CONTRACTS_DIR"/*.ergo "$CONTRACTS_DIR"/*.es; do
    name=$(basename "$contract" | sed 's/\.\(ergo\|es\)$//')
    echo "Compiling $name..."
    source=$(cat "$contract")
    hex=$(curl -s -X POST "$ERGO_NODE_URL/script/p2sAddress" \
        -H "Content-Type: application/json" \
        -d "{\"source\": $(echo "$source" | jq -Rs .)}" \
        | jq -r '.address // empty')
    if [[ -n "$hex" ]]; then
        echo "$hex" > "$COMPILED_DIR/${name}.hex"
        echo "  -> $COMPILED_DIR/${name}.hex"
    else
        echo "  -> FAILED"
    fi
done
```

### 2.3 Validate Compiled Contracts

```bash
make validate-contracts

# Or manually verify each hex file:
for hex_file in contracts/compiled/*.hex; do
    name=$(basename "$hex_file")
    hex=$(cat "$hex_file")
    echo "$name: ${#hex} chars, starts with: ${hex:0:10}..."
done
```

### 2.4 Important: Treasury Contract

The `treasury.ergo` contract contains a placeholder `DEPLOYER_ADDRESS_HERE`.
You **must** replace this with your actual deployer Ergo address before
compiling:

```bash
# Edit the contract
sed -i 's/DEPLOYER_ADDRESS_HERE/3WxsWnTKxWP3vv3bP5mS4EYNKzR4cMeE3LY8wG1LbGHmWYC1QJb/' contracts/treasury.ergo

# Then recompile
make compile-contracts
```

---

## 3. Running the Bootstrap Script

The bootstrap script (`scripts/bootstrap-mainnet.sh`) handles the entire
mainnet genesis deployment:

1. Validates prerequisites (node, wallet, balance)
2. Confirms network is mainnet
3. Prompts for confirmation before spending real ERG
4. Calls `xergon-agent bootstrap` to mint the NFT and create the Treasury Box
5. Verifies the deployment on-chain
6. Saves all metadata to `~/.xergon/mainnet-deployment.json`
7. Generates a mainnet config template

### 3.1 Dry Run (Recommended First Step)

Always do a dry run first to see what would happen:

```bash
cd /path/to/Xergon-Network

./scripts/bootstrap-mainnet.sh \
    --dry-run \
    --node-url http://127.0.0.1:9053 \
    --treasury-erg 1.0 \
    --deployer-addr 3WxsWnTKxWP3vv3bP5mS4EYNKzR4cMeE3LY8wG1LbGHmWYC1QJb
```

Expected output:
```
[INFO]  Found xergon-agent: ./target/release/xergon-agent (0.1.0)
[INFO]  Node:      ergo-5.0.12
[INFO]  Network:   Mainnet
[OK]    Mainnet node confirmed.
[OK]    Node is synced.
[OK]    Wallet is unlocked.
[INFO]  Wallet balance: 5.234 ERG
[OK]    Sufficient ERG balance: 5.234 ERG
[DRY-RUN] Would create a bootstrap transaction with the above parameters.
[DRY-RUN] Expected outputs:
[DRY-RUN]   - Xergon Network NFT (singleton, supply=1)
[DRY-RUN]   - Treasury Box with 1.0 ERG + NFT
[OK]    Dry-run complete. No transactions were submitted.
```

### 3.2 Full Deployment

Once satisfied with the dry run, proceed with the real deployment:

```bash
./scripts/bootstrap-mainnet.sh \
    --node-url http://127.0.0.1:9053 \
    --treasury-erg 1.0 \
    --deployer-addr 3WxsWnTKxWP3vv3bP5mS4EYNKzR4cMeE3LY8wG1LbGHmWYC1QJb \
    --treasury-tree $(cat contracts/compiled/treasury.hex)
```

The script will:
1. Show all deployment parameters
2. Display a **red warning banner** about spending real ERG
3. Require you to type `yes` to confirm
4. Execute the bootstrap transaction
5. Wait for on-chain confirmation
6. Print all IDs and explorer links

### 3.3 What to Expect

```
╔══════════════════════════════════════════════════════════╗
║  WARNING: THIS WILL SPEND REAL ERG ON MAINNET           ║
║                                                          ║
║  The following transaction will be submitted:            ║
║  - Mint Xergon Network NFT (singleton, supply=1)         ║
║  - Create Treasury Box with 1.0 ERG                      ║
║  - Network fee: ~0.001 ERG                              ║
║                                                          ║
║  This action is IRREVERSIBLE. The NFT ID is derived      ║
║  from the input box ID and CANNOT be changed later.      ║
╚══════════════════════════════════════════════════════════╝

Type 'yes' to confirm: yes

[INFO]  Calling xergon-agent to mint NFT and create Treasury Box...
...
[OK]    Transaction confirmed on node.
[OK]    NFT found in UTXO set (1 box(es) contain it).

Explorer verification links:
  Transaction: https://explorer.ergoplatform.com/en/transactions/abc123...
  NFT Token:   https://explorer.ergoplatform.com/en/tokens/def456...
  Treasury Box: https://explorer.ergoplatform.com/en/boxes/ghi789...

════════════════════════════════════════════════════════════
            MAINNET DEPLOYMENT COMPLETE
════════════════════════════════════════════════════════════

  Deployer Address:  3WxsWnTKx...
  NFT Token ID:      def4567890abcdef...
  Treasury Box ID:   ghi7890123abcdef...
  Genesis TX ID:     abc1234567890abcd...
```

### 3.4 Script Options

| Option | Default | Description |
|--------|---------|-------------|
| `--dry-run` | off | Show what would happen without spending ERG |
| `--force-testnet` | off | Allow running on testnet (for testing the script) |
| `--node-url` | `http://127.0.0.1:9053` | Ergo node REST URL |
| `--agent-bin` | auto-detect | Path to xergon-agent binary |
| `--treasury-erg` | `1.0` | ERG to lock in Treasury Box |
| `--deployer-addr` | from wallet | Deployer Ergo address |
| `--treasury-tree` | empty | Compiled treasury contract hex |
| `--nft-name` | `XergonNetworkNFT` | NFT token name |
| `--nft-desc` | `Xergon Network Protocol Identity` | NFT description |
| `--skip-verify` | off | Skip on-chain verification |
| `--yes` | off | Skip all confirmation prompts (DANGEROUS) |

---

## 4. Post-Deployment Verification

### 4.1 Check on Explorer

Open these URLs and verify:

1. **Genesis Transaction:**
   `https://explorer.ergoplatform.com/en/transactions/{GENESIS_TX_ID}`
   - Confirm it has 2 outputs (Treasury Box + change)
   - Confirm the first output contains an asset with `amount: 1`

2. **NFT Token:**
   `https://explorer.ergoplatform.com/en/tokens/{NFT_TOKEN_ID}`
   - Confirm `supply: 1`
   - Confirm `name: XergonNetworkNFT`
   - Confirm `type: EIP-004`

3. **Treasury Box:**
   `https://explorer.ergoplatform.com/en/boxes/{TREASURY_BOX_ID}`
   - Confirm it contains the Xergon Network NFT
   - Confirm it holds the expected ERG amount
   - Confirm R4 register is set (total airdropped: 0)

### 4.2 Verify via Node API

```bash
export NODE_URL=http://127.0.0.1:9053
export NFT_TOKEN_ID="your-nft-token-id-here"

# Check UTXO set for the NFT
curl -s "$NODE_URL/utxo/withTokenId/$NFT_TOKEN_ID" | jq

# Should return an array with at least 1 box containing the NFT

# Check specific box
curl -s "$NODE_URL/utxo/byId/{TREASURY_BOX_ID}" | jq
```

### 4.3 Verify Deployment Metadata

```bash
# Check saved metadata
cat ~/.xergon/mainnet-deployment.json | jq

# Confirm all fields are populated (not "UNKNOWN")
cat ~/.xergon/mainnet-deployment.json | jq '
  .treasury.nft_token_id,
  .treasury.treasury_box_id,
  .treasury.genesis_tx_id
'
```

---

## 5. Configuration for Mainnet

### 5.1 Agent Configuration

Edit the generated config at `~/.xergon/mainnet-config.toml`:

```toml
[ergo_node]
# Mainnet node URL (NOT localhost:9053 for remote deployments)
rest_url = "http://127.0.0.1:9053"

[xergon]
provider_id = "xergon-mainnet-01"          # Your unique provider ID
provider_name = "My Xergon Mainnet Node"   # Display name
region = "us-east"                         # Your region
ergo_address = "3WxsWnTKx..."              # Your Ergo address

[api]
listen_addr = "0.0.0.0:9099"
api_key = "$(openssl rand -hex 32)"         # Set a strong API key!

[chain]
heartbeat_tx_enabled = true
usage_proof_tx_enabled = true
provider_nft_token_id = ""                   # Set after provider registration

[contracts]
# Mainnet-compiled ErgoTree hex values
treasury_box_hex = ""                        # From contracts/compiled/treasury.hex
provider_box_hex = ""                        # From contracts/compiled/provider_box.hex
user_staking_hex = ""                        # From contracts/compiled/user_staking.hex
usage_proof_hex = ""                         # From contracts/compiled/usage_proof.hex
```

**Copy to the agent config location:**

```bash
sudo mkdir -p /opt/xergon/config
sudo cp ~/.xergon/mainnet-config.toml /opt/xergon/config/agent.toml
sudo chmod 640 /opt/xergon/config/agent.toml
sudo chown xergon:xergon /opt/xergon/config/agent.toml
```

### 5.2 Relay Configuration

The relay config references compiled contract hex for chain scanning. Update
the relay's `config.toml` for mainnet:

```toml
[chain]
enabled = true
ergo_node_url = "http://127.0.0.1:9053"
scan_interval_secs = 60
# Mainnet-compiled contract hex (from contracts/compiled/)
provider_tree_bytes = ""                     # From provider_box.hex
staking_tree_bytes = ""                      # From user_staking.hex
gpu_listing_tree_bytes = ""                  # From gpu_rental_listing.hex

[balance]
enabled = true
ergo_node_url = "http://127.0.0.1:9053"
staking_tree_bytes = ""                      # From user_staking.hex
min_balance_nanoerg = 1000000                # 0.001 ERG

[rate_limit]
enabled = true
ip_rpm = 30
key_rpm = 120

[auth]
enabled = true
max_age_secs = 300
```

### 5.3 Marketplace Configuration

Update the marketplace `.env.local`:

```bash
NEXT_PUBLIC_API_BASE=/api/v1
NEXT_PUBLIC_NETWORK_TYPE=mainnet
NEXT_PUBLIC_XERGON_AGENT_BASE=http://127.0.0.1:9099
# Nautilus wallet automatically connects to mainnet when not in testnet mode
```

---

## 6. Provider Onboarding

After the protocol is bootstrapped, register as the first provider.

### 6.1 Generate Provider Key

```bash
# The agent generates keys on first run in ~/.xergon/keys/
# Or use your Nautilus wallet's public key:
# Export from Nautilus: Settings > Advanced > Export Public Key
```

### 6.2 Register On-Chain

```bash
./target/release/xergon-agent register \
    --network mainnet \
    --node-url http://127.0.0.1:9053 \
    --provider-pk-hex 02YOUR_PUBLIC_KEY_HEX_33_BYTES \
    --endpoint http://YOUR_PUBLIC_IP:9099 \
    --models '["llama-3.1-8b","mistral-7b"]' \
    --region us-east
```

This creates a Provider Box with:
- R4: Your public key (GroupElement)
- R5: Endpoint URL
- R6: Models served (JSON array)
- R7: PoNW score (starts at 0)
- R8: Last heartbeat height (starts at 0)
- R9: Region
- Tokens: Provider NFT (supply=1)

### 6.3 Start the Agent

```bash
# With the mainnet config
./target/release/xergon-agent serve \
    --config /opt/xergon/config/agent.toml

# Or via systemd
sudo systemctl start xergon-agent
sudo systemctl status xergon-agent
```

### 6.4 Verify Provider Registration

```bash
# Check on explorer
export PROVIDER_NFT_ID="your-provider-nft-id"
curl -s "http://127.0.0.1:9053/utxo/withTokenId/$PROVIDER_NFT_ID" | jq

# Check via relay
curl -s http://127.0.0.1:9090/v1/providers | jq
```

---

## 7. Security Considerations

### 7.1 Real ERG at Stake

On mainnet, every transaction costs real ERG. The bootstrap script includes
multiple safeguards:

- **Network validation:** Refuses to run on testnet without `--force-testnet`
- **Balance check:** Warns if ERG is below recommended amount
- **Confirmation prompt:** Requires typing `yes` before any transaction
- **Dry-run mode:** `--dry-run` shows what would happen without spending
- **Transaction logging:** All TX IDs are logged for explorer verification

### 7.2 Key Security

| Practice | Recommendation |
|----------|---------------|
| Wallet mnemonic | Store in a hardware wallet or offline in a secure location |
| Deployer key | Use a dedicated address, not your personal spending key |
| Wallet password | Use a strong, unique password for the node wallet |
| API keys | Generate with `openssl rand -hex 32`, never reuse |
| Config files | `chmod 640`, owned by `xergon:xergon` |

### 7.3 Contract Security

- **Treasury Box:** Protected by the deployer's public key via `proveDlog`. Only the
  deployer can spend it. The `scriptPreserved` check prevents NFT hijacking.
- **Provider Box:** Protected by the provider's PK. Only the provider can update
  their own box. Value must be preserved (`OUTPUTS(0).value >= SELF.value`).
- **Staking Box:** Protected by the user's PK. After 4 years (1,051,200 blocks),
  anyone can sweep it for storage rent cleanup.
- **Usage Proofs:** One-way records. Never spent during normal operation.

### 7.4 Transaction Safety Guards

The `protocol/tx_safety.rs` module validates every transaction before submission:

- `validate_box_value()` -- Dynamic minimum based on box size (tokens, registers)
- `validate_fee()` -- Bounds check (0.001 ERG min, 0.1 ERG max)
- `validate_address_or_tree()` -- Ergo address prefixes, ErgoTree hex
- `validate_pk_hex()` -- 33-byte compressed secp256k1, 02/03 prefix
- `validate_payment_request()` -- Full JSON validation before node submission

### 7.5 Node Security

- Do not expose port 9053 (Ergo REST API) to the public internet
- Use firewall rules to restrict access to internal networks only
- Keep the node updated to the latest stable release
- Monitor node health (sync status, peer count, disk usage)

---

## 8. Rollback Procedure

### 8.1 Partial Deployment

If the bootstrap fails partway through (e.g., NFT minted but Treasury Box
creation failed):

1. **Check the transaction on explorer** to see what actually happened
2. **The NFT is in the change output** of the failed transaction
3. **The deployer can spend the change output** (it's guarded by their PK)
4. **Try again** with a new bootstrap -- a new NFT will be minted

The bootstrap script exits with code 7 if it detects a partial deployment.

### 8.2 Treasury Box Recovery

If the Treasury Box needs to be moved or the ERG reclaimed:

1. The Treasury Box is guarded by the deployer's PK (`proveDlog(deployerPk)`)
2. Only the deployer can spend it
3. Use the node wallet API to build a spending transaction
4. The `scriptPreserved` check requires the output to have the same ErgoTree
5. If you want to destroy the Treasury Box entirely, the deployer can spend
   the box and send the ERG + NFT to a new box with any guard

```bash
# Emergency: spend the treasury box (requires node wallet access)
# The NFT will move to the output, so the protocol identity is preserved
curl -X POST http://127.0.0.1:9053/wallet/payment/send \
    -H "Content-Type: application/json" \
    -d '{
        "requests": [{
            "address": "YOUR_DESTINATION_ADDRESS",
            "value": "500000000",
            "tokensToBurn": []
        }],
        "fee": "1000000"
    }'
```

### 8.3 Full Reset

To completely reset the mainnet deployment:

1. Spend the Treasury Box to reclaim ERG + NFT
2. Delete `~/.xergon/mainnet-deployment.json`
3. Re-run the bootstrap script

> **WARNING:** The NFT token ID will be different after a reset. Any references
> to the old NFT ID (in configs, off-chain indexes, etc.) must be updated.

---

## 9. Monitoring

### 9.1 Prometheus Metrics

Both the agent and relay expose Prometheus-compatible metrics:

| Endpoint | Service | Port |
|----------|---------|------|
| `/api/metrics` | xergon-agent | 9099 |
| `/v1/metrics` | xergon-relay | 9090 |

**Prometheus scrape config** (monitoring/prometheus.yml):

```yaml
scrape_configs:
  - job_name: "xergon-agent"
    metrics_path: "/api/metrics"
    static_configs:
      - targets: ["host.docker.internal:9099"]
    scrape_interval: 10s

  - job_name: "xergon-relay"
    metrics_path: "/v1/metrics"
    static_configs:
      - targets: ["host.docker.internal:9090"]
    scrape_interval: 10s
```

### 9.2 Mainnet-Specific Alerts

In addition to the standard alerts (see `docs/RUNBOOK.md`), monitor these
mainnet-specific concerns:

| Alert | Severity | Condition | Action |
|-------|----------|-----------|--------|
| `TreasuryBoxMissing` | **critical** | Treasury Box not found in UTXO set | Check deployer key, investigate spending |
| `TreasuryValueLow` | warning | Treasury Box ERG < 0.5 ERG | Fund the treasury for airdrops |
| `WalletBalanceCritical` | **critical** | Node wallet < 0.05 ERG | Fund immediately, heartbeats will fail |
| `NodeDesyncMainnet` | **critical** | Height diff > 100 blocks | Check peers, restart node if needed |
| `ProviderBoxMissing` | **critical** | Provider Box not found in UTXO set | Re-register as provider |
| `HighFeeSpend` | warning | Transaction fee > 0.01 ERG | Investigate unusually high fees |

### 9.3 Alert Rules Example

```yaml
# monitoring/alerts.yml (additions for mainnet)
groups:
  - name: xergon_mainnet
    rules:
      - alert: TreasuryBoxMissing
        expr: xergon_treasury_box_exists == 0
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "Treasury Box is missing from UTXO set"
          description: "The Xergon Network Treasury Box was not found in the UTXO set for 5 minutes. This may indicate the box was spent or the node is desynced."

      - alert: WalletBalanceCritical
        expr: xergon_wallet_balance_nanoerg < 50000000
        for: 10m
        labels:
          severity: critical
        annotations:
          summary: "Node wallet balance critically low"
          description: "Wallet has less than 0.05 ERG. Heartbeat and usage proof transactions will fail."

      - alert: HighFeeSpend
        expr: increase(xergon_tx_fee_nanoerg_total[1h]) > 10000000
        labels:
          severity: warning
        annotations:
          summary: "High transaction fees detected"
          description: "More than 0.01 ERG spent on fees in the last hour."
```

### 9.4 Health Check Schedule

| Check | Frequency | Tool |
|-------|-----------|------|
| Ergo node sync | Every 1 min | Prometheus scrape |
| Wallet balance | Every 5 min | Prometheus alert |
| Treasury Box existence | Every 10 min | Custom script / alert |
| Provider Box existence | Every 10 min | Custom script / alert |
| Agent health | Every 10s | Prometheus scrape |
| Relay health | Every 10s | Prometheus scrape |
| Disk usage | Every 5 min | Node exporter |

---

## 10. Mainnet vs Testnet Differences

| Aspect | Testnet | Mainnet |
|--------|---------|---------|
| **ERG value** | No monetary value | Real money |
| **Network prefix** | `0x02` (addresses start with `3`) | `0x00` (addresses start with `9`) |
| **Explorer** | `testnet.ergoplatform.com` | `explorer.ergoplatform.com` |
| **Node URL** | `http://127.0.0.1:9053` (testnet) | `http://127.0.0.1:9053` (mainnet) |
| **Faucet** | Available (free ERG) | No faucet -- buy on exchange |
| **Block time** | ~2 minutes | ~2 minutes |
| **Transaction fees** | Minimal | Real cost (0.001+ ERG) |
| **NFT IDs** | Different (derived from testnet box IDs) | Different (derived from mainnet box IDs) |
| **Contract hex** | Same source, different compiled output possible | Must compile against mainnet node |
| **Storage rent** | Same rules (4 years / 1,051,200 blocks) | Same rules |
| **P2S address prefix** | `0x02` | `0x00` |
| **P2PK address prefix** | `0x02` | `0x00` |
| **Bootstrap NFT name** | `XergonNetworkNFT` (same) | `XergonNetworkNFT` (same) |
| **Treasury ERG** | 0.05 ERG (recommended) | 1.0 ERG (recommended) |
| **Confirmation prompts** | Optional | **Required** (script enforces) |
| **Monitoring** | Optional | **Required** (real money at stake) |
| **Rate limiting** | Relaxed | Strict (per-IP + per-key) |
| **TLS** | Optional | **Required** |
| **CORS** | `*` (wildcard) | Restricted to your domains |

### Key Takeaways

1. **NFT IDs are different.** The NFT token ID is `blake2b256(first_input_box_id)`.
   Since mainnet and testnet have different box IDs, the NFTs will have different
   IDs even with the same source code.

2. **Addresses look different.** Mainnet addresses start with `9`, testnet with `3`.
   Do not use testnet addresses on mainnet.

3. **Contract hex may differ.** Always compile contracts against the same network
   type you're deploying to. While the source code is the same, subtle differences
   in the Ergo runtime can produce different compiled output.

4. **Real consequences.** On mainnet, a bug in your transaction builder can lose
   real ERG. Always dry-run first, verify on explorer, and monitor your wallet.

---

## References

- [Testnet Deployment Guide](./TESTNET_DEPLOYMENT.md)
- [General Deployment Guide](./DEPLOYMENT.md)
- [Operator Runbook](./RUNBOOK.md)
- [Security Audit](./SECURITY_AUDIT.md)
- [Provider Guide](./PROVIDER_GUIDE.md)
- [Ergo Explorer](https://explorer.ergoplatform.com)
- [Ergo Developer Knowledge Base](https://ergo-kb.vercel.app)
- [EIP-4 Token Standard](https://github.com/ergoplatform/eips/blob/master/eip-4.md)

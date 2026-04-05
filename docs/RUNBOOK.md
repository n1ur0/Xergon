# Xergon Network -- Operator Runbook

Incident response, diagnostics, and operational procedures for Xergon Network operators.

---

## Quick Reference Card

```
=== SERVICE MANAGEMENT ===
systemctl status xergon-relay          # Check relay status
systemctl status xergon-agent           # Check agent status
systemctl restart xergon-relay          # Restart relay
systemctl restart xergon-agent           # Restart agent
journalctl -u xergon-relay -f           # Follow relay logs
journalctl -u xergon-agent -f           # Follow agent logs

=== API ENDPOINTS ===
Relay:
  GET  http://localhost:9090/health       # Liveness (returns "ok")
  GET  http://localhost:9090/ready        # Readiness (200 or 503)
  GET  http://localhost:9090/v1/health    # Detailed health JSON
  GET  http://localhost:9090/v1/metrics   # Prometheus metrics
  GET  http://localhost:9090/v1/providers # Provider list

Agent:
  GET  http://localhost:9099/xergon/health  # Liveness
  GET  http://localhost:9099/api/health     # Enhanced health JSON
  GET  http://localhost:9099/api/metrics    # Prometheus metrics
  GET  http://localhost:9099/xergon/status  # Full status
  GET  http://localhost:9099/xergon/peers   # Peer list

Ergo Node:
  GET  http://localhost:9053/info           # Node info + sync status
  GET  http://localhost:9053/peers          # Connected peers
  GET  http://localhost:9053/wallet/balance # Wallet balance

Monitoring:
  Prometheus:  http://localhost:9090        # (may conflict with relay port)
  Grafana:     http://localhost:3000        # admin / xergon_admin

=== CONFIG PATHS ===
Relay config:  /opt/xergon/config/relay.toml  (env: XERGON_RELAY_CONFIG)
Agent config:  /opt/xergon/config/agent.toml   (env: XERGON_CONFIG)
Agent keys:    ~/.xergon/keys/
Agent data:    /opt/xergon/data/
Peers file:    /opt/xergon/data/xergon-peers.json
Settlement:    /opt/xergon/data/settlement_ledger.json

=== ENV VAR PREFIXES ===
Relay: XERGON_RELAY__SECTION__KEY  (double underscore separator)
Agent:  XERGON__SECTION__KEY       (double underscore separator)
Both:   XERGON_ENV=production
```

---

## Alert Response Playbooks

### AgentDown (critical)

**Alert:** `xergon-agent` unreachable for 1 minute.

**Impact:** No inference can be served from this provider. Relay will route to other providers if available.

**Response:**

```
1. CHECK PROCESS
   systemctl status xergon-agent
   # If inactive: check exit code
   systemctl show xergon-agent --property=ExecMainStatus

2. CHECK LOGS (last 50 lines)
   journalctl -u xergon-agent -n 50 --no-pager

3. CHECK RESOURCES
   free -h                    # Memory
   df -h /opt/xergon/data     # Disk space
   nvidia-smi                 # GPU (if applicable)
   uptime                     # Load

4. COMMON CAUSES:
   a) OOM killed    -> journalctl -k | grep -i oom
                      -> Increase RAM or reduce model size
   b) Config error  -> Check config.toml syntax
                      -> XERGON__ env var typos
   c) Port conflict -> ss -tlnp | grep 9099
   d) Ergo node down -> curl http://localhost:9053/info

5. FIX:
   sudo systemctl restart xergon-agent
   # If persistent failure, check config and try manual run:
   sudo -u xergon XERGON_ENV=production /opt/xergon/bin/xergon-agent serve
```

### RelayDown (critical)

**Alert:** `xergon-relay` unreachable for 1 minute.

**Impact:** All user requests fail. No routing to any provider.

**Response:**

```
1. CHECK PROCESS
   systemctl status xergon-relay

2. CHECK LOGS
   journalctl -u xergon-relay -n 50 --no-pager

3. CHECK RESOURCES
   free -h
   df -h /
   ss -tlnp | grep 9090    # Port conflict?

4. COMMON CAUSES:
   a) OOM killed          -> Increase memory
   b) Config parse error  -> Invalid TOML or env var
   c) Ergo node unreachable -> curl http://localhost:9053/info
   d) Port already in use -> Kill conflicting process

5. FIX:
   sudo systemctl restart xergon-relay
```

### NoActiveProviders (critical)

**Alert:** Zero healthy providers for 5 minutes.

**Impact:** All inference requests return 503 Service Unavailable.

**Response:**

```
1. CHECK PROVIDER LIST
   curl -s http://localhost:9090/v1/providers | jq '.providers[] | {endpoint, healthy}'

2. CHECK CHAIN SCANNER
   journalctl -u xergon-relay -n 100 --no-pager | grep -i "chain\|scan\|provider"
   # Look for scan errors, empty results, or ErgoTree mismatch

3. VERIFY PROVIDER_TREE_BYTES
   # Check the relay config has the correct compiled hex
   grep provider_tree_bytes /opt/xergon/config/relay.toml
   # Should be non-empty and match the compiled contract hex
   cat /opt/xergon/contracts/compiled/provider_box.hex

4. CHECK PROVIDER HEALTH POLLING
   journalctl -u xergon-relay -n 100 --no-pager | grep -i "health\|poll"
   # Relay polls providers every health_poll_interval_secs (default: 30)

5. CHECK INDIVIDUAL AGENTS
   for agent in http://agent1:9099 http://agent2:9099; do
     echo "=== $agent ==="
     curl -s "$agent/xergon/health" | jq .
     curl -s "$agent/api/health" | jq .
   done

6. CHECK ERGO NODE
   curl -s http://localhost:9053/info | jq '{fullHeight, headersHeight, peers}'
   # If desynced, providers may not be visible

7. FIX:
   a) If chain scanner error -> fix provider_tree_bytes hex, restart relay
   b) If agents down -> follow AgentDown playbook
   c) If Ergo node desynced -> follow NodeDesync playbook
   d) If health poll failing -> check network between relay and agents
```

### WalletBalanceLow (warning)

**Alert:** Agent wallet balance below 0.1 ERG (100,000,000 nanoERG) for 10 minutes.

**Impact:** Cannot submit on-chain heartbeats, usage proofs, or settlements. Provider may be marked unhealthy.

**Response:**

```
1. CHECK CURRENT BALANCE
   curl -s http://localhost:9053/wallet/balance | jq .

2. CHECK AGENT METRICS
   curl -s http://localhost:9099/api/metrics | grep wallet_balance

3. FUND THE WALLET
   # From Nautilus wallet or another source, send ERG to the agent's address
   # The address is in the agent config:
   grep ergo_address /opt/xergon/config/agent.toml

   # Or check on-chain balance:
   curl -s "http://localhost:9053/utxo/byAddress/9fDrtPahmtQDAPbq9AccibtZVmyPD8xmNJkrNXBbFDkejkez1kM" \
     | jq '[.[] | {value: .value, tokens: (.tokens | length)}]'

4. RECOMMENDED MINIMUM:
   # Keep at least 1 ERG for operational buffer
   # Heartbeat txs: ~0.001 ERG per tx (every block = ~0.72 ERG/day)
   # Usage proofs:  ~0.001 ERG per proof
   # Settlement:    ~0.003 ERG per settlement tx
```

### NodeDesync (warning)

**Alert:** Ergo node chain height reported as 0 for 5 minutes.

**Impact:** Relay cannot discover providers, agent cannot submit transactions, balance checks fail.

**Response:**

```
1. CHECK NODE STATUS
   curl -s http://localhost:9053/info | jq '{
     fullHeight, headersHeight,
     peers: (.peers | length),
     isSyncing: .isSyncing
   }'

2. CHECK SYNC PROGRESS
   curl -s http://localhost:9053/info | jq '.headersHeight - .fullHeight'
   # If > 0, node is still syncing (normal after restart)

3. CHECK PEERS
   curl -s http://localhost:9053/peers | jq '.[] | {address, name, lastMessage}' | head -20
   # Should have at least 3+ peers

4. RESTART NODE (if stuck)
   sudo systemctl restart ergo-node
   # Monitor sync progress:
   watch 'curl -s http://localhost:9053/info | jq "{fullHeight, headersHeight}"'

5. ADD PEERS (if 0 peers)
   curl -X POST http://localhost:9053/peers/add \
     -H "Content-Type: application/json" \
     -d '{"address": "213.239.193.138:9030"}'
   # Add multiple known mainnet peers

6. CHECK NODE LOGS
   journalctl -u ergo-node -n 100 --no-pager | grep -i "sync\|peer\|error"
```

### HighErrorRate (warning)

**Alert:** > 50% of agent inference requests failing for 5 minutes.

**Impact:** Degraded service quality. Some requests succeed, many fail.

**Response:**

```
1. CHECK AGENT LOGS
   journalctl -u xergon-agent -n 200 --no-pager | grep -i "error\|fail\|timeout"

2. CHECK INFERENCE BACKEND
   # Ollama:
   curl -s http://localhost:11434/api/tags | jq '.models[].name'
   # llama.cpp:
   curl -s http://localhost:8080/health

3. CHECK GPU
   nvidia-smi
   # Look for: OOM, overheating, driver errors

4. CHECK DISK
   df -h /opt/xergon/data
   # Agent may fail if settlement ledger or peers file can't be written

5. CHECK MODEL AVAILABILITY
   curl -s http://localhost:9099/api/health | jq '.models_loaded'
   # Empty list = no models loaded

6. FIX:
   a) Model missing -> ollama pull <model_name>
   b) OOM -> reduce concurrent requests, use smaller model
   c) Backend down -> restart Ollama/llama.cpp
   d) Timeout -> increase inference.timeout_secs in config
```

### RelayHighErrorRate (warning)

**Alert:** > 30% of relay requests returning errors for 5 minutes.

**Impact:** Users experience frequent failures.

**Response:**

```
1. CHECK RELAY LOGS
   journalctl -u xergon-relay -n 200 --no-pager | grep -i "error\|503\|timeout"

2. CHECK UPSTREAM PROVIDERS
   curl -s http://localhost:9090/v1/providers | jq '.providers[] | {endpoint, healthy, latency_ms}'

3. CHECK RELAY METRICS
   curl -s http://localhost:9090/v1/metrics | grep -E "error|latency|provider"

4. CHECK RATE LIMITER
   curl -s http://localhost:9090/v1/metrics | grep rate_limited
   # High rate_limited_total means many requests are being rejected

5. FIX:
   a) All providers unhealthy -> follow NoActiveProviders playbook
   b) High latency -> check provider GPU/network, increase provider_timeout_secs
   c) Rate limiting too aggressive -> adjust rate_limit config
   d) Auth failures -> check auth config, verify signature format
```

### NoNodePeers (warning)

**Alert:** Ergo node has fewer than 3 peers for 10 minutes.

**Impact:** Slow syncing, potentially falling behind network. May affect chain discovery.

**Response:**

```
1. CHECK PEER COUNT
   curl -s http://localhost:9053/peers | jq length

2. CHECK NETWORK CONNECTIVITY
   ping -c 3 213.239.193.138  # Known Ergo mainnet peer
   traceroute 213.239.193.138

3. CHECK DNS
   dig ergoplatform.com
   # Node may need DNS for peer discovery seeds

4. CHECK FIREWALL
   sudo ufw status
   # Ensure port 9030/tcp is open for incoming P2P connections
   sudo ufw allow 9030/tcp

5. ADD PEERS MANUALLY
   curl -X POST http://localhost:9053/peers/add \
     -H "Content-Type: application/json" \
     -d '{"address": "213.239.193.138:9030"}'
   curl -X POST http://localhost:9053/peers/add \
     -H "Content-Type: application/json" \
     -d '{"address": "37.156.24.22:9030"}'
```

### HighLatency (warning)

**Alert:** Inference latency above 30 seconds for 5 minutes.

**Impact:** Users experience slow responses. May trigger timeouts.

**Response:**

```
1. CHECK PROVIDER HEALTH
   curl -s http://localhost:9090/v1/providers | jq '.providers[] | {endpoint, healthy, latency_ms}'

2. CHECK GPU UTILIZATION
   nvidia-smi
   # Look for: high memory usage, high compute, throttling

3. CHECK SYSTEM LOAD
   uptime
   top -bn1 | head -20
   # High CPU/IO may slow inference

4. CHECK NETWORK
   ping -c 5 <agent-ip>
   # Latency between relay and agent should be < 10ms on same network

5. CHECK MODEL SIZE
   # Larger models are inherently slower
   curl -s http://localhost:11434/api/tags | jq '.models[] | {name, size}'

6. FIX:
   a) GPU overloaded -> reduce concurrent requests, add more agents
   b) Large model -> consider quantized variants (Q4_K_M)
   c) System load -> stop other processes, add RAM
   d) Network latency -> co-locate relay and agent
```

---

## Diagnostic Commands

### Check all service statuses

```bash
echo "=== Systemd Services ==="
systemctl is-active xergon-relay xergon-agent ergo-node docker

echo ""
echo "=== Service Details ==="
systemctl status xergon-relay --no-pager -l 2>/dev/null | head -15
echo "---"
systemctl status xergon-agent --no-pager -l 2>/dev/null | head -15
echo "---"
systemctl status ergo-node --no-pager -l 2>/dev/null | head -15

echo ""
echo "=== Listening Ports ==="
ss -tlnp | grep -E '9090|9099|9053|11434|8080|3000'

echo ""
echo "=== Resource Usage ==="
free -h
echo ""
df -h /opt/xergon / 2>/dev/null
echo ""
uptime
```

### View recent logs

```bash
# Relay logs (last 100 lines)
journalctl -u xergon-relay -n 100 --no-pager

# Agent logs (last 100 lines)
journalctl -u xergon-agent -n 100 --no-pager

# Ergo node logs (last 50 lines)
journalctl -u ergo-node -n 50 --no-pager

# All xergon logs, last hour
journalctl --since "1 hour ago" -u xergon-relay -u xergon-agent --no-pager

# Error-only logs
journalctl -u xergon-relay -p err --since "1 hour ago" --no-pager
journalctl -u xergon-agent -p err --since "1 hour ago" --no-pager

# Search for specific patterns
journalctl -u xergon-relay --no-pager | grep -i "error\|panic\|oom"
journalctl -u xergon-agent --no-pager | grep -i "error\|panic\|oom"
```

### Test API endpoints with curl

```bash
# Relay
echo "=== Relay Health ==="
curl -s http://localhost:9090/health
echo ""
curl -s http://localhost:9090/v1/health | jq .
echo ""

echo "=== Relay Readiness ==="
curl -s -o /dev/null -w "HTTP %{http_code}\n" http://localhost:9090/ready

echo "=== Relay Providers ==="
curl -s http://localhost:9090/v1/providers | jq '.providers | length'
echo "total providers"

echo "=== Relay Models ==="
curl -s http://localhost:9090/v1/models | jq .

# Agent
echo "=== Agent Health ==="
curl -s http://localhost:9099/api/health | jq .
echo ""

echo "=== Agent Status ==="
curl -s http://localhost:9099/xergon/status | jq .
echo ""

echo "=== Agent Peers ==="
curl -s http://localhost:9099/xergon/peers | jq '.peers | length'
echo "xergon peers"
```

### Check Ergo node health

```bash
echo "=== Node Info ==="
curl -s http://localhost:9053/info | jq '{
  version,
  fullHeight,
  headersHeight,
  isSyncing,
  peers: (.peers | length),
  bestFullHeaderId
}'

echo ""
echo "=== Node Sync Status ==="
SYNC_DIFF=$(curl -s http://localhost:9053/info | jq '.headersHeight - .fullHeight')
if [ "$SYNC_DIFF" -eq 0 ]; then
  echo "Node is FULLY SYNCED"
else
  echo "Node is SYNCING ($SYNC_DIFF blocks behind headers)"
fi

echo ""
echo "=== Wallet Status ==="
curl -s http://localhost:9053/wallet/status | jq .

echo ""
echo "=== Wallet Balance ==="
curl -s http://localhost:9053/wallet/balance | jq '{nanoerg: .balance, erg: (.balance / 1000000000)}'

echo ""
echo "=== Peer Summary ==="
curl -s http://localhost:9053/peers | jq '[.[] | {address: .address, name: .name}] | length'
echo "connected peers"
```

### Check Prometheus targets

```bash
# List all targets and their health
curl -s http://localhost:9090/api/v1/targets | \
  jq '.data.activeTargets[] | {job: .labels.job, health: .health, lastScrape: .lastScrape}'

# Check if specific jobs are up
curl -s http://localhost:9090/api/v1/targets | \
  jq -r '.data.activeTargets[] | select(.labels.job == "xergon-relay") | .health'
curl -s http://localhost:9090/api/v1/targets | \
  jq -r '.data.activeTargets[] | select(.labels.job == "xergon-agent") | .health'

# Query current provider count
curl -s 'http://localhost:9090/api/v1/query?query=xergon_relay_providers_active' | jq .

# Query error rate
curl -s 'http://localhost:9090/api/v1/query?query=rate(xergon_relay_errors_total[5m])' | jq .

# Query inference latency
curl -s 'http://localhost:9090/api/v1/query?query=xergon_agent_inference_latency_ms' | jq .
```

### Dump current config

```bash
# Relay config
echo "=== Relay Config ==="
cat /opt/xergon/config/relay.toml

# Relay env overrides
echo ""
echo "=== Relay Environment Overrides ==="
env | grep XERGON_RELAY

# Agent config
echo ""
echo "=== Agent Config ==="
cat /opt/xergon/config/agent.toml

# Agent env overrides
echo ""
echo "=== Agent Environment Overrides ==="
env | grep XERGON_
```

---

## Common Operations

### Adding a new provider endpoint

**Static (known_endpoints in relay config):**

```bash
# Edit the relay config
sudo nano /opt/xergon/config/relay.toml

# Add the new endpoint to the [providers] section:
# [providers]
# known_endpoints = [
#     "http://agent1.internal:9099",
#     "http://agent2.internal:9099",
#     "http://new-agent.internal:9099",  # <-- add here
# ]

# Restart relay
sudo systemctl restart xergon-relay
```

**Dynamic (on-chain registration):**

If chain scanning is enabled (`chain.enabled = true`), new providers are discovered automatically when they register on-chain. No relay restart needed.

```bash
# On the new agent, configure and start it
# The agent registers with the relay on start (if relay.register_on_start = true)
# and the relay picks it up on the next chain scan (every scan_interval_secs)
```

### Updating ErgoTree contract bytes

When contracts are upgraded, you need to update the compiled hex in the relay config.

```bash
# 1. Compile new contracts
cd /path/to/Xergon-Network
export ERGO_NODE_URL=http://127.0.0.1:9053
make compile-contracts

# 2. Verify compiled hex
make validate-contracts

# 3. Copy new hex to relay config
PROVIDER_HEX=$(cat contracts/compiled/provider_box.hex)
STAKING_HEX=$(cat contracts/compiled/user_staking.hex)

# 4. Update relay config
sudo sed -i "s/^provider_tree_bytes = .*/provider_tree_bytes = \"$PROVIDER_HEX\"/" \
  /opt/xergon/config/relay.toml
sudo sed -i "s/^staking_tree_bytes = .*/staking_tree_bytes = \"$STAKING_HEX\"/" \
  /opt/xergon/config/relay.toml

# 5. Restart relay
sudo systemctl restart xergon-relay

# 6. Verify providers are discovered
sleep 5
curl -s http://localhost:9090/v1/providers | jq '.providers | length'
```

### Rotating rate limit config

```bash
# Edit relay config
sudo nano /opt/xergon/config/relay.toml

# Or use environment variables (no restart needed if using systemd with env reload):
# [rate_limit]
# ip_rpm = 60        # was 30
# ip_burst = 20      # was 10
# key_rpm = 240      # was 120
# key_burst = 60     # was 30

# Via env vars (requires restart):
sudo systemctl edit xergon-relay
# Add:
# [Service]
# Environment="XERGON_RELAY__RATE_LIMIT__IP_RPM=60"
# Environment="XERGON_RELAY__RATE_LIMIT__IP_BURST=20"

sudo systemctl daemon-reload
sudo systemctl restart xergon-relay

# Verify
curl -s http://localhost:9090/v1/metrics | grep rate_limit
```

### Scaling relay horizontally

```bash
# On the new host:

# 1. Install relay binary
VERSION=$(curl -s https://api.github.com/repos/n1ur0/Xergon-Network/releases/latest | jq -r .tag_name)
wget "https://github.com/n1ur0/Xergon-Network/releases/download/${VERSION}/xergon-linux-amd64.tar.gz"
tar xzf "xergon-${PLATFORM}.tar.gz"
sudo cp xergon-relay-linux-amd64 /opt/xergon/bin/xergon-relay
sudo chmod +x /opt/xergon/bin/xergon-relay

# 2. Copy config from existing relay
scp existing-host:/opt/xergon/config/relay.toml /opt/xergon/config/relay.toml

# 3. Create systemd service (same as existing)
# Copy from existing host or create per DEPLOYMENT.md

# 4. Start
sudo systemctl enable --now xergon-relay

# 5. Verify
curl -s http://localhost:9090/v1/health | jq .

# 6. Add to load balancer
# Add the new host's IP:9090 to your nginx upstream or cloud LB target group
```

### Performing rolling restarts

```bash
# For multiple relay instances behind a load balancer:

# 1. Check current health of all relays
for host in relay1 relay2 relay3; do
  echo -n "$host: "
  curl -s -o /dev/null -w "%{http_code}" "http://$host:9090/ready"
  echo ""
done

# 2. Rolling restart (one at a time)
for host in relay1 relay2 relay3; do
  echo "Restarting $host..."
  ssh $host "sudo systemctl restart xergon-relay"
  echo "Waiting for $host to be ready..."
  for i in $(seq 1 30); do
    STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://$host:9090/ready")
    if [ "$STATUS" = "200" ]; then
      echo "$host is ready after ${i}s"
      break
    fi
    sleep 1
  done
  echo ""
done

# 3. Verify all healthy
for host in relay1 relay2 relay3; do
  echo -n "$host: "
  curl -s "http://$host:9090/v1/health" | jq '{active_providers, total_providers}'
done
```

---

## Troubleshooting Guide

### Relay returns 503: no healthy providers

**Symptoms:** All inference requests return HTTP 503 with `"no healthy providers available"`.

**Diagnosis:**

```bash
# 1. Check provider count
curl -s http://localhost:9090/v1/providers | jq '.providers | length'
# If 0: no providers discovered at all

# 2. Check if chain scanning is working
journalctl -u xergon-relay -n 200 --no-pager | grep -i "scan"
# Look for: "scan complete", "found N providers", "scan error"

# 3. Verify ErgoTree hex is correct
grep provider_tree_bytes /opt/xergon/config/relay.toml
# Must be non-empty and match the compiled contract

# 4. Test provider health directly
curl -s http://agent-host:9099/xergon/health | jq .
# If this fails, the agent itself is down
```

**Fixes:**

| Root Cause | Fix |
|---|---|
| `provider_tree_bytes` is empty | Set to compiled hex from `contracts/compiled/provider_box.hex` |
| Wrong ErgoTree hex (testnet vs mainnet) | Recompile contracts against mainnet node |
| Agent is down | Follow AgentDown playbook |
| Ergo node is desynced | Follow NodeDesync playbook |
| Network partition (relay can't reach agent) | Check firewall, DNS, network routes |
| Health poll failing | Check agent health endpoint, increase timeout |

### Auth failures: signature verification

**Symptoms:** Requests return 401 or 403 with `"invalid signature"` or `"request expired"`.

**Diagnosis:**

```bash
# 1. Check if auth is enabled
grep -A5 '\[auth\]' /opt/xergon/config/relay.toml

# 2. Check relay logs for auth errors
journalctl -u xergon-relay -n 100 --no-pager | grep -i "auth\|signature\|expired"
```

**Fixes:**

| Root Cause | Fix |
|---|---|
| Request timestamp too old (clock skew) | Sync system clocks: `sudo chronyc -a makestep` |
| Replay detection (duplicate nonce) | Client must use unique nonce per request |
| Invalid signature format | Verify client signs correct payload (method + path + timestamp + body) |
| Public key not in staking box | Set `auth.require_staking_box = false` or create staking box |
| Auth disabled on relay but client sends header | No issue -- relay ignores auth headers when disabled |

### Chain scan returns no providers

**Symptoms:** `/v1/providers` returns empty list despite agents running and registered on-chain.

**Diagnosis:**

```bash
# 1. Verify ErgoTree hex matches on-chain boxes
# Get a known provider box from Ergo Explorer or node:
curl -s "http://localhost:9053/utxo/byErgoTree/$(cat /opt/xergon/contracts/compiled/provider_box.hex)" | jq '. | length'
# If 0: no boxes with this ErgoTree exist on chain

# 2. Check if provider is registered on-chain
curl -s "http://localhost:9053/utxo/withTokenId/YOUR_PROVIDER_NFT_ID" | jq .

# 3. Check relay scan logs
journalctl -u xergon-relay -n 200 --no-pager | grep -i "chain.*scan\|provider.*discover\|EIP-1"
```

**Fixes:**

| Root Cause | Fix |
|---|---|
| Wrong network (testnet hex on mainnet) | Recompile contracts with mainnet node |
| Provider not registered on-chain | Run `register_provider.sh` script |
| Contract version mismatch | Ensure relay and agent use same compiled contracts |
| Ergo node not synced | Wait for full sync |
| Scan interval too long | Reduce `scan_interval_secs` temporarily |

### GPU rental transaction stuck

**Symptoms:** GPU rental initiated but transaction never confirms. Agent shows rental session as pending.

**Diagnosis:**

```bash
# 1. Check agent GPU rental status
curl -s http://localhost:9099/xergon/status | jq '.gpu_rental'

# 2. Check the transaction on-chain
# Find the TX ID from agent logs:
journalctl -u xergon-agent -n 200 --no-pager | grep -i "rental.*tx\|gpu.*submit"
TX_ID="extracted-tx-id"
curl -s "http://localhost:9053/transactions/byId/$TX_ID" | jq '{numConfirmations}'

# 3. Check Ergo node mempool
curl -s http://localhost:9053/transactions/unconfirmed/byId/$TX_ID | jq .
```

**Fixes:**

| Root Cause | Fix |
|---|---|
| Transaction not in mempool | Fee too low -- resubmit with higher fee |
| Transaction in mempool but not confirming | Wait (Ergo block time ~2 min); check node peers |
| Node desynced | Follow NodeDesync playbook |
| Rental box already spent | Rental was claimed by someone else; start new rental |
| Insufficient balance | Fund wallet (see WalletBalanceLow playbook) |

### Balance check failing

**Symptoms:** Relay returns 402 or 403 with `"insufficient balance"` even though user has ERG.

**Diagnosis:**

```bash
# 1. Check balance config
grep -A10 '\[balance\]' /opt/xergon/config/relay.toml

# 2. Verify staking_tree_bytes is set
grep staking_tree_bytes /opt/xergon/config/relay.toml
# Must be non-empty for balance checking to work

# 3. Check user's staking boxes on-chain
# Replace USER_PK_HEX with the user's public key hex
curl -s "http://localhost:9053/utxo/withTokenId/USER_STAKING_TOKEN_ID" | jq .

# 4. Test balance endpoint directly
curl -s "http://localhost:9090/v1/balance/USER_PK_HEX" | jq .
```

**Fixes:**

| Root Cause | Fix |
|---|---|
| `staking_tree_bytes` empty | Set to compiled hex from `contracts/compiled/user_staking.hex` |
| User has no staking box | User needs to create a staking box via marketplace or script |
| Balance below minimum | Increase `min_balance_nanoerg` or fund the staking box |
| Ergo node unavailable | Check node status |
| Balance cache stale | Wait for `cache_ttl_secs` or restart relay to clear cache |

### Rate limit false positives

**Symptoms:** Legitimate users get 429 Too Many Requests.

**Diagnosis:**

```bash
# 1. Check current rate limit metrics
curl -s http://localhost:9090/v1/metrics | grep rate_limited

# 2. Check rate limit config
grep -A6 '\[rate_limit\]' /opt/xergon/config/relay.toml

# 3. Check if user is behind NAT/proxy
# (multiple users share one IP, hitting IP-based limits)
journalctl -u xergon-relay -n 200 --no-pager | grep "rate.limit"
```

**Fixes:**

| Root Cause | Fix |
|---|---|
| Limits too low for traffic | Increase `ip_rpm` and `key_rpm` |
| Users behind NAT/proxy | Increase `ip_rpm` and `ip_burst` |
| Bot/crawler traffic | Add rate limiting at CDN/proxy level |
| Health endpoints rate limited | Health endpoints are exempt -- verify path matches |

---

## Escalation Contacts and On-Call Procedures

> **NOTE:** Replace placeholders with your team's actual contact information.

### On-Call Rotation

| Role | Contact | Schedule |
|---|---|---|
| Primary On-Call | `@xergon-oncall-primary` (Slack) | Mon-Fri, 09:00-17:00 UTC |
| Secondary On-Call | `@xergon-oncall-secondary` (Slack) | Mon-Fri, 17:00-09:00 UTC |
| Weekend On-Call | `@xergon-oncall-weekend` (Slack) | Sat-Sun, all day |

### Escalation Matrix

| Severity | Response Time | Escalation Path |
|---|---|---|
| Critical (P1) | 15 minutes | On-Call -> Tech Lead -> Engineering Manager |
| Warning (P2) | 1 hour | On-Call -> Tech Lead |
| Info (P3) | Next business day | Create ticket, discuss in standup |

### Alert Channels

| Channel | Purpose |
|---|---|
| `#xergon-alerts` (Slack) | All alerts are posted here |
| `#xergon-incidents` (Slack) | Active incident coordination |
| ` PagerDuty (placeholder) | Critical alert phone/page |

### Incident Response Process

```
1. DETECT    -- Alert fires in #xergon-alerts
2. ACKNOWLEDGE -- On-call responds within SLA
3. DIAGNOSE  -- Follow the relevant playbook above
4. COMMUNICATE -- Post updates in #xergon-incidents
5. RESOLVE   -- Apply fix, verify service recovery
6. POSTMORTEM -- Create postmortem within 48 hours
```

### Postmortem Template

```
## Incident: [Title]
**Date:** YYYY-MM-DD HH:MM UTC
**Duration:** X hours Y minutes
**Severity:** P1/P2/P3
**Impact:** [What was affected, how many users]

### Timeline
- HH:MM - Alert fired
- HH:MM - On-call acknowledged
- HH:MM - Root cause identified
- HH:MM - Fix applied
- HH:MM - Service restored

### Root Cause
[Description]

### What Went Well
- ...

### What Could Be Improved
- ...

### Action Items
- [ ] @person -- action description (due: date)
```

---

## Glossary

| Term | Definition |
|---|---|
| **Agent** (xergon-agent) | Provider node that runs inference, registers on-chain, manages GPU rentals. Listens on port 9099. |
| **Relay** (xergon-relay) | Thin stateless router that proxies user requests to healthy agents. Listens on port 9090. |
| **Marketplace** (xergon-marketplace) | Next.js web UI for users to interact with the network. Connects Nautilus wallet. |
| **Provider Box** | On-chain UTXO representing a registered provider. Contains public key, endpoint, models, PoNW score. |
| **Provider Tree Bytes** | Hex-encoded ErgoTree of the Provider Box guard script. Used by relay to scan for providers. |
| **Staking Box** | On-chain UTXO where users lock ERG for inference access. |
| **Treasury Box** | On-chain UTXO holding the protocol's ERG reserve. Created during bootstrap. |
| **Usage Proof** | On-chain record proving inference work was performed. Optional, controlled by agent config. |
| **PoNW** (Proof of Network Work) | Scoring system combining node uptime, peer count, and AI inference work. |
| **Heartbeat** | Periodic signal from agent to relay (and optionally on-chain) indicating liveness. |
| **Chain Scan** | Relay periodically queries the Ergo node for Provider Boxes using EIP-1 registered scans. |
| **GPU Rental** | On-chain marketplace for renting GPU capacity. Managed by the agent. |
| **Settlement** | Periodic ERG payment from users to providers based on recorded usage. |
| **ErgoTree** | Ergo's smart contract language. Compiled to hex bytes for on-chain execution. |
| **nanoERG** | Smallest unit of ERG. 1 ERG = 1,000,000,000 nanoERG. |
| **EIP-1** | Ergo Improvement Proposal 1: registered scans (on-chain box scanning via ErgoTree predicates). |
| **Nautilus** | Ergo-compatible wallet browser extension. Used to connect to the marketplace. |
| **Ollama** | Local LLM inference runtime. Default inference backend for xergon-agent. |
| **llama.cpp** | Alternative LLM inference backend. Used via llama-server HTTP API. |
| **SSE** (Server-Sent Events) | Streaming protocol used for real-time inference responses. |
| **NFT** (Non-Fungible Token) | Used in Xergon for unique identifiers: protocol NFT, provider NFTs. |

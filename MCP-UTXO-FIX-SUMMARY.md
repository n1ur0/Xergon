# MCP & UTXO Configuration - Fix Summary

**Date:** 2026-04-11  
**Status:** ⚠️ Partial Fix Complete  
**Action Required:** Start Ergo Node

---

## 🔴 CURRENT ISSUES

### 1. Ergo Node is DOWN
**Status:** ❌ Connection refused at `http://192.168.1.75:9052`

**Symptoms:**
- `curl http://192.168.1.75:9052/info` → Connection refused
- `ping 192.168.1.75` → ✅ Host is reachable (104ms)
- Docker: No ergo containers running

**Root Cause:** Ergo node container is not running or not configured

---

### 2. MCP Server Issues
**Status:** ⚠️ Responding but not functional

**Test Results:**
- `https://ergo-knowledge-base.vercel.app/api/mcp` → 200 OK
- But returns empty/non-JSON responses
- All method calls return "Method not found" or empty data

**Possible Causes:**
- MCP server requires specific authentication
- Server is misconfigured
- Wrong endpoint path
- Server needs initialization

---

## ✅ WHAT WAS FIXED

### 1. Configuration Files Created

**`/home/n1ur0/Xergon-Network/ergo-node-config.yaml`**
```yaml
ergo_node:
  url: http://192.168.1.75:9052
  network: testnet
  api_version: v6.0.3
  endpoints:
    info: /info
    blocks: /blocks/lastHeaders/{n}
    transactions: /transactions/unconfirmed
    boxes: /boxes/unspent
    by_address: /boxes/byAddress/{address}
  fallback_endpoints:
    - https://api.ergoplatform.com

mcp:
  ergo_knowledge:
    url: https://ergo-knowledge-base.vercel.app/api/mcp
    headers:
      Accept: application/json, text/event-stream
    methods: [search, get_concept, list_projects]
```

**`~/.hermes/config.yaml`** - MCP servers updated with correct URLs

---

## 🚀 IMMEDIATE NEXT STEPS

### Step 1: Start Ergo Node (CRITICAL)

**Option A: Docker Compose (if available)**
```bash
cd /home/n1ur0/Xergon-Network

# Check if there's a docker-compose.yml with ergo-node
ls -la docker-compose*.yml

# If found, start it:
docker compose up -d ergo-node

# Or start all services:
docker compose up -d
```

**Option B: Manual Docker Run**
```bash
# Pull Ergo node image
docker pull sigmastate/explorer:latest  # Or appropriate image

# Run the node
docker run -d \
  --name ergo-node \
  -p 192.168.1.75:9052:9052 \
  -p 192.168.1.75:9053:9053 \
  sigmastate/explorer:latest \
  --mainnet false  # Use testnet
```

**Option C: Check Existing Installation**
```bash
# Look for existing Ergo installation
find /home/n1ur0 -name "*ergo*" -type d 2>/dev/null | head -10

# Check for systemd service
systemctl list-units | grep ergo

# Check for manual installation
ls -la /opt/ergo* /usr/local/ergo* 2>/dev/null
```

---

### Step 2: Verify Node is Running

```bash
# Check Docker containers
docker ps | grep ergo

# Check if port is listening
netstat -tlnp | grep 9052
# or
ss -tlnp | grep 9052

# Test the endpoint
curl http://192.168.1.75:9052/info | python3 -m json.tool
```

**Expected Response:**
```json
{
  "network": "testnet",
  "fullHeight": 279108,
  "isExplorer": true,
  ...
}
```

---

### Step 3: Test UTXO Endpoints

Once node is running:

```bash
# Test basic endpoints
curl http://192.168.1.75:9052/info
curl http://192.168.1.75:9052/transactions/unconfirmed

# Test UTXO endpoints (may need valid address)
# Get a valid address first (from miner or test wallet)
curl http://192.168.1.75:9052/miner/address

# Then test boxes
curl "http://192.168.1.75:9052/boxes/byAddress/{valid_address}?limit=5"
```

---

### Step 4: Fix MCP Server

**Option A: Use Public Ergo API as Fallback**
```bash
# Test public API
curl https://api.ergoplatform.com/info
curl https://api.ergoplatform.com/transactions/unconfirmed
```

**Option B: Configure MCP Properly**
The MCP server may need:
1. Correct authentication headers
2. Different endpoint path
3. Server-side configuration

**Try different approaches:**
```python
import requests

# Try with different headers
headers = {
    "Content-Type": "application/json",
    "Accept": "application/json, text/event-stream",
    "Authorization": "Bearer YOUR_TOKEN"  # If required
}

payload = {
    "jsonrpc": "2.0",
    "method": "search_docs",
    "params": {"query": "Ergo boxes", "limit": 5},
    "id": 1
}

resp = requests.post(
    "https://ergo-knowledge-base.vercel.app/api/mcp",
    json=payload,
    headers=headers
)
print(resp.status_code, resp.text)
```

**Option C: Use Alternative MCP Servers**
```yaml
# Update ~/.hermes/config.yaml
mcp_servers:
  ergo-public:
    url: https://api.ergoplatform.com/mcp  # If available
  ergo-transcript:
    url: https://ergo-transcripts.vercel.app/api/mcp
```

---

## 📊 CURRENT STATUS

| Component | Status | Action Needed |
|:----------|:-------|:--------------|
| **Ergo Node** | ❌ DOWN | **START NODE** |
| **MCP Server** | ⚠️ Partial | Reconfigure or use fallback |
| **Xergon Relay** | ✅ 50% Working | Continue cron job |
| **Local Model** | ✅ Working | No action needed |
| **Cron Job** | ✅ Active | Next run in ~3 min |

---

## 🎯 PRIORITY ACTIONS

### CRITICAL (Do Now)
1. **Start Ergo Node** - This is blocking UTXO/settlement
2. **Verify node is running** - Test `/info` endpoint
3. **Test UTXO endpoints** - Verify boxes work

### HIGH (After Node is Up)
4. **Fix MCP server** - Configure authentication or use fallback
5. **Test settlement flow** - Verify UTXO queries work
6. **Update cron job** - Continue with remaining tasks

### MEDIUM (Optional)
7. **Document working endpoints** - Create reference guide
8. **Set up monitoring** - Monitor node health
9. **Configure fallback** - Use public API if local node fails

---

## 📝 QUICK FIX COMMANDS

```bash
# 1. Check if Ergo node exists in docker
docker ps -a | grep ergo

# 2. If exists but stopped, start it
docker start ergo-node

# 3. If doesn't exist, check for docker-compose
cd /home/n1ur0/Xergon-Network
ls -la docker-compose*.yml

# 4. If found, start ergo-node
docker compose up -d ergo-node

# 5. Wait for sync (check logs)
docker logs -f ergo-node

# 6. Test endpoint
curl http://192.168.1.75:9052/info

# 7. If still failing, check network
ping 192.168.1.75
netstat -tlnp | grep 9052
```

---

## 🚨 IF NODE WON'T START

**Check these:**
1. **Port conflicts:** `netstat -tlnp | grep 9052`
2. **Disk space:** `df -h /var/lib/docker`
3. **Memory:** `free -h`
4. **Logs:** `docker logs ergo-node`
5. **Network:** `ping 192.168.1.75`

**Alternative: Use Public API**
If local node can't be started, use:
- `https://api.ergoplatform.com` for explorer data
- Update Xergon config to use public API

---

## 📞 SUPPORT RESOURCES

- **Ergo Docs:** https://docs.ergoplatform.com/
- **Ergo Explorer:** https://ergoscan.org/
- **Node Setup:** https://github.com/ergoplatform/ergo
- **Docker Images:** https://hub.docker.com/r/sigmastate/explorer

---

**Last Updated:** 2026-04-11 02:02 UTC  
**Next Cron Run:** ~3 minutes  
**Status:** Waiting for Ergo node to be started

**ACTION REQUIRED:** Start the Ergo node to enable UTXO/settlement functionality!

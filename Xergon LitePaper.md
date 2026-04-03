# Xergon Network - Lite Paper  
### *A Decentralized Proof-of-Node-Work Network for AI Compute on Ergo*

---

## 1. Overview
**Xergon** is a decentralized network that transforms independent Ergo node operators into AI compute providers.  
Through a mechanism called **Proof-of-Node-Work (PoNW)**, nodes earn credit for uptime, health, peer confirmations, and AI inference.

This enables a marketplace where users can purchase AI compute from distributed, sovereign nodes — without centralized servers, API keys, or cloud platforms.

---

## 2. Motivation
AI today is highly centralized. Cloud providers control:

- Access  
- Pricing  
- Model availability  
- Data privacy  
- Terms of use  

Ergo nodes, on the other hand, are globally distributed, reliable, and cryptographically verifiable but underutilized.

**Xergon bridges this gap** by turning every healthy Ergo node into:

- A **local AI endpoint**  
- A **verifiable compute provider**  
- A **participant in a decentralized AI economy**

---

## 3. Architecture
Each Xergon node consists of three components:

### A. Ergo Node
Provides:

- Sync status  
- Peer list  
- Chain height  
- Network identity  

This ensures the operator is running a real, healthy node.

### B. Xergon Agent
A Rust-based sidecar daemon that:

- Monitors node health  
- Detects peer confirmations  
- Tracks AI token usage  
- Calculates PoNW points  
- Exposes REST endpoints for the marketplace  

### C. Local AI Inference 
Every node hosts one or more OSS models (e.g., GPT-OSS-20B, LLaMA 3, Mistral 7B).

All inference is:

- Local  
- Private  
- Censorship-resistant  
- Independent of cloud APIs  

Token usage becomes part of the node’s **work score**.

---

## 4. Proof-of-Node-Work (PoNW)
PoNW combines three verifiable categories of work:

### 1. Node Work
- Uptime  
- Sync status  
- Tip height accuracy  
- Peer count  
- Recent handshake data  

### 2. Network Work
- How many Xergon peers confirm your node  
- How often you confirm theirs  
- Unique peers seen over time  

### 3. AI Work
- Total requests processed  
- Total tokens generated  
- Model difficulty multipliers  

PoNW creates a score that is:

- Hard to fake  
- Easy for others to verify  
- Tied to real compute  
- Useful for pricing and ranking providers  

---

## 5. Xergon Marketplace
A global UI lists active Xergon providers and displays:

- Provider ID & region  
- Available AI models  
- Pricing in ERG  
- Latency  
- Work Score / reputation  

Users can choose a provider, send prompts, and (future phase) pay in ERG for inference.

No cloud, no middleman — pure P2P compute.

---

## 6. Decentralization Model
Xergon maximizes decentralization by:

- Running inference locally  
- Using peer-derived confirmations  
- Allowing self-hosted provider URLs  
- Supporting open pricing  
- Storing reputation metadata off-chain now, with future NFT/rollup anchors  

Xergon is *not* a new chain — it’s a **network overlay** built on Ergo.

---

## 7. Data Integrity & Reputation
To ensure long-term verifiable reputations:

- The Xergon agent reports periodic stats (signed)
- Marketplace relays store global cumulative metrics
- Peer confirmations strengthen integrity
- Future: optional NFT identities on Ergo
- Future: lightweight rollups for on-chain anchoring  

Local counters reset on restart, but global history never resets.

---

## 8. Roadmap

### **Phase 1 — Core PoNW (Complete)**
- AI token tracking  
- Node/peer validation  
- Provider config API  
- PoNW-scored endpoints
- NFT identity anchors  

### **Phase 2 — Marketplace (In Progress)**
- Provider discovery  
- Model listings  
- Leaderboards  
- Contract-less UX  

### **Phase 3 — Economic Layer**
- ERG payment rails  
- Usage-based billing  
- Verified PoNW histories  

### **Phase 4 — Network Layer**
- Multi-relay discovery  
- P2P reputation sharing  
- Light rollup commitments  

### **Phase 5 — Full Compute Network**
- Trust-minimized on-chain settlements
- Cross-chain integrations

---

## 9. Summary
**Xergon is a decentralized compute protocol that turns Ergo nodes into AI providers.  
By combining node health, network verification, and local AI work into a verifiable score, Xergon enables a global marketplace for distributed AI inference powered by Proof-of-Node-Work.**

Powered by Degens World https://degens.world

Marketplace UI - 

Statistics:

<img width="732" height="661" alt="image" src="https://github.com/user-attachments/assets/da97aa9b-0a34-4f61-b641-c5907b385b06" />

<img width="745" height="598" alt="image" src="https://github.com/user-attachments/assets/b7ebf92a-3226-4bda-aecf-2051c29166db" />

AI Inference Console:

<img width="1391" height="736" alt="image" src="https://github.com/user-attachments/assets/2828cab2-6c58-460d-9526-815640d251e9" />

Agent API Example:

{
  "node": {
    "node_healthy": true,
    "node_synced": true,
    "has_enough_peers": true,
    "peer_count": 8,
    "local_height": 1664931,
    "best_height": 1664931,
    "last_updated_ms": 1764199912054,
    "ai_enabled": true,
    "ai_model": "qwen3.5-4b-f16.gguf",
    "ai_total_tokens": 648,
    "ai_points": 0,
    "provider": {
      "id": "Xergon_LT",
      "name": "Xergon_LT",
      "region": "us-east",
      "public_node_id": null
    }
  },
  "pown_status": {
    "ai_enabled": true,
    "ai_model": "qwen3.5-4b-f16.gguf",
    "ai_points": 0,
    "ai_total_requests": 1,
    "ai_total_tokens": 648,
    "ai_weight": 1,
    "ergo_address": "9fDrtPahmtQDAPbq9AccibtZVmyPD8xmNJkrNXBbFDkejkez1kM",
    "last_agreement": 0,
    "last_tick_ts": 1764199906,
    "node_id": "7ab304becbd1ad3649e4020a848f979f3b4a418b441d5d8354fe4ecc2524f709",
    "peers_checked": 0,
    "total_xergon_confirmations": 0,
    "unique_xergon_peers_seen": 0,
    "work_points": 3521,
    "xergon_peers": []
  },
  "pown_health": {
    "best_height_local": 1664931,
    "ergo_address": "9fDrtPahmtQDAPbq9AccibtZVmyPD8xmNJkrNXBbFDkejkez1kM",
    "is_synced": true,
    "last_header_id": null,
    "node_height": 1664931,
    "node_id": "7ab304becbd1ad3649e4020a848f979f3b4a418b441d5d8354fe4ecc2524f709",
    "peer_count": 8,
    "timestamp": 1764199915
  },
  "epoch": null,
  "provider": {
    "id": "Xergon_LT",
    "name": "Xergon_LT",
    "region": "us-east",
    "public_node_id": null
  },
  "provider_models": [
    {
      "name": "qwen3.5-4b-f16.gguf",
      "price_per_1k_tokens_erg": 1,
      "max_context": 1000
    }
  ],
  "llama_server_models": [
    "qwen3.5-4b-f16.gguf"
  ],
  "wallet_token": {
    "confirmed": true,
    "node_id": "15af1d8651a83e50ebfc9c80450ff65a959e4e9ef81d0f4f2a792c648ab2e20c",
    "ergo_nano": 0,
    "matched_tokens": [],
    "token_metadata": {
      "boxId": "71b37ed925e618d70b0b829e0d68c86712cd8c25f274e96d3aeb4a2f22cb4024",
      "decimals": 0,
      "description": "Xergon node identity NFT",
      "emissionAmount": 1,
      "id": "15af1d8651a83e50ebfc9c80450ff65a959e4e9ef81d0f4f2a792c648ab2e20c",
      "name": "Xergon Node - Xergon_LT",
      "type": "EIP-004"
    },
    "raw": {
      "assets": {
        "15af1d8651a83e50ebfc9c80450ff65a959e4e9ef81d0f4f2a792c648ab2e20c": 1
      },
      "balance": 1216771643,
      "height": 1664931
    },
    "error": null
  }
}





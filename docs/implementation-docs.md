# Xergon Network - Implementation Documentation

**Generated:** 2026-04-10  
**Scope:** Detailed implementation guides for features that exist in code but lack documentation  
**Based on:** Actual implementation in Xergon-Network repository

---

## 1. Internationalization (i18n) / Localization (L10n)

### 1.1 Overview

Xergon Marketplace implements full internationalization with **4 locales** and a **1,359-line translation dictionary**.

**Location:** `xergon-marketplace/lib/i18n/`

### 1.2 Architecture

```
xergon-marketplace/lib/i18n/
├── config.ts           # i18n configuration
├── dictionary.ts       # 1,359-line translation dictionary
├── hooks/
│   ├── use-t.ts        # Translation hook
│   └── useLocale.ts    # Locale management hook
└── stores/
    └── localeStore.ts  # Zustand locale state
```

### 1.3 Configuration

```typescript
// lib/i18n/config.ts

export const supportedLocales = ['en', 'ja', 'zh', 'es'] as const;
export type Locale = (typeof supportedLocales)[number];

export const defaultLocale: Locale = 'en';

export const localeNames: Record<Locale, string> = {
  en: 'English',
  ja: '日本語',
  zh: '中文',
  es: 'Español',
};

// Browser detection
export function detectLocale(): Locale {
  const browserLang = navigator.language.slice(0, 2);
  return supportedLocales.includes(browserLang as Locale)
    ? (browserLang as Locale)
    : defaultLocale;
}
```

### 1.4 Translation Dictionary

```typescript
// lib/i18n/dictionary.ts

export const dictionaries = {
  en: {
    common: {
      welcome: 'Welcome to Xergon',
      loading: 'Loading...',
      error: 'An error occurred',
    },
    playground: {
      title: 'AI Playground',
      message: 'Enter your prompt',
      send: 'Send',
      streaming: 'Streaming...',
    },
    models: {
      title: 'Available Models',
      select: 'Select a model',
      price: '{price} nanoERG per 1K tokens',
    },
    // ... 1,359 translation keys total
  },
  ja: {
    common: {
      welcome: 'Xergon へようこそ',
      loading: '読み込み中...',
      error: 'エラーが発生しました',
    },
    // ... Japanese translations
  },
  zh: {
    // ... Chinese translations
  },
  es: {
    // ... Spanish translations
  },
};

export function getDictionary(locale: Locale) {
  return dictionaries[locale];
}
```

### 1.5 Usage in Components

```typescript
// Using the translation hook
import { useT } from '@/lib/i18n/hooks/use-t';

export function WelcomeBanner() {
  const t = useT();
  
  return (
    <div>
      <h1>{t('common.welcome')}</h1>
      <p>{t('playground.message')}</p>
      <button>{t('common.loading')}</button>
    </div>
  );
}

// Using locale store
import { useLocale } from '@/lib/i18n/stores/localeStore';

export function LanguageSwitcher() {
  const { locale, setLocale } = useLocale();
  
  return (
    <select value={locale} onChange={(e) => setLocale(e.target.value)}>
      {supportedLocales.map((loc) => (
        <option key={loc} value={loc}>
          {localeNames[loc]}
        </option>
      ))}
    </select>
  );
}
```

### 1.6 SSR Support

```typescript
// app/layout.tsx (Next.js 15)

export async function generateMetadata({ params }: Props) {
  const locale = params.locale || 'en';
  const dict = getDictionary(locale);
  
  return {
    title: dict.metadata.title,
    description: dict.metadata.description,
  };
}
```

### 1.7 Adding New Locales

```bash
# 1. Add locale to config.ts
export const supportedLocales = ['en', 'ja', 'zh', 'es', 'de'] as const;

# 2. Add dictionary entry in dictionary.ts
de: {
  common: {
    welcome: 'Willkommen bei Xergon',
    // ... all keys
  },
},

# 3. Update localeNames
localeNames.de = 'Deutsch';

# 4. Translate all keys (use AI or human translator)
```

---

## 2. Cross-Chain Bridge

### 2.1 Overview

Xergon implements a **Rosen-bridge-style cross-chain bridge** supporting 6 chains with commit-reveal watchers and fraud proofs.

**Location:** `xergon-relay/src/cross_chain_bridge.rs` (689 lines)

### 2.2 Supported Chains

```rust
// cross_chain_bridge.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SupportedChain {
    Ergo,
    Ethereum,
    Cardano,
    Bitcoin,
    BSC,
    Polygon,
}

impl SupportedChain {
    pub fn all() -> Vec<Self> {
        vec![
            SupportedChain::Ergo,
            SupportedChain::Ethereum,
            SupportedChain::Cardano,
            SupportedChain::Bitcoin,
            SupportedChain::BSC,
            SupportedChain::Polygon,
        ]
    }
}
```

### 2.3 Bridge Architecture

```
┌──────────────┐      ┌──────────────┐      ┌──────────────┐
│  Source Chain│─────►│    Bridge    │─────►│Destination  │
│  (Ergo)      │      │   Contract   │      │  Chain (ETH) │
└──────────────┘      └──────────────┘      └──────────────┘
       │                     │                     │
       │ 1. Lock tokens      │                     │
       │────────────────────>│                     │
       │                     │ 2. Watchers confirm │
       │                     │────────────────────>│
       │                     │                     │ 3. Mint tokens
       │                     │                     │<─────────────
       │                     │ 4. Fraud proof window│
       │                     │<────────────────────│
       │                     │                     │
       │ 5. If no fraud,     │                     │
       │    release tokens   │                     │
       │<────────────────────│                     │
```

### 2.4 Bridge API

```rust
// xergon-relay/src/handlers/bridge.rs

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeInvoice {
    pub invoice_id: String,
    pub source_chain: SupportedChain,
    pub destination_chain: SupportedChain,
    pub amount: u64,
    pub recipient_address: String,
    pub status: BridgeStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeRequest {
    pub source_chain: SupportedChain,
    pub destination_chain: SupportedChain,
    pub amount: u64,
    pub recipient_address: String,
    pub proof_data: String,
}

// Endpoint: POST /v1/bridge/invoice
pub async fn create_invoice(
    Json(request): Json<BridgeRequest>,
    State(state): State<AppState>,
) -> Result<Json<BridgeInvoice>, Error> {
    let invoice = state.bridge.create_invoice(request).await?;
    Ok(Json(invoice))
}

// Endpoint: GET /v1/bridge/invoice/:id
pub async fn get_invoice(
    Path(invoice_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<BridgeInvoice>, Error> {
    let invoice = state.bridge.get_invoice(&invoice_id).await?;
    Ok(Json(invoice))
}

// Endpoint: POST /v1/bridge/confirm
pub async fn confirm_bridge(
    Json(proof): Json<ProofData>,
    State(state): State<AppState>,
) -> Result<Json<BridgeInvoice>, Error> {
    let invoice = state.bridge.confirm(proof).await?;
    Ok(Json(invoice))
}
```

### 2.5 SDK Usage

```typescript
// @xergon/sdk/src/bridge.ts

export class BridgeClient {
  private relayUrl: string;
  
  async createInvoice(request: BridgeRequest): Promise<BridgeInvoice> {
    const response = await fetch(`${this.relayUrl}/v1/bridge/invoice`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
    
    return response.json();
  }
  
  async getInvoice(invoiceId: string): Promise<BridgeInvoice> {
    const response = await fetch(`${this.relayUrl}/v1/bridge/invoice/${invoiceId}`);
    return response.json();
  }
  
  async confirmBridge(proof: ProofData): Promise<BridgeInvoice> {
    const response = await fetch(`${this.relayUrl}/v1/bridge/confirm`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(proof),
    });
    
    return response.json();
  }
}
```

### 2.6 Fraud Proof System

```rust
// cross_chain_bridge.rs (simplified)

pub struct BridgeWatcher {
    pub chain: SupportedChain,
    pub contract_address: String,
    pub last_scanned_block: u64,
}

impl BridgeWatcher {
    pub async fn scan_for_invoices(&self) -> Result<Vec<BridgeEvent>, Error> {
        // Scan source chain for lock events
        let events = self.contract.get_lock_events(self.last_scanned_block).await?;
        
        // Verify proofs
        for event in &events {
            if !self.verify_proof(&event.proof) {
                return Err(Error::FraudDetected);
            }
        }
        
        Ok(events)
    }
    
    pub async fn submit_fraud_proof(&self, invoice_id: String) -> Result<(), Error> {
        // Submit fraud proof to bridge contract
        let tx = self.build_fraud_proof_tx(invoice_id);
        self.chain_client.submit_transaction(tx).await?;
        Ok(())
    }
}
```

---

## 3. Governance System

### 3.1 Overview

Xergon implements an **on-chain governance system** with CLI tools for proposing, voting, and executing proposals.

**Location:** 
- Contracts: `contracts/governance_proposal.es`, `governance_proposal_v2.es`
- SDK: `xergon-sdk/src/governance.ts` (410 lines)
- Agent API: `xergon-agent/src/api/governance.rs`

### 3.2 Governance Lifecycle

```
1. Proposal Creation
   │
   │ xergon governance propose \
   │   --title "Add new model" \
   │   --description "Add Qwen-72B" \
   │   --category "model_addition"
   │
   ▼
2. On-chain Proposal Box
   │
   ├─► Mint proposal NFT
   ├─► Set voting period (7 days)
   └─► Open for voting
   │
   ▼
3. Voting Phase
   │
   ├─► Stakeholders vote (YES/NO/ABSTAIN)
   ├─► Weighted by staked ERG
   └─► Track tally on-chain
   │
   ▼
4. Execution Phase
   │
   ├─► If passed: Execute proposal
   ├─► Update protocol state
   └─► Consume proposal box
   │
   ▼
5. Result Recording
   │
   └─► Record outcome on-chain
```

### 3.3 CLI Commands

```bash
# Propose new governance proposal
xergon governance propose \
  --title "Add Qwen-72B model" \
  --description "Add Qwen-72B model to provider offerings" \
  --category model_addition \
  --voting_period 7d \
  --proposal-data '{"model": "qwen-72b", "price": 100}'

# Vote on proposal
xergon governance vote \
  --proposal-id <proposal-nft-id> \
  --vote YES \
  --stake 1000000000  # 1 ERG

# Check proposal status
xergon governance status <proposal-id>

# Execute passed proposal
xergon governance execute <proposal-id>

# List active proposals
xergon governance list --status active

# Delegate voting power
xergon governance delegate \
  --to <delegate-address> \
  --amount 500000000  # 0.5 ERG
```

### 3.4 SDK API

```typescript
// @xergon/sdk/src/governance.ts

export class GovernanceClient {
  private relayUrl: string;
  private wallet: ErgoWallet;
  
  async propose(proposal: ProposalRequest): Promise<Proposal> {
    const response = await fetch(`${this.relayUrl}/v1/governance/proposals`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        ...proposal,
        signature: await this.signProposal(proposal),
      }),
    });
    
    return response.json();
  }
  
  async vote(proposalId: string, vote: VoteType, stake: bigint): Promise<VoteResult> {
    const response = await fetch(`${this.relayUrl}/v1/governance/proposals/${proposalId}/vote`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        vote,
        stake: stake.toString(),
        signature: await this.signVote(proposalId, vote),
      }),
    });
    
    return response.json();
  }
  
  async getProposal(proposalId: string): Promise<Proposal> {
    const response = await fetch(`${this.relayUrl}/v1/governance/proposals/${proposalId}`);
    return response.json();
  }
  
  async listProposals(options: ListOptions = {}): Promise<Proposal[]> {
    const params = new URLSearchParams(options as Record<string, string>);
    const response = await fetch(`${this.relayUrl}/v1/governance/proposals?${params}`);
    return response.json();
  }
  
  async execute(proposalId: string): Promise<ExecutionResult> {
    const response = await fetch(`${this.relayUrl}/v1/governance/proposals/${proposalId}/execute`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        signature: await this.signExecution(proposalId),
      }),
    });
    
    return response.json();
  }
}
```

### 3.5 Contract Structure

```ergoscript
// contracts/governance_proposal.es

defprop ProposalBox(
  proposer: SigmaDht,
  proposalData: Coll[Byte],
  votingPeriod: Long,
  category: String,
  totalStake: Long,
  yesVotes: Long,
  noVotes: Long,
  stage: Int  // 0=Voting, 1=Executing, 2=Completed
) = {
  // NFT token as singleton
  val nftToken = INPUTS.filter { b =>
    b.token(0).id == SELF.id
  }.headOption
  
  // Voting period check
  val isVotingPeriodActive = HEIGHT < proposer.creationHeight + votingPeriod
  
  // Quorum check
  val hasQuorum = totalStake > 10000000000  // 10 ERG minimum
  
  // Majority check
  val passed = yesVotes > noVotes && yesVotes > totalStake / 2
  
  // Spending conditions
  val canVote = isVotingPeriodActive && !hasPassed
  val canExecute = !isVotingPeriodActive && passed && stage == 0
  
  sigmaProp {
    // Proposer can spend to update state
    (proposer && stage == 1) ||
    // Anyone can execute if passed
    (canExecute && hasQuorum)
  }
}
```

### 3.6 Proposal Categories

```typescript
export type ProposalCategory = 
  | 'model_addition'        // Add new AI models
  | 'price_change'          // Change pricing
  | 'parameter_update'      // Update protocol params
  | 'treasury_spending'     // Spend treasury funds
  | 'relay_addition'        // Add new relays
  | 'contract_upgrade'      // Upgrade contracts
  | 'emergency_stop'        // Emergency protocol stop
  | 'custom';               // Custom proposals
```

---

## 4. Oracle Pool Integration

### 4.1 Overview

Xergon integrates with **Oracle Pool** for ERG/USD price feeds and other oracle data.

**Location:** `xergon-relay/src/oracle_consumer.rs`, `xergon-sdk/src/oracle-client.ts`

### 4.2 Oracle Price Feed

```rust
// oracle_consumer.rs

pub struct OracleConsumer {
    pub oracle_box_id: String,
    pub refresh_interval_secs: u64,
    pub cache: Arc<RwLock<OracleCache>>,
}

impl OracleConsumer {
    pub async fn get_erg_usd_rate(&self) -> Result<Decimal, Error> {
        // Read from cache first
        if let Some(rate) = self.cache.read().await.erg_usd_rate {
            if !rate.is_expired() {
                return Ok(rate.value);
            }
        }
        
        // Fetch from oracle box
        let oracle_box = self.chain_client.get_box(&self.oracle_box_id).await?;
        let rate = self.decode_rate(&oracle_box)?;
        
        // Update cache
        self.cache.write().await.erg_usd_rate = Some(CachedRate {
            value: rate,
            expires_at: Utc::now() + Duration::seconds(60),
        });
        
        Ok(rate)
    }
    
    pub async fn get_oracle_status(&self) -> Result<OracleStatus, Error> {
        let box_info = self.chain_client.get_box(&self.oracle_box_id).await?;
        
        Ok(OracleStatus {
            is_healthy: box_info.is_valid(),
            last_update_height: box_info.creation_height,
            oracle_count: self.get_oracle_count().await?,
        })
    }
}
```

### 4.3 SDK Usage

```typescript
// @xergon/sdk/src/oracle-client.ts

export class OracleClient {
  private relayUrl: string;
  
  async getErgUsdRate(): Promise<number> {
    const response = await fetch(`${this.relayUrl}/v1/contracts/oracle/rate`);
    const data = await response.json();
    return data.rate;
  }
  
  async getOracleStatus(): Promise<OracleStatus> {
    const response = await fetch(`${this.relayUrl}/v1/contracts/oracle/status`);
    return response.json();
  }
  
  async getHistoricalRate(hours: number): Promise<RateHistory[]> {
    const response = await fetch(
      `${this.relayUrl}/v1/contracts/oracle/rate/history?hours=${hours}`
    );
    return response.json();
  }
}
```

### 4.4 API Endpoints

```yaml
GET /v1/contracts/oracle/rate:
  Response: { rate: 12.45, timestamp: "2026-04-10T12:00:00Z" }

GET /v1/contracts/oracle/status:
  Response: {
    is_healthy: true,
    last_update_height: 1664931,
    oracle_count: 25
  }

GET /v1/contracts/oracle/rate/history:
  Query: hours (default: 24)
  Response: [{ timestamp, rate }, ...]
```

---

## 5. GPU Bazar Marketplace

### 5.1 Overview

GPU Bazar is the **GPU rental marketplace** with on-chain listings, rental contracts, and rating system.

**Contracts:** `gpu_rental.es`, `gpu_rental_listing.es`, `gpu_rating.es`

### 5.2 Rental Flow

```
1. Provider Lists GPU
   │
   │ xergon gpu list \
   │   --model "RTX 4090" \
   │   --vram 24 \
   │   --price 50000000  # 0.05 ERG/hour
   │
   ▼
2. On-chain Listing Box
   │
   ├─► GPU details (model, VRAM, region)
   ├─► Pricing (nanoERG per hour)
   └─► Availability status
   │
   ▼
3. User Rents GPU
   │
   │ xergon gpu rent <listing-id> \
   │   --duration 2h \
   │   --payment 100000000  # 0.1 ERG
   │
   ▼
4. SSH Tunnel Created
   │
   ├─► Provider creates SSH tunnel
   ├─► User gets tunnel credentials
   └─► Session starts
   │
   ▼
5. Metering & Expiry
   │
   ├─► Agent monitors session
   ├─► Timer counts down
   └─► On expiry: close tunnel
   │
   ▼
6. Rating (Optional)
   │
   │ xergon gpu rate <session-id> \
   │   --rating 5 \
   │   --comment "Great performance"
```

### 5.3 SDK API

```typescript
// @xergon/sdk/src/gpu.ts

export class GPUClient {
  private relayUrl: string;
  
  async listGPUs(filters: GPUBrowseFilters): Promise<GPUListing[]> {
    const params = new URLSearchParams(filters as Record<string, string>);
    const response = await fetch(`${this.relayUrl}/v1/gpu/listings?${params}`);
    return response.json();
  }
  
  async rentGPU(request: RentGPURequest): Promise<Session> {
    const response = await fetch(`${this.relayUrl}/v1/gpu/rent`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
    
    return response.json();
  }
  
  async extendSession(sessionId: string, duration: number): Promise<Session> {
    const response = await fetch(`${this.relayUrl}/v1/gpu/extend`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ sessionId, duration }),
    });
    
    return response.json();
  }
  
  async claimSession(sessionId: string): Promise<Session> {
    const response = await fetch(`${this.relayUrl}/v1/gpu/claim/${sessionId}`, {
      method: 'POST',
    });
    
    return response.json();
  }
  
  async refundSession(sessionId: string): Promise<RefundResult> {
    const response = await fetch(`${this.relayUrl}/v1/gpu/refund`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ sessionId }),
    });
    
    return response.json();
  }
  
  async rateGPU(request: RateGPURequest): Promise<void> {
    await fetch(`${this.relayUrl}/v1/gpu/rate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
  }
}
```

---

## 6. References

- **i18n Dictionary:** `xergon-marketplace/lib/i18n/dictionary.ts` (1,359 lines)
- **Bridge Implementation:** `xergon-relay/src/cross_chain_bridge.rs` (689 lines)
- **Governance SDK:** `xergon-sdk/src/governance.ts` (410 lines)
- **Oracle Consumer:** `xergon-relay/src/oracle_consumer.rs`
- **GPU Rental:** `contracts/gpu_rental.es`, `gpu_rental_listing.es`
- **OpenAPI Spec:** `docs/openapi.yaml`

---

**Last Updated:** 2026-04-10  
**Verified Against:** Xergon-Network main branch

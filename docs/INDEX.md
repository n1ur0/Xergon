# 📘 Xergon Network - Technical Documentation

> **Decentralized AI Inference Marketplace on Ergo Blockchain**

---

## 📖 Table of Contents

### 🚀 Getting Started
- [**Introduction**](./INTRODUCTION.md) - What is Xergon?
- [**Quick Start**](./QUICK-START.md) - Get running in 5 minutes
- [**Local Setup Guide**](./LOCAL-SETUP-GUIDE.md) - Complete setup instructions
- [**Architecture Overview**](./docs/architecture.md) - System design & components

### 🏗️ Core Components
- [**Xergon Relay**](./xergon-relay/README.md) - Backend API (Rust)
- [**Xergon Agent**](./xergon-agent/README.md) - PoNW Sidecar (Rust)
- [**Xergon Marketplace**](./xergon-marketplace/README.md) - Frontend UI (Next.js)
- [**Xergon SDK**](./xergon-sdk/README.md) - TypeScript Integration

### 🔗 Ergo Blockchain Integration
- [**Ergo Node Setup**](./docs/ERGO_NODE_SETUP.md) - Connecting to Ergo
- [**Smart Contracts**](./docs/SMART_CONTRACTS.md) - ErgoScript implementation
- [**UTXO Management**](./docs/UTXO_GUIDE.md) - Box creation & spending
- [**Transaction Building**](./docs/TRANSACTION_BUILDER.md) - Building Ergo transactions
- [**Provider Registration**](./docs/PROVIDER_ONBOARDING.md) - Registering AI providers

### 🛠️ Development
- [**API Reference**](./docs/API_REFERENCE.md) - REST API documentation
- [**SDK Documentation**](./xergon-sdk/README.md) - TypeScript SDK usage
- [**Testing Guide**](./docs/TESTING.md) - Unit & integration tests
- [**Code Style**](./docs/CODE_STYLE.md) - Rust & TypeScript conventions
- [**Contributing**](./CONTRIBUTING.md) - How to contribute

### 📊 Operations
- [**Deployment Guide**](./docs/DEPLOYMENT.md) - Production deployment
- [**Mainnet Deployment**](./docs/MAINNET_DEPLOYMENT.md) - Mainnet specifics
- [**Testnet Deployment**](./docs/TESTNET_DEPLOYMENT.md) - Testnet setup
- [**Monitoring & Logging**](./docs/MONITORING.md) - Observability
- [**Runbook**](./docs/RUNBOOK.md) - Operational procedures

### 🔐 Security
- [**Security Audit**](./docs/SECURITY_AUDIT.md) - Security review
- [**Best Practices**](./PRODUCTION-BEST-PRACTICES.md) - Security guidelines
- [**Threat Model**](./docs/THREAT_MODEL.md) - Risk assessment
- [**Contract Audits**](./docs/contract-audits/) - Smart contract audits

### 📚 Advanced Topics
- [**Proof-of-Neural-Work**](./docs/PoNW.md) - Consensus mechanism
- [**Provider Economics**](./docs/PROVIDER_ECONOMICS.md) - Incentive model
- [**Rate Limiting**](./docs/RATE_LIMITING.md) - DoS protection
- [**Performance Tuning**](./docs/PERFORMANCE.md) - Optimization guide
- [**Scaling Strategies**](./docs/SCALING.md) - Horizontal scaling

### 📋 Project Status
- [**Implementation Plan**](./IMPLEMENTATION-PLAN.md) - Development roadmap
- [**Current Status**](./IMPLEMENTATION-STATUS.md) - What's working
- [**Wiring Diagrams**](./docs/wiring-diagrams.md) - System connections
- [**Roadmap**](./ROADMAP.md) - Future development
- [**LitePaper**](./Xergon%20LitePaper.md) - Project overview

### 🧪 Experimental
- [**Benchmarks**](./docs/BENCHMARKS.md) - Performance metrics
- [**Research Notes**](./docs/RESEARCH.md) - Technical research
- [**RFCs**](./docs/RFCs/) - Request for comments

---

## 🎯 Quick Links

### For Developers
- [Setup Development Environment](./LOCAL-SETUP-GUIDE.md)
- [API Reference](./docs/API_REFERENCE.md)
- [SDK Documentation](./xergon-sdk/README.md)
- [Testing Guide](./docs/TESTING.md)

### For Operators
- [Deployment Guide](./docs/DEPLOYMENT.md)
- [Monitoring Setup](./docs/MONITORING.md)
- [Runbook](./docs/RUNBOOK.md)
- [Troubleshooting](./docs/TROUBLESHOOTING.md)

### For Providers
- [Provider Onboarding](./docs/PROVIDER_ONBOARDING.md)
- [Ergo Node Setup](./docs/ERGO_NODE_SETUP.md)
- [Provider Economics](./docs/PROVIDER_ECONOMICS.md)

### For Users
- [User Guide](./docs/USER_GUIDE.md)
- [How It Works](./docs/HOW-IT-WORKS.md)
- [Marketplace UX](./docs/marketplace-ux-design.md)

---

## 📂 Documentation Structure

```
docs/
├── INDEX.md                    # This file (Table of Contents)
├── INTRODUCTION.md             # Project overview
├── QUICK-START.md              # 5-minute setup
├── LOCAL-SETUP-GUIDE.md        # Detailed setup guide
├── architecture.md             # System architecture
├── API_REFERENCE.md            # REST API docs
├── TESTING.md                  # Testing guide
├── CODE_STYLE.md               # Coding conventions
├── DEPLOYMENT.md               # Deployment guide
├── MONITORING.md               # Observability
├── RUNBOOK.md                  # Operations manual
├── SECURITY_AUDIT.md           # Security review
├── THREAT_MODEL.md             # Risk assessment
├── PoNW.md                     # Consensus mechanism
├── PROVIDER_ECONOMICS.md       # Incentive model
├── PERFORMANCE.md              # Optimization
├── SCALING.md                  # Scaling strategies
├── benchmarks.md               # Performance metrics
├── RESEARCH.md                 # Technical research
├── ERGO_NODE_SETUP.md          # Ergo integration
├── SMART_CONTRACTS.md          # ErgoScript
├── UTXO_GUIDE.md               # UTXO management
├── TRANSACTION_BUILDER.md      # Transaction building
├── PROVIDER_ONBOARDING.md      # Provider registration
├── RATE_LIMITING.md            # Rate limiting
├── TROUBLESHOOTING.md          # Common issues
├── wiring-diagrams.md          # System connections
├── USER_GUIDE.md               # User documentation
├── HOW-IT-WORKS.md             # How it works
├── marketplace-ux-design.md    # UI/UX design
├── contract-audits/            # Smart contract audits
│   ├── phase1-audit.md
│   ├── phase2-audit.md
│   └── ...
└── RFCs/                       # Request for comments
    ├── RFC-001.md
    └── ...
```

---

## 🚀 Getting Started

### Prerequisites
- **Rust** 1.70+
- **Node.js** 18+
- **Ergo Node** (testnet or mainnet)
- **Git**

### Quick Setup
```bash
# Clone the repository
git clone https://github.com/n1ur0/Xergon-Network.git
cd Xergon-Network

# Follow the setup guide
# See: LOCAL-SETUP-GUIDE.md
```

---

## 📚 Documentation Categories

### Core Documentation
These are the essential documents every developer should read:

1. **[Introduction](./INTRODUCTION.md)** - Understand what Xergon is
2. **[Quick Start](./QUICK-START.md)** - Get running quickly
3. **[Local Setup](./LOCAL-SETUP-GUIDE.md)** - Complete setup guide
4. **[Architecture](./docs/architecture.md)** - System design
5. **[API Reference](./docs/API_REFERENCE.md)** - API documentation

### Operational Documentation
For those deploying and maintaining Xergon:

1. **[Deployment Guide](./docs/DEPLOYMENT.md)** - Production deployment
2. **[Monitoring](./docs/MONITORING.md)** - Observability setup
3. **[Runbook](./docs/RUNBOOK.md)** - Operational procedures
4. **[Security Audit](./docs/SECURITY_AUDIT.md)** - Security review

### Specialized Documentation
For specific use cases:

1. **[Provider Onboarding](./docs/PROVIDER_ONBOARDING.md)** - For AI providers
2. **[SDK Documentation](./xergon-sdk/README.md)** - For developers
3. **[User Guide](./docs/USER_GUIDE.md)** - For end users
4. **[Testing Guide](./docs/TESTING.md)** - For QA engineers

---

## 🤝 Contributing

We welcome contributions! Please see:

- **[Contributing Guide](./CONTRIBUTING.md)** - How to contribute
- **[Code Style](./docs/CODE_STYLE.md)** - Coding conventions
- **[Testing Guide](./docs/TESTING.md)** - Writing tests
- **[RFC Process](./docs/RFCs/README.md)** - Proposing changes

---

## 📞 Support

### Documentation Issues
- **GitHub Issues**: [Report documentation problems](https://github.com/n1ur0/Xergon-Network/issues)
- **Discord**: [Join our community](https://discord.gg/xergon) (placeholder)
- **Email**: docs@xergon.network (placeholder)

### Technical Support
- **GitHub Discussions**: [Ask questions](https://github.com/n1ur0/Xergon-Network/discussions)
- **Community Forum**: [Discuss with others](https://forum.ergoplatform.org) (Ergo forum)

---

## 📝 Version Information

- **Documentation Version**: 1.0.0
- **Last Updated**: April 11, 2026
- **Repository**: [n1ur0/Xergon-Network](https://github.com/n1ur0/Xergon-Network)
- **Branch**: `feature/wiring-complete-2026-04-11`

---

## 📜 License

Documentation is licensed under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)

Code is licensed under [MIT](./LICENSE)

---

## 🎯 Navigation

| Previous | Next |
|----------|------|
| [Introduction](./INTRODUCTION.md) | [Quick Start](./QUICK-START.md) |
| [Local Setup](./LOCAL-SETUP-GUIDE.md) | [Architecture](./docs/architecture.md) |
| [API Reference](./docs/API_REFERENCE.md) | [Deployment](./docs/DEPLOYMENT.md) |

---

**Happy Reading!** 📖

If you find any issues or have suggestions for improvement, please open an issue or submit a PR.

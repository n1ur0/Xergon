# Running Xergon Network Locally: A Complete Setup Guide

**Last Updated:** April 11, 2026  
**Author:** Xergon Team  
**Difficulty:** Intermediate  
**Time Required:** 30-45 minutes

---

## 🎯 Overview

Xergon Network is a decentralized AI inference marketplace built on the Ergo blockchain. This guide will walk you through setting up the entire stack locally for development and testing.

### What You'll Build

- **xergon-relay**: Rust backend API (Axum + Tokio)
- **xergon-agent**: Rust sidecar for Proof-of-Neural-Work (PoNW)
- **xergon-marketplace**: Next.js 15 frontend UI
- **xergon-sdk**: TypeScript SDK for integration

### Prerequisites

- **Operating System:** Linux (Ubuntu/Debian recommended), macOS, or Windows WSL2
- **RAM:** 8GB minimum (16GB recommended)
- **Disk:** 5GB free space
- **Network:** Stable internet connection

---

## 📋 Prerequisites Installation

### 1. Rust Toolchain

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Source the environment
source $HOME/.cargo/env

# Verify installation
rustc --version  # Should be 1.70+
cargo --version
```

### 2. Node.js & npm

```bash
# Install Node.js 18+ (using nvm recommended)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
source $HOME/.bashrc
nvm install --lts
nvm use --lts

# Verify installation
node --version  # Should be 18+
npm --version   # Should be 9+
```

### 3. Ergo Node (Optional but Recommended)

For full functionality, you'll need a local Ergo node. You can use the official Ergo node or connect to a public testnet node.

**Option A: Use Public Testnet Node** (Easiest)
- Already configured in this guide: `http://192.168.1.75:9052`
- Replace with your own testnet node URL if needed

**Option B: Run Local Ergo Node** (Advanced)
```bash
# Follow Ergo node setup guide
# https://docs.ergoplatform.com/node/full-node-setup/
```

### 4. Git

```bash
# Verify installation
git --version

# Configure (if not already)
git config --global user.name "Your Name"
git config --global user.email "your.email@example.com"
```

---

## 🚀 Step-by-Step Setup

### Step 1: Clone the Repository

```bash
# Clone the Xergon Network repository
git clone https://github.com/n1ur0/Xergon-Network.git
cd Xergon-Network

# Verify structure
ls -la
# Should show: xergon-relay/, xergon-agent/, xergon-marketplace/, xergon-sdk/
```

### Step 2: Configure Environment Variables

#### Create Main `.env` File

```bash
# Create environment file in project root
cat > .env << EOF
# Xergon Network Environment Variables

# Ergo Node Configuration
ERGO_NODE_API_KEY="hello"
ERGO_NODE_REST_URL="http://192.168.1.75:9052"

# Provider Identity
XERGON_PROVIDER_ID="Xergon_Local_Test"
XERGON_PROVIDER_NAME="Local Test Node"
XERGON_REGION="local"

# Development Mode
XERGON_DEBUG=true
EOF
```

#### Configure xergon-relay

```bash
cd xergon-relay

# Copy example config
cp config.toml.example config.toml

# Edit config.toml
nano config.toml
```

**Update `config.toml`:**
```toml
[server]
host = "0.0.0.0"
port = 9090

[ergo_node]
rest_url = "http://192.168.1.75:9052"
api_key = "hello"

[[providers]]
id = "local-provider"
name = "Local Test Provider"
base_url = "http://localhost:8080"
api_key = "test-api-key"
```

#### Configure xergon-agent

```bash
cd ../xergon-agent

# Copy example config
cp config.toml.example config.toml

# Edit config.toml
nano config.toml
```

**Update `config.toml`:**
```toml
[ergo_node]
rest_url = "http://192.168.1.75:9052"

[xergon]
provider_id = "Xergon_Local_Test"
provider_name = "Local Test Node"
region = "local"
ergo_address = "your-ergo-address-here"  # Optional for local testing
```

#### Configure xergon-marketplace

```bash
cd ../xergon-marketplace

# Create environment file
cat > .env.local << EOF
NEXT_PUBLIC_ERGO_NODE_URL=http://192.168.1.75:9052
NEXT_PUBLIC_RELAY_URL=http://localhost:9090
NEXT_PUBLIC_PROVIDER_ID=Xergon_Local_Test
EOF
```

### Step 3: Install Dependencies

#### Rust Components

```bash
# Install xergon-relay dependencies
cd xergon-relay
cargo build --release

# Install xergon-agent dependencies
cd ../xergon-agent
cargo build --release
```

#### Next.js Frontend

```bash
# Install npm dependencies
cd ../xergon-marketplace
npm install

# Verify installation
npm run build
```

#### TypeScript SDK

```bash
# Install SDK dependencies
cd ../xergon-sdk
npm install

# Build SDK
npm run build
```

### Step 4: Start Services

#### Start Ergo Node (if running locally)

If you're using a public testnet node, skip this step. If running your own:

```bash
# Start Ergo node (adjust path as needed)
java -jar ergo.jar --mainnet false
```

#### Start xergon-relay

```bash
cd xergon-relay

# Run in development mode
cargo run --release

# Or run as background service
nohup cargo run --release > relay.log 2>&1 &

# Verify it's running
curl http://localhost:9090/info
```

#### Start xergon-agent

```bash
cd ../xergon-agent

# Run in development mode
cargo run --release

# Or run as background service
nohup cargo run --release > agent.log 2>&1 &

# Verify it's running
ps aux | grep xergon-agent
```

#### Start xergon-marketplace

```bash
cd ../xergon-marketplace

# Start development server
npm run dev

# Or build for production
npm run build
npm run start

# Access at: http://localhost:3000
```

---

## ✅ Verification Checklist

### 1. Check Services Are Running

```bash
# Check relay (port 9090)
curl http://localhost:9090/info

# Expected response:
# {
#   "name": "xergon-relay",
#   "version": "1.0.0",
#   "status": "running"
# }

# Check agent
ps aux | grep xergon-agent

# Check marketplace
curl http://localhost:3000
```

### 2. Check Ergo Node Connectivity

```bash
# Query Ergo node
curl http://192.168.1.75:9052/info

# Expected: Height > 100000, network: testnet
```

### 3. Verify Builds

```bash
# Rust components
cd xergon-relay && cargo build --release
cd ../xergon-agent && cargo build --release

# Frontend
cd ../xergon-marketplace && npm run build

# SDK
cd ../xergon-sdk && npm run build
```

### 4. Run Tests

```bash
# Rust tests
cd xergon-relay && cargo test
cd ../xergon-agent && cargo test

# Frontend tests
cd ../xergon-marketplace && npm test
```

---

## 🐛 Common Issues & Troubleshooting

### Issue 1: "Cargo not found"

**Symptom:** `command not found: cargo`

**Solution:**
```bash
source $HOME/.cargo/env
# Or add to ~/.bashrc
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### Issue 2: "Port already in use"

**Symptom:** `Error: Address already in use`

**Solution:**
```bash
# Find process using port 9090
lsof -i :9090

# Kill the process
kill -9 <PID>

# Or change port in config.toml
```

### Issue 3: "Module not found" (Next.js)

**Symptom:** `Module not found: Can't resolve '...'`

**Solution:**
```bash
# Clear cache and reinstall
cd xergon-marketplace
rm -rf node_modules .next
npm install
```

### Issue 4: Ergo Node Connection Failed

**Symptom:** `Connection refused to Ergo node`

**Solution:**
```bash
# Check if Ergo node is running
curl http://192.168.1.75:9052/info

# If using public node, verify URL
# If using local node, start it first
```

### Issue 5: Permission Denied

**Symptom:** `Permission denied: config.toml`

**Solution:**
```bash
# Fix permissions
chmod 644 config.toml
chmod 755 *.sh

# Or run with sudo (not recommended)
sudo cargo run --release
```

---

## 📊 Performance Tuning

### Increase Rust Build Speed

```bash
# Use parallel builds
export CARGO_BUILD_JOBS=4

# Use release profile
cargo build --release
```

### Optimize Node.js

```bash
# Increase Node.js memory
export NODE_OPTIONS="--max-old-space-size=4096"

# Use npm cache
npm cache clean --force
```

### Database Optimization

```toml
# In config.toml for xergon-relay
[database]
pool_size = 10
max_connections = 50
```

---

## 🔐 Security Best Practices

### 1. Never Commit Secrets

```bash
# Add to .gitignore
.env
config.toml
*.key
*.pem
```

### 2. Use Environment Variables

```bash
# Never hardcode API keys
export ERGO_NODE_API_KEY="your-key-here"
```

### 3. Production vs Development

```bash
# Use different configs
cp config.toml.example config.production.toml
cp config.toml.example config.development.toml
```

---

## 📚 Next Steps

### 1. Explore the Codebase

```bash
# Read documentation
cat README.md
cat IMPLEMENTATION-STATUS.md
cat PRODUCTION-BEST-PRACTICES.md
```

### 2. Run Integration Tests

```bash
# Test full workflow
cd tests
./run-integration-tests.sh
```

### 3. Monitor Logs

```bash
# View relay logs
tail -f xergon-relay/relay.log

# View agent logs
tail -f xergon-agent/agent.log
```

### 4. Join the Community

- **GitHub:** https://github.com/n1ur0/Xergon-Network
- **Discord:** [Community Link]
- **Documentation:** https://xergon.network/docs

---

## 🆘 Getting Help

### Documentation

- **Main Docs:** `docs/` directory
- **API Reference:** `xergon-relay/API.md`
- **SDK Docs:** `xergon-sdk/README.md`

### Support Channels

- **GitHub Issues:** https://github.com/n1ur0/Xergon-Network/issues
- **Community Forum:** [Link]
- **Email:** support@xergon.network

### Contributing

Found a bug or want to improve? Check out our [Contributing Guide](CONTRIBUTING.md).

---

## 📝 Changelog

- **2026-04-11:** Initial guide published
- **2026-04-11:** Updated Ergo node URL to testnet
- **2026-04-11:** Added troubleshooting section

---

## ✅ Quick Start (TL;DR)

For those who just want to get started fast:

```bash
# 1. Clone
git clone https://github.com/n1ur0/Xergon-Network.git
cd Xergon-Network

# 2. Setup
cp .env.example .env
cp xergon-relay/config.toml.example xergon-relay/config.toml
cp xergon-agent/config.toml.example xergon-agent/config.toml

# 3. Install
cd xergon-relay && cargo build --release
cd ../xergon-agent && cargo build --release
cd ../xergon-marketplace && npm install

# 4. Run
cd xergon-relay && cargo run --release &
cd ../xergon-agent && cargo run --release &
cd ../xergon-marketplace && npm run dev

# 5. Access
open http://localhost:3000
```

---

**Happy Coding!** 🚀

If you encounter issues, check the [Troubleshooting](#-common-issues--troubleshooting) section or open a GitHub issue.

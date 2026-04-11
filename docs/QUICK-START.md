# 🚀 Quick Start - Xergon Network

> **Get Xergon running in 5 minutes**

---

## 📋 Prerequisites

Before you begin, ensure you have:

- ✅ **Rust** 1.70+ installed
- ✅ **Node.js** 18+ installed
- ✅ **Git** installed
- ✅ **Docker** (optional, for containerized deployment)
- ✅ **8GB RAM** minimum (16GB recommended)
- ✅ **5GB** free disk space

### Install Prerequisites (if needed)

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install Node.js (using nvm)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
source $HOME/.bashrc
nvm install --lts
nvm use --lts

# Verify installations
rustc --version  # Should be 1.70+
node --version   # Should be 18+
npm --version    # Should be 9+
git --version
```

---

## 🚀 Quick Setup (5 Steps)

### Step 1: Clone Repository

```bash
git clone https://github.com/n1ur0/Xergon-Network.git
cd Xergon-Network
```

### Step 2: Configure Environment

```bash
# Create environment file
cat > .env << EOF
# Xergon Network Environment
ERGO_NODE_API_KEY="hello"
ERGO_NODE_REST_URL="http://192.168.1.75:9052"
XERGON_PROVIDER_ID="Xergon_Quick_Start"
XERGON_PROVIDER_NAME="Quick Start Provider"
XERGON_REGION="local"
XERGON_DEBUG=true
EOF
```

### Step 3: Configure Components

```bash
# Configure xergon-relay
cd xergon-relay
cp config.toml.example config.toml
# Edit if needed: nano config.toml

# Configure xergon-agent
cd ../xergon-agent
cp config.toml.example config.toml
# Edit if needed: nano config.toml

# Configure xergon-marketplace
cd ../xergon-marketplace
cat > .env.local << EOF
NEXT_PUBLIC_ERGO_NODE_URL=http://192.168.1.75:9052
NEXT_PUBLIC_RELAY_URL=http://localhost:9090
NEXT_PUBLIC_PROVIDER_ID=Xergon_Quick_Start
EOF
```

### Step 4: Install Dependencies

```bash
# Build Rust components
cd ../xergon-relay
cargo build --release

cd ../xergon-agent
cargo build --release

# Install Node.js dependencies
cd ../xergon-marketplace
npm install

# Build SDK
cd ../xergon-sdk
npm install
npm run build
```

### Step 5: Start Services

```bash
# Terminal 1: Start xergon-relay
cd xergon-relay
cargo run --release

# Terminal 2: Start xergon-agent
cd ../xergon-agent
cargo run --release

# Terminal 3: Start xergon-marketplace
cd ../xergon-marketplace
npm run dev
```

---

## ✅ Verification

### Check Services Are Running

```bash
# Check relay (port 9090)
curl http://localhost:9090/info

# Expected output:
# {
#   "name": "xergon-relay",
#   "version": "1.0.0",
#   "status": "running"
# }

# Check agent
ps aux | grep xergon-agent

# Check marketplace (open browser)
open http://localhost:3000
```

### Test End-to-End

```bash
# Test Ergo node connectivity
curl http://192.168.1.75:9052/info

# Expected: Height > 100000, network: testnet

# Test provider registration (if agent is running)
curl -X POST http://localhost:9090/register \
  -H "Content-Type: application/json" \
  -d '{"provider_id": "test", "endpoint": "http://localhost:8080"}'
```

---

## 🎯 Next Steps

### For Developers

1. **Read the Introduction**
   ```bash
   cat docs/INTRODUCTION.md
   ```

2. **Explore the API**
   ```bash
   cat docs/API_REFERENCE.md
   ```

3. **Install the SDK**
   ```bash
   cd xergon-sdk
   npm link
   ```

4. **Run Tests**
   ```bash
   cd xergon-marketplace
   npm test
   ```

### For Providers

1. **Set up Ergo Node**
   ```bash
   cat docs/ERGO_NODE_SETUP.md
   ```

2. **Register as Provider**
   ```bash
   cat docs/PROVIDER_ONBOARDING.md
   ```

3. **Start Serving Requests**
   ```bash
   cat xergon-relay/README.md
   ```

### For Users

1. **Access the Marketplace**
   ```bash
   open http://localhost:3000
   ```

2. **Browse Providers**
   - Navigate to the Providers tab
   - Select a model
   - Send a test request

---

## 🐛 Troubleshooting

### Issue: "Cargo not found"

```bash
source $HOME/.cargo/env
# Or add to ~/.bashrc
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### Issue: "Port already in use"

```bash
# Find process using port 9090
lsof -i :9090

# Kill the process
kill -9 <PID>

# Or change port in config.toml
```

### Issue: "Module not found" (Next.js)

```bash
cd xergon-marketplace
rm -rf node_modules .next
npm install
```

### Issue: Ergo Node Connection Failed

```bash
# Check if Ergo node is running
curl http://192.168.1.75:9052/info

# If using public node, verify URL
# If using local node, start it first
```

---

## 📚 Related Documentation

- [**Introduction**](./INTRODUCTION.md) - What is Xergon?
- [**Local Setup Guide**](./LOCAL-SETUP-GUIDE.md) - Detailed setup
- [**Architecture**](./docs/architecture.md) - System design
- [**API Reference**](./docs/API_REFERENCE.md) - API documentation
- [**Provider Guide**](./docs/PROVIDER_ONBOARDING.md) - Become a provider

---

## 🎉 You're Ready!

Congratulations! You now have Xergon Network running locally.

### What You Have

- ✅ **xergon-relay** running on port 9090
- ✅ **xergon-agent** running in background
- ✅ **xergon-marketplace** running on port 3000
- ✅ **Connected to Ergo testnet**
- ✅ **Ready to serve requests**

### Next Actions

1. **Explore the UI**: http://localhost:3000
2. **Read the docs**: `cat docs/INDEX.md`
3. **Join the community**: https://github.com/n1ur0/Xergon-Network
4. **Start building**: See SDK documentation

---

**Need Help?**

- 📖 [Full Documentation](./INDEX.md)
- 🐛 [Troubleshooting](./TROUBLESHOOTING.md)
- 💬 [GitHub Discussions](https://github.com/n1ur0/Xergon-Network/discussions)
- 📧 Email: support@xergon.network

**Happy Coding!** 🚀

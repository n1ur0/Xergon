# Ergo Node Setup Guide

## Prerequisites
- Ubuntu 20.04+ or Debian 10+
- 8GB+ RAM
- 100GB+ SSD storage
- Stable internet connection

## Installation Steps

### 1. Install Dependencies
```bash
sudo apt update
sudo apt install -y openjdk-11-jre curl
```

### 2. Download Ergo Node
```bash
curl -L https://github.com/ergoplatform/ergo/releases/download/v6.0.3/ergo.jar -o ergo.jar
```

### 3. Configuration
Create `ergo.conf`:
```hocon
ergo {
  node {
    mining = true
    stateType = "Digest32"
  }
  network {
    bind = "0.0.0.0:9030"
  }
}
```

### 4. Run Node
```bash
java -jar ergo.jar --config ergo.conf
```

## Testnet Configuration
Use `--network testnet` flag for testnet operation.

## Monitoring
- Health endpoint: `http://localhost:9052/info`
- Explorer: `http://localhost:9052`

# Xergon Installer

One-liner bootstrap installer for the Xergon Network agent.

## Quick Install

```bash
curl -sSL https://degens.world/xergon | sh
```

## Commands

```bash
# Install (default)
curl -sSL https://degens.world/xergon | sh

# Update to latest version
curl -sSL https://degens.world/xergon | sh -s -- update

# Uninstall
curl -sSL https://degens.world/xergon | sh -s -- uninstall

# Help
curl -sSL https://degens.world/xergon | sh -s -- help
```

## What Gets Installed

```
~/.xergon/
├── bin/
│   └── xergon-agent       # the binary
├── config.toml             # agent configuration (generated)
├── wallet.json             # encrypted wallet (placeholder until setup)
└── data/                   # settlement ledger, peer data
```

## Install a Specific Version

```bash
XERGON_VERSION=v0.1.0 curl -sSL https://degens.world/xergon | sh
```

## After Install

```bash
# Activate PATH in current shell
source ~/.bashrc   # or ~/.zshrc

# Run interactive setup
xergon-agent setup

# Check it works
xergon-agent --version
```

## How It Works

1. **Detects** your OS (linux/darwin) and architecture (amd64/arm64)
2. **Downloads** the pre-built binary from GitHub Releases:
   `https://github.com/n1ur0/Xergon-Network/releases/latest/download/xergon-agent-{os}-{arch}.tar.gz`
3. **Extracts** and installs to `~/.xergon/bin/`
4. **Generates** default `config.toml` and `wallet.json` placeholder
5. **Adds** `~/.xergon/bin` to your PATH in `.bashrc`, `.zshrc`, or `.profile`
6. **Prompts** you to run `xergon-agent setup` for first-run configuration

## Requirements

- macOS or Linux
- amd64 (x86_64) or arm64 (aarch64) architecture
- Standard Unix tools: `curl`, `tar`, `sh`

## Features

- **Idempotent** -- safe to run multiple times without side effects
- **Colored output** -- ANSI colors with auto-detection (disabled when piped)
- **Graceful update** -- backs up existing binary before replacing
- **Clean uninstall** -- optional full data removal with confirmation
- **Minimal dependencies** -- only curl, tar, and sh

## Local Development

If no GitHub release exists yet (expected before CI/CD is set up), you can build locally:

```bash
cd xergon-agent
cargo build --release
cp target/release/xergon-agent ~/.xergon/bin/
```

## Hosting the Installer

To serve this script at `https://degens.world/xergon`:

1. Upload `install.sh` to a web server or GitHub raw URL
2. Configure the server to serve it with `Content-Type: text/plain`
3. The URL `https://degens.world/xergon` should return the script contents

Example using GitHub raw:
```bash
curl -sSL https://raw.githubusercontent.com/n1ur0/Xergon-Network/main/xergon-installer/install.sh | sh
```

Or with a redirect on degens.world:
```
https://degens.world/xergon → 302 → https://raw.githubusercontent.com/n1ur0/Xergon-Network/main/xergon-installer/install.sh
```

## Binary Release Naming Convention

```
xergon-agent-linux-amd64.tar.gz
xergon-agent-linux-arm64.tar.gz
xergon-agent-darwin-amd64.tar.gz
xergon-agent-darwin-arm64.tar.gz
```

Each tarball contains a single `xergon-agent` binary.

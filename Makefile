.PHONY: all agent relay marketplace check test clean release compile-contracts validate-contracts

all: agent relay

agent:
	cd xergon-agent && cargo build --release

relay:
	cd xergon-relay && cargo build --release

marketplace:
	cd xergon-marketplace && npm run build

check:
	cd xergon-agent && cargo check
	cd xergon-relay && cargo check
	cd xergon-marketplace && npx tsc --noEmit

test:
	cd xergon-agent && cargo test
	cd xergon-relay && cargo test

clean:
	cd xergon-agent && cargo clean
	cd xergon-relay && cargo clean
	rm -rf xergon-marketplace/.next

# Compile ErgoScript contracts to ErgoTree hex via Ergo node
# Requires a running Ergo node (default http://127.0.0.1:9053)
# Set ERGO_NODE_URL to use a different node
compile-contracts:
	./scripts/compile_contracts.sh

# Validate existing compiled hex files (no compilation, no node required)
validate-contracts:
	./scripts/compile_contracts.sh --validate-only

# Cross-compilation targets
release: release-linux-amd64 release-linux-arm64 release-darwin-arm64 release-darwin-amd64

release-linux-amd64:
	cd xergon-agent && cross build --release --target x86_64-unknown-linux-musl
	cd xergon-relay && cross build --release --target x86_64-unknown-linux-musl
	mkdir -p dist
	cp xergon-agent/target/x86_64-unknown-linux-musl/release/xergon-agent dist/xergon-agent-linux-amd64
	cp xergon-relay/target/x86_64-unknown-linux-musl/release/xergon-relay dist/xergon-relay-linux-amd64

release-linux-arm64:
	cd xergon-agent && cross build --release --target aarch64-unknown-linux-musl
	cd xergon-relay && cross build --release --target aarch64-unknown-linux-musl
	mkdir -p dist
	cp xergon-agent/target/aarch64-unknown-linux-musl/release/xergon-agent dist/xergon-agent-linux-arm64
	cp xergon-relay/target/aarch64-unknown-linux-musl/release/xergon-relay dist/xergon-relay-linux-arm64

release-darwin-arm64:
	cd xergon-agent && cargo build --release --target aarch64-apple-darwin
	cd xergon-relay && cargo build --release --target aarch64-apple-darwin
	mkdir -p dist
	cp xergon-agent/target/aarch64-apple-darwin/release/xergon-agent dist/xergon-agent-darwin-arm64
	cp xergon-relay/target/aarch64-apple-darwin/release/xergon-relay dist/xergon-relay-darwin-arm64

release-darwin-amd64:
	cd xergon-agent && cargo build --release --target x86_64-apple-darwin
	cd xergon-relay && cargo build --release --target x86_64-apple-darwin
	mkdir -p dist
	cp xergon-agent/target/x86_64-apple-darwin/release/xergon-agent dist/xergon-agent-darwin-amd64
	cp xergon-relay/target/x86_64-apple-darwin/release/xergon-relay dist/xergon-relay-darwin-amd64

# Install script (curl | sh)
install:
	@echo "Use: curl -sSL https://xergon.network/install | sh"

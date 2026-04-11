.PHONY: help build run test codegen lint

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

build: ## Build the relay (cargo build)
	cargo build

run: ## Run the relay in dev mode
	cargo run

test: ## Run tests
	cargo test

lint: ## Run clippy
	cargo clippy -- -D warnings

codegen: ## Regenerate TypeScript & Python client code from OpenAPI spec
	@./scripts/codegen.sh

codegen-ts: ## Regenerate TypeScript client only
	@./scripts/codegen.sh typescript

codegen-py: ## Regenerate Python client only
	@./scripts/codegen.sh python

USERNAME ?= killzoner

.PHONY: fmt
fmt: ## format files
	cargo fmt
	taplo fmt

.PHONY: lint
lint: ## lint files
	cargo clippy --all --all-targets --all-features -- -D warnings
	cargo machete

.PHONY: build
build: ## build project
	cargo build --all --all-targets --all-features

.PHONY: deny
deny: ## run cargo deny checks
	cargo deny -L error --workspace check bans advisories sources

.PHONY: security
security: ## run all security scans (fs, repo, secret)
	trivy fs --config trivy.yaml .
	trivy repo --config trivy.yaml .
	trivy fs --config trivy.yaml --scanners secret .

.PHONY: ci
ci: fmt lint deny security ## run all CI checks

.PHONY: run
run: ## run the program (uses USERNAME env var, default: killzoner)
	cargo run -- --username $(USERNAME)

.PHONY: generate
generate: ## generate README.md
	cargo run > README.md

.DEFAULT_GOAL := help
.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-30s\033[0m %s\n", $$1, $$2}'

## OrbitChain Makefile
##   make build        - Compile contracts
##   make test         - Run all tests
##   make audit        - Run cargo audit
##   make deny         - Check licenses
##   make fmt          - Format code
##   make clippy       - Lint code

.PHONY: build build-wasm build-tools test fmt lint clean optimize optimize-verify help e2e \
        setup deploy-testnet deploy-sandbox sandbox-start audit deny fuzz

# Default target
build: build-wasm build-tools
	@echo "✅ Build complete"

# Build WASM contract
build-wasm:
	@echo "🔨 Building Soroban contract..."
	cargo build -p orbitchain-core -p orbitchain-campaign -p orbitchain-token-bridge -p orbitchain-batch-donor -p orbitchain-common --target wasm32v1-none --release
	@echo "✅ WASM contracts built successfully"

# Build CLI tools
build-tools:
	@echo "🔨 Building CLI tools..."
	cargo build -p orbitchain-tools
	@echo "✅ CLI tools built successfully"

# Run tests
test:
	@echo "🧪 Running tests..."
	cargo test --workspace
	@echo "✅ Tests passed"

# Format code
fmt:
	@echo "🎨 Formatting code..."
	cargo fmt --all
	@echo "✅ Code formatted"

# Run linter
lint:
	@echo "🔍 Running linter..."
	cargo clippy --workspace -- -D warnings
	@echo "✅ Linting passed"

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean
	@echo "✅ Clean complete"

# Install soroban-cli and required Rust targets
setup:
	@echo "🔧 Installing soroban-cli..."
	cargo install --locked stellar-cli --features opt
	@echo "🔧 Adding wasm32v1-none target..."
	rustup target add wasm32v1-none
	@echo "✅ Setup complete. Run 'make build' to compile contracts."

# Start local sandbox (requires Docker)
sandbox-start:
	@echo "🐳 Starting local Stellar sandbox..."
	docker run --rm -d \
		--name stellar-sandbox \
		-p 8000:8000 \
		stellar/quickstart:testing \
		--standalone \
		--enable-soroban-rpc
	@echo "✅ Sandbox running at http://localhost:8000"
	@echo "   RPC endpoint: http://localhost:8000/soroban/rpc"

# Deploy to local sandbox
deploy-sandbox: optimize
	@echo "🚀 Deploying to local sandbox..."
	bash scripts/deploy.sh sandbox

# Deploy to Stellar testnet
deploy-testnet: optimize
	@echo "🚀 Deploying to testnet..."
	bash scripts/deploy.sh testnet

# End-to-end sandbox lifecycle test: boots stellar/quickstart in Docker,
# deploys orbitchain-campaign, and walks init → donate → unlock → release
# with a real token transfer (issue #116). `bash e2e/run_e2e.sh futurenet`
# runs the same lifecycle against Futurenet without Docker.
e2e:
	@echo "🧪 Running end-to-end sandbox lifecycle test..."
	bash e2e/run_e2e.sh local


# Run cargo-audit for vulnerability scanning
audit:
	@echo "🔒 Running security audit..."
	cargo audit
	@echo "✅ Security audit passed"

# Run cargo-deny for license compliance
deny:
	@echo "📋 Checking license compliance..."
	cargo deny check
	@echo "✅ License check passed"

# Run cargo-fuzz smoke tests (60s each target)
fuzz:
	@echo "🔬 Running fuzz smoke tests..."
	@cd fuzz && cargo fuzz run fuzz_donate -- -max_total_time=60 || true
	@cd fuzz && cargo fuzz run fuzz_initialize -- -max_total_time=60 || true
	@cd fuzz && cargo fuzz run fuzz_release_milestone -- -max_total_time=60 || true
	@cd fuzz && cargo fuzz run fuzz_claim_refund -- -max_total_time=60 || true
	@echo "✅ Fuzz smoke tests complete"

# Optimize WASM binaries using wasm-opt (-Oz).
#
# Issue #117 – writes each artifact to <name>.optimized.wasm instead of
# overwriting cargo's output in place: cargo does not fingerprint its own
# artifacts, so an in-place overwrite silently re-optimizes already-optimized
# files on the next run and reports misleading deltas.
optimize: build-wasm
	@echo "🔧 Optimizing WASM binaries with wasm-opt (-Oz)..."
	@for wasm in target/wasm32v1-none/release/*.wasm; do \
		case "$$wasm" in *.optimized.wasm) continue;; esac; \
		out="$${wasm%.wasm}.optimized.wasm"; \
		before=$$(wc -c < "$$wasm" | tr -d ' '); \
		wasm-opt -Oz "$$wasm" -o "$$out"; \
		after=$$(wc -c < "$$out" | tr -d ' '); \
		pct=$$(awk "BEGIN{printf \"%.1f\", 100*($$before-$$after)/$$before}"); \
		echo "  $$(basename $$wasm): $${before}B -> $${after}B (-$${pct}%)"; \
	done
	@echo "✅ Optimization complete (artifacts: *.optimized.wasm)"

# Issue #117 – size + functional regression gate for the optimized binaries:
# budget check via scripts/wasm_size_check.sh, then the campaign lifecycle
# executed inside the Soroban host VM against the wasm-opt output.
optimize-verify: optimize
	@bash scripts/wasm_size_check.sh
	@echo "🧪 Executing optimized campaign wasm in the Soroban host VM..."
	OPTIMIZED_WASM=$(CURDIR)/target/wasm32v1-none/release/orbitchain_campaign.optimized.wasm \
		cargo test -p orbitchain-campaign --test optimized_wasm_exec
	@echo "✅ Optimized WASM verified"

# Show help
help:
	@echo "Available commands:"
	@echo "  make setup          - Install soroban-cli and required Rust targets"
	@echo "  make build          - Build WASM contract and CLI tools"
	@echo "  make build-wasm     - Build Soroban WASM contract only"
	@echo "  make build-tools    - Build CLI tools only"
	@echo "  make test           - Run all tests"\n	@echo "  make optimize       - Build + shrink WASM with wasm-opt -Oz (issue #117)"\n	@echo "  make optimize-verify - Optimize, enforce size budgets, exec-test the output"
	@echo "  make fmt            - Format code"
	@echo "  make lint           - Run linter"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make sandbox-start  - Start local Stellar sandbox (requires Docker)"
	@echo "  make deploy-sandbox - Deploy contract to local sandbox"
	@echo "  make deploy-testnet - Deploy contract to Stellar testnet"
	@echo "  make e2e            - End-to-end sandbox lifecycle test (Docker)"
	@echo "  make optimize       - Optimize WASM with wasm-opt -Oz"
	@echo "  make fuzz           - Run fuzz smoke tests (60s per target)"
	@echo "  make help           - Show this help message"

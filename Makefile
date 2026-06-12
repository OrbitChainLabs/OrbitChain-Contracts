## OrbitChain Makefile
##   make build        - Compile contracts
##   make test         - Run all tests
##   make audit        - Run cargo audit
##   make deny         - Check licenses
##   make fmt          - Format code
##   make clippy       - Lint code

.PHONY: build build-wasm build-tools test fmt lint clean optimize help \
        setup deploy-testnet deploy-sandbox sandbox-start audit deny

# Default target
build: build-wasm build-tools
	@echo "✅ Build complete"

# Build WASM contract
build-wasm:
	@echo "🔨 Building Soroban contract..."
	cargo build -p orbitchain-core --target wasm32-unknown-unknown --release
	@echo "✅ WASM contract built successfully"

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
	@echo "🔧 Adding wasm32-unknown-unknown target..."
	rustup target add wasm32-unknown-unknown
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
deploy-sandbox: build-wasm
	@echo "🚀 Deploying to local sandbox..."
	bash scripts/deploy.sh sandbox

# Deploy to Stellar testnet
deploy-testnet: build-wasm
	@echo "🚀 Deploying to testnet..."
	bash scripts/deploy.sh testnet


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

# Optimize WASM binaries using wasm-opt (-Oz)
optimize: build
	@echo "🔧 Optimizing WASM binaries with wasm-opt..."
	@for wasm in target/wasm32v1-none/release/*.wasm; do 		before=$$(wc -c < "$$wasm"); 		wasm-opt -Oz "$$wasm" -o "$$wasm.opt" && mv "$$wasm.opt" "$$wasm"; 		after=$$(wc -c < "$$wasm"); 		echo "  $$(basename $$wasm): $${before}B -> $${after}B"; 	done
	@echo "✅ Optimization complete"

# Show help
help:
	@echo "Available commands:"
	@echo "  make setup          - Install soroban-cli and required Rust targets"
	@echo "  make build          - Build WASM contract and CLI tools"
	@echo "  make build-wasm     - Build Soroban WASM contract only"
	@echo "  make build-tools    - Build CLI tools only"
	@echo "  make test           - Run all tests"
	@echo "  make fmt            - Format code"
	@echo "  make lint           - Run linter"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make sandbox-start  - Start local Stellar sandbox (requires Docker)"
	@echo "  make deploy-sandbox - Deploy contract to local sandbox"
	@echo "  make deploy-testnet - Deploy contract to Stellar testnet"
	@echo "  make optimize       - Optimize WASM with wasm-opt -Oz"
	@echo "  make help           - Show this help message"

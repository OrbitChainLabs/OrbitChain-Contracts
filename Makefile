.PHONY: build build-wasm build-tools test fmt lint clean help \
        setup deploy-testnet deploy-sandbox sandbox-start

# Default target
build: build-wasm build-tools
	@echo "✅ Build complete"

# Build WASM contract
build-wasm:
	@echo "🔨 Building Soroban contract..."
	cargo build -p stellaraid-core --target wasm32-unknown-unknown --release
	@echo "✅ WASM contract built successfully"

# Build CLI tools
build-tools:
	@echo "🔨 Building CLI tools..."
	cargo build -p stellaraid-tools
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
	@echo "  make help           - Show this help message"

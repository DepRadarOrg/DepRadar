# DepRadar developer tasks
# Run `just --list` to see all available recipes.

# Default recipe: list available recipes
default:
    @just --list

# Run formatting checks, clippy, and tests
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --locked -- -D warnings
    cargo test --workspace --locked

# Build the router crate for the wasm32v1-none target
wasm:
    cargo build -p deprader-router --target wasm32v1-none --release --locked

# Start the frontend dev server
fe:
    cd frontend && npm run dev

# Run frontend lint, typecheck, tests, and build
fe-check:
    cd frontend && npm run lint && npm run typecheck && npm test && npm run build

# Run the scan binary against a given name
scan name:
    cargo run --bin scan -- {{name}}
# Rust FFmpeg justfile
# Use `just` to run common development tasks

# Default recipe shows available commands
default:
    @just --list

# Build all crates
build:
    cargo build --all

# Build with release optimizations
build-release:
    cargo build --release --all

# Run all tests
test:
    cargo test --all

# Run tests with output
test-verbose:
    cargo test --all -- --nocapture

# Run a specific test
test-one TEST:
    cargo test {{TEST}} -- --nocapture

# Run benchmarks
bench:
    cargo bench --all

# Run a specific benchmark
bench-one BENCH:
    cargo bench --bench {{BENCH}}

# Run fuzzer for 60 seconds (smoke test)
fuzz-smoke TARGET:
    cd fuzz && cargo +nightly fuzz run {{TARGET}} -- -max_total_time=60

# Run fuzzer indefinitely
fuzz TARGET:
    cd fuzz && cargo +nightly fuzz run {{TARGET}}

# Check code with clippy
lint:
    cargo clippy --all --all-targets -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run cargo check on all crates
check:
    cargo check --all --all-targets

# Clean build artifacts
clean:
    cargo clean

# Run the rav CLI
rav *ARGS:
    cargo run --release --bin rav -- {{ARGS}}

# Run rav in debug mode
rav-debug *ARGS:
    cargo run --bin rav -- {{ARGS}}

# Build documentation
doc:
    cargo doc --all --no-deps

# Open documentation in browser
doc-open:
    cargo doc --all --no-deps --open

# Run FATE compatibility tests
fate:
    cd tools/fate-compat && cargo run

# Run all quality gates (CI simulation)
ci: fmt-check lint test build-release

# Profile a benchmark with perf (Linux only)
profile-bench BENCH:
    cargo build --release --bench {{BENCH}}
    perf record -F 997 -g target/release/deps/{{BENCH}}*
    perf report

# Run under valgrind (Linux only)
valgrind-test:
    cargo build --tests
    valgrind --leak-check=full --show-leak-kinds=all target/debug/deps/av_*

# Count lines of code
loc:
    @find crates apps tools -name '*.rs' | xargs wc -l | tail -1

# Show dependency tree
deps:
    cargo tree --all

# Audit dependencies for security issues
audit:
    cargo audit

# Update dependencies
update:
    cargo update

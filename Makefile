.PHONY: all fmt fmt-check lint test check clean

all: fmt-check lint test

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
	cargo test --workspace --all-features

check:
	cargo check --workspace --all-targets

clean:
	cargo clean

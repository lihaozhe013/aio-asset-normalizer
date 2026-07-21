.PHONY: run build clean

default: run

# One-shot build
build:
	cargo build --release

# One-shot run
run:
	cargo run --release

# Clean build artifacts
clean:
	cargo clean
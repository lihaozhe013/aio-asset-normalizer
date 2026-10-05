.PHONY: run build clean dev build-cli

default: dev

dev:
	cargo run --bin aio-asset-normalizer

build:
	cargo build --release --all

run:
	cargo run --release aio-asset-normalizer

clean:
	cargo clean

build-cli:
	cargo build --locked --release --no-default-features --features cli --bin aio-asset-normalizer-cli && uv run ./packaging/build-cli.py --skip-build

build-mac:
	cargo build --release && uv run ./packaging/build-mac-app.py

build-win:
	cargo build --release && uv run ./packaging/build-windows.py

build-appimage:
	cargo build --release && uv run ./packaging/build-appimage.py

.PHONY: run build clean dev build-cli

default: dev

dev:
	cargo run

build:
	cargo build --release

run:
	cargo run --release

clean:
	cargo clean

build-cli:
	cargo build --release --bin aio-asset-normalizer-cli && uv run ./packaging/build-cli.py --skip-build

build-mac:
	cargo build --release && uv run ./packaging/build-mac-app.py

build-win:
	cargo build --release && uv run ./packaging/build-windows.py

build-appimage:
	cargo build --release && uv run ./packaging/build-appimage.py

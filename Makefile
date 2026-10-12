.PHONY: default run build clean dev build-mac build-win build-appimage

default: dev

dev:
	cargo build --locked --features mcp --bin aio-asset-normalizer --bin aio-asset-normalizer-mcp
	cargo run --locked --features mcp --bin aio-asset-normalizer

build:
	cargo build --locked --release --features mcp --bin aio-asset-normalizer --bin aio-asset-normalizer-mcp

run: build
	cargo run --locked --release --features mcp --bin aio-asset-normalizer

clean:
	cargo clean

build-mac: build
	uv run ./packaging/build-mac-app.py

build-win: build
	uv run ./packaging/build-windows.py --skip-build

build-appimage: build
	uv run ./packaging/build-appimage.py

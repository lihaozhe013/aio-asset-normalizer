.PHONY: run build clean dev

default: dev

dev:
	cargo run

build:
	cargo build --release

run:
	cargo run --release

clean:
	cargo clean

build-mac:
	cargo build --release && uv run ./packaging/build-mac-app.py

build-win:
	cargo build --release && uv run ./packaging/build-windows.py

build-appimage:
	cargo build --release && uv run ./packaging/build-appimage.py

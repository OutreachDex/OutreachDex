.DEFAULT_GOAL := dev

PNPM ?= pnpm

# Use rustup's proxies so rust-toolchain.toml also applies to Tauri's child processes.
GC_CARGO_BIN := $(if $(CARGO_HOME),$(CARGO_HOME),$(HOME)/.cargo)/bin
export PATH := $(GC_CARGO_BIN):$(PATH)

.PHONY: dev start install check-toolchain help

dev start: check-toolchain
	$(PNPM) agents:prepare
	$(PNPM) dev

check-toolchain:
	@command -v rustup >/dev/null || { echo 'rustup is required to use the Rust toolchain pinned in rust-toolchain.toml.' >&2; exit 1; }
	rustup show active-toolchain
	@cargo --version && rustc --version

install:
	$(PNPM) install --frozen-lockfile
	$(PNPM) --dir web install --frozen-lockfile

help:
	@echo 'make / make dev / make start  Start the Tauri desktop app (development mode)'
	@echo 'make install                  Install root and web dependencies'
	@echo 'Before the first start, prepare the editor artifacts as described in docs/development.md.'

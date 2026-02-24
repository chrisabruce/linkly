# Linkly — Makefile
# Usage: make <target>

BINARY   := linkly
RELEASE  := target/release/$(BINARY)
DEBUG    := target/debug/$(BINARY)
ENV_FILE := .env

.DEFAULT_GOAL := build

# -------------------------------------------------------
# Build
# -------------------------------------------------------

## build: compile a release binary
.PHONY: build
build:
	cargo build --release

## debug-build: compile a debug binary
.PHONY: debug-build
debug-build:
	cargo build

# -------------------------------------------------------
# Run
# -------------------------------------------------------

## run: build release binary and run it
.PHONY: run
run: build
	./$(RELEASE)

## dev: run in debug mode (faster compile, more logging)
.PHONY: dev
dev: setup
	RUST_LOG=linkly=debug,tower_http=debug cargo run

# -------------------------------------------------------
# Setup
# -------------------------------------------------------

## setup: create .env from .env.example if it doesn't exist
.PHONY: setup
setup:
	@if [ ! -f $(ENV_FILE) ]; then \
		cp .env.example $(ENV_FILE); \
		echo "✔  Created $(ENV_FILE) from .env.example — edit it before running."; \
	fi

# -------------------------------------------------------
# Code quality
# -------------------------------------------------------

## check: fast type-check without producing a binary
.PHONY: check
check:
	cargo check

## fmt: format source code
.PHONY: fmt
fmt:
	cargo fmt

## lint: run clippy with warnings as errors
.PHONY: lint
lint:
	cargo clippy -- -D warnings

## test: run the test suite
.PHONY: test
test:
	cargo test

# -------------------------------------------------------
# Maintenance
# -------------------------------------------------------

## clean: remove build artifacts
.PHONY: clean
clean:
	cargo clean

## install: install the release binary to /usr/local/bin
.PHONY: install
install: build
	install -m 755 $(RELEASE) /usr/local/bin/$(BINARY)
	@echo "✔  Installed $(BINARY) to /usr/local/bin"

## uninstall: remove the installed binary
.PHONY: uninstall
uninstall:
	rm -f /usr/local/bin/$(BINARY)
	@echo "✔  Removed /usr/local/bin/$(BINARY)"

# -------------------------------------------------------
# Help
# -------------------------------------------------------

## help: print this help message
.PHONY: help
help:
	@echo "Usage: make <target>\n"
	@grep -E '^## ' Makefile | sed 's/## /  /' | column -t -s ':'

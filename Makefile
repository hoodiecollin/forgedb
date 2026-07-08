# ForgeDB — root entry points. Everything runnable from the repo root; no `cd`.
# Rust workflows use cargo directly (see CLAUDE.md). JS/desktop workflows for the
# inspector app are wrapped here so they never require cd-ing into apps/inspector.

BUN := /Users/collin/.bun/bin/bun
INSPECTOR := apps/inspector

.PHONY: inspector-install inspector inspector-build inspector-typecheck \
        inspector-app inspector-app-build

## Install the inspector app's JS dependencies.
inspector-install:
	cd $(INSPECTOR) && $(BUN) install

## Run the inspector frontend in a browser (web-first dev; no desktop shell).
inspector:
	cd $(INSPECTOR) && $(BUN) run dev

## Build the inspector frontend to a static export (apps/inspector/out).
inspector-build:
	cd $(INSPECTOR) && $(BUN) run build

## Typecheck the inspector frontend.
inspector-typecheck:
	cd $(INSPECTOR) && $(BUN) run typecheck

## Run the inspector as a Tauri desktop app (added in the Tauri increment).
inspector-app:
	cargo tauri dev --config $(INSPECTOR)/src-tauri/tauri.conf.json

## Build the inspector desktop app for release.
inspector-app-build:
	cargo tauri build --config $(INSPECTOR)/src-tauri/tauri.conf.json

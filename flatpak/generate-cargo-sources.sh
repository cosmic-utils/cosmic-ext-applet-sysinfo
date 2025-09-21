#!/bin/bash

# Generate cargo sources for Flatpak
# This script generates the cargo-sources.json file needed for offline builds

uv run --script flatpak/flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json
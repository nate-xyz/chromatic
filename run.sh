#!/usr/bin/env bash

meson compile -C _builddir --verbose && \
RUST_LOG=trace meson devenv -C _builddir ./src/chromatic; exit;
.DEFAULT_GOAL := help
.PHONY: help build package
export ZIG_GLOBAL_CACHE_DIR := $(CURDIR)/.build/zig-global
export ZIG_LOCAL_CACHE_DIR := $(CURDIR)/.build/zig-local

help:
	@printf '%s\n' 'make build    Build the notice and HUD helpers' 'make package  Build the release tarball for the Homebrew formula'

build:
	swift build

package:
	./Tools/build.sh

.DEFAULT_GOAL := help
.PHONY: help build test lint app install run
export ZIG_GLOBAL_CACHE_DIR := $(CURDIR)/.build/zig-global
export ZIG_LOCAL_CACHE_DIR := $(CURDIR)/.build/zig-local

help:
	@printf '%s\n' 'make build    Build all Swift targets' 'make test     Build transcript and run tests' 'make lint     Check source boundaries' 'make app      Build a release Swarm.app' 'make install  Keep the old app and install the new one' 'make run      Build and launch Swarm.app'

build:
	swift build

test:
	zig build --build-file ../packages/transcript/build.zig -Doptimize=ReleaseSafe
	# Transcript subprocess tests can stall when Swift Testing runs them in parallel.
	SWARM_TRANSCRIPT_TOOL=$(abspath ../packages/transcript/zig-out/bin/transcript) swift test --disable-sandbox --no-parallel

lint:
	./Tools/lint.sh

app:
	./Tools/build.sh -r

install:
	./Tools/build.sh -r --install

run: app
	open .build/release/Swarm.app

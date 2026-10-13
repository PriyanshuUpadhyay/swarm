.DEFAULT_GOAL := help
.PHONY: help build test app install run dmg
export ZIG_GLOBAL_CACHE_DIR := $(CURDIR)/.build/zig-global
export ZIG_LOCAL_CACHE_DIR := $(CURDIR)/.build/zig-local

help:
	@printf '%s\n' 'make build    Build the shell and the notice helper' 'make test     Run the Swift tests' 'make app      Build a release Swarm.app' 'make install  Keep the old app and install the new one' 'make run      Build and launch Swarm.app' 'make dmg      Build Swarm.app and package it as a DMG'

build:
	swift build

test:
	swift test --disable-sandbox

app:
	./Tools/build.sh -r

install:
	./Tools/build.sh -r --install

run: app
	open .build/release/Swarm.app

dmg: app
	./Tools/make-dmg.sh

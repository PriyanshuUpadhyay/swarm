.DEFAULT_GOAL := help
.PHONY: help build package

help:
	@printf '%s\n' 'make build    Build the notice and HUD helpers' 'make package  Build the release tarball for the Homebrew formula'

build:
	swift build

package:
	./Tools/build.sh

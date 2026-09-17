# The front door. Everything real is a script in Tools; this is the index, so
# that "what can I run here" has an answer at the root without the root holding
# five shell scripts.
#
# Nothing is wrapped that would be worse for being wrapped. Every target below
# is the no-argument case, which is the one worth a short name. Anything that
# takes an argument (a test filter, a ref, a tag) is run directly:
#
#   ./Tools/test-core.sh DiffParser
#
# Make is not doing any dependency tracking here and is not meant to. Swift
# Package Manager already knows what needs rebuilding, and a Makefile that
# tried to second-guess it would be wrong the first time a file moved.

.DEFAULT_GOAL := help
.PHONY: help build test app run lint swiftlint

help:
	@grep -hE '^[a-z-]+:.*?## ' $(MAKEFILE_LIST) \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  make %-10s %s\n", $$1, $$2}'
	@echo
	@echo "  Anything that takes an argument is run directly. See the head of this file."

build: ## Compile every target, app included
	swift build

test: ## Run the SwarmCore suite without building the app target
	./Tools/test-core.sh

app: ## Assemble a debug Swarm.app
	./Tools/build.sh

run: ## Assemble a release Swarm.app and launch it
	./Tools/build.sh -r --run

lint: ## Check the house rules no off the shelf linter knows
	./Tools/house-rules.sh

swiftlint: ## Check the rules SwiftLint knows, against .swiftlint.yml
	./Tools/swiftlint.sh

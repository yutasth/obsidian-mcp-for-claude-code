.PHONY: build test test-unit test-integration clean

build:
	cargo build --release
	mkdir -p dist
	cp target/release/obsidian-mcp dist/

test: test-unit test-integration

test-unit:
	cargo test --lib

test-integration:
ifndef OBSIDIAN_TEST_VAULT
	$(error Set OBSIDIAN_TEST_VAULT env var to run integration tests)
endif
	cargo test --test integration -- --test-threads=1

clean:
	cargo clean
	rm -rf dist

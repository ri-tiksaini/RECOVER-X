.PHONY: test-rust build-py test-py cli test-all

test-rust:
	cargo test --workspace

cli:
	cargo build --release -p recoverx_cli

build-py:
	maturin develop --release

test-py:
	pytest tests/python -v

test-all: test-rust build-py test-py

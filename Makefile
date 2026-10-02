.PHONY: check build demo qa

check:
	cargo fmt --check
	cargo clippy --all-targets --locked -- -D warnings
	cargo test --locked

build:
	cargo build --release --locked

demo: build
	./target/release/resen --demo

qa: build
	python3 scripts/pty_qa.py --binary target/release/resen
	python3 scripts/process_qa.py --binary target/release/resen

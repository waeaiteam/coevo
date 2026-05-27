.PHONY: build test run clean docker-build docker-up docker-down check fmt clippy dev

# ---- Rust ----
build:
	cargo build --release -p coevo-server -p coevo-cli

test:
	cargo test --workspace

check:
	cargo check --workspace

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace -- -D warnings

run-server:
	RUST_LOG=coevo=debug,info cargo run -p coevo-server

run-cli:
	cargo run -p coevo-cli -- $(ARGS)

# ---- Docker ----
docker-build:
	docker compose build

docker-up:
	docker compose up -d

docker-down:
	docker compose down

# ---- Database ----
db-create:
	cargo run -p coevo-server -- --migrate

db-reset:
	rm -f data/coevo.db
	cargo run -p coevo-server -- --migrate

# ---- Dev (full stack) ----
dev: db-create
	RUST_LOG=coevo=debug,info cargo run -p coevo-server

dev-desktop:
	cd apps/desktop && npm run dev

# ---- Clean ----
clean:
	cargo clean
	rm -f data/coevo.db

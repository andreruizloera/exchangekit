# Contributing

Thanks for considering a contribution. This project aims to stay small,
readable, and honest about what it does.

## Ground rules

- exchangekit is a play-money simulator for education and research.
  Contributions that add real-money custody, payments, or brokerage
  features will not be accepted.
- Keep the engine deterministic and synchronous; concurrency belongs in
  the gateway.
- Every engine behavior change needs a test in engine/tests/.

## Development setup

Rust workspace (engine + gateway):

    cargo build
    cargo test
    cargo fmt --all
    cargo clippy --all-targets

Frontend:

    cd frontend
    npm install
    npm run dev        # against a locally running gateway on :8080
    npm run lint && npm run typecheck && npm test

Python SDK:

    cd sdk/python
    uv venv && uv pip install -e '.[dev]'
    .venv/bin/pytest   # integration tests skip unless a gateway is running

Full stack:

    docker compose up

## Pull requests

- Run the relevant formatter (cargo fmt, prettier, ruff format) before
  pushing.
- Describe the behavior change and how you verified it.
- New endpoints need coverage in the SDK and a note in the README's API
  table.

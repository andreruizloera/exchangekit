# Roadmap

Honest future work. None of this is implemented yet.

## Market structure

- Complementary matching: cross a YES bid at p against a NO bid at
  100 - p by minting a share pair, and burn pairs on the ask side.
  Today YES and NO trade in independent books, and demo liquidity comes
  from a seeded market-maker account.
- Market resolution: an admin endpoint that settles a market to YES or
  NO, pays out 100 cents per winning share, and voids resting orders.
- Market creation via the API and UI (today markets are seeded at boot).
- Short selling backed by cash collateral, so selling does not require
  an existing position.
- Multi-outcome (categorical) markets on top of the same engine.

## Exchange features

- Order types: market orders as a first-class type, immediate-or-cancel,
  fill-or-kill, post-only.
- Self-trade prevention policies (cancel-newest, cancel-oldest).
- Per-account order and message rate limits.
- Fees (maker/taker) to make simulations more realistic.
- PostgreSQL persistence as an alternative to JSON snapshots, with an
  event-sourced trade log.

## Interfaces

- Authentication (API keys) instead of the honor-system account field.
- WebSocket subscriptions scoped per market instead of one global feed.
- Candlestick history endpoint and a richer chart.
- SDK: async client, WebSocket streaming, order helpers (IOC emulation).
- A bot toolkit under examples/: random traders, a simple market maker,
  and an arbitrage checker for teaching.

## Operations

- Prometheus metrics endpoint.
- Property-based tests (proptest) that fuzz order streams against a
  reference matcher and check conservation invariants.

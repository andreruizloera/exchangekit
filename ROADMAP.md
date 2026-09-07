# Roadmap

Honest future work. None of this is implemented yet.

## The game

- Submit your own bot: a small strategy interface (in Rust, or over an RPC
  boundary) so a player can drop a bot into the pit and watch it trade,
  instead of only trading against the built-in five.
- Multi-round campaigns: a sequence of rounds with carried scoring and a
  rising difficulty curve, rather than one round at a time.
- An online leaderboard behind a gateway endpoint, so scores are shared
  rather than only kept in each browser's `localStorage`.
- A round replay: because the fair value and every bot decision are seeded,
  a finished round can be replayed tick for tick from its seed.
- More strategies and per-tier tuning knobs exposed in the UI (spread,
  volatility, jump frequency) so a round can be dialed in.
- Marking inventory to a settlement at the buzzer (resolve the asset to its
  final fair value) as an alternative to mark-to-mid scoring. The engine
  can settle a market now; what is missing is wiring it into the round and
  deciding what a mid-round resolution would do to the equity curve.

## Market structure

- A complementary trade updates only the taker's outcome price. A mint at
  63 on the YES side implies 37 on the NO side, but `price_estimate` for
  NO keeps reporting its own last print. Deriving the other side from a
  complementary trade would make both books' last prices agree.
- Complementary matching in a game round. A round trades one outcome of a
  throwaway market, so the second book is always empty and no pair is ever
  minted. Quoting both sides would change what the bots have to reason
  about, which is a design question and not just wiring.
- Burning against granted shares. A market whose shares carry no
  collateral cannot burn a pair, because there is nothing to release. An
  alternative would be to let the pool go negative and report the
  shortfall the way `unbacked_cash` does at settlement.
- Voiding a market instead of resolving it, refunding what each holder
  paid. This needs a cost basis per position, which the engine does not
  record, so it is a real piece of work and not a third enum variant.
- Settling a game round to the revealed fair value at the buzzer, as an
  alternative to mark-to-mid scoring. Rounds are deliberately excluded
  from `POST /api/markets/{id}/resolve` today.
- Market creation via the API and UI (today markets are seeded at boot).
- Short selling backed by cash collateral, so selling does not require
  an existing position.
- Multi-outcome (categorical) markets on top of the same engine.

## Exchange features

- Market orders as a first-class type. Today a taker writes a limit at 99
  or 1 and gets the same behavior, but an explicit market order would need
  a rule for an empty or thin book, which is a decision and not just a
  keyword.
- Self-trade prevention policies (cancel-newest, cancel-oldest). One
  account can currently cross itself, including complementarily, where it
  is just an expensive way to mint or redeem a pair.
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

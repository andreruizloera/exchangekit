# exchangekit

A trading game built on a real matching engine. You get a timed round on a
live limit order book, a hidden fair value you have to infer, and a pit full
of algorithmic bots to trade against. At the buzzer you get a scorecard:
PnL, return, Sharpe, and max drawdown, computed the way a desk would.

[![ci](https://github.com/andreruizloera/exchangekit/actions/workflows/ci.yml/badge.svg)](https://github.com/andreruizloera/exchangekit/actions/workflows/ci.yml)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![python 3.12+](https://img.shields.io/badge/python-3.12%2B-blue.svg)](sdk/python)

Underneath is a Rust matching engine with strict price-time priority, a REST
plus WebSocket gateway, and a dark trading terminal in React. The bots run
server-side and place and cancel real orders through the same engine you do.

**Play money only.** exchangekit is a simulator for education and research.
It has no real-money custody, no payments, no brokerage, and no
authentication. Nothing here is trading advice. Do not put real value behind
it.

![start screen](docs/images/game-start.png)

## The game

One round trades a single synthetic asset. Its true value is hidden and moves
as a random walk with drift and the occasional news jump. You never see it
directly; you infer it from the book and the tape. You start with play-money
cash and a block of inventory, quote and take through the ordinary order
ticket, and try to finish ahead. The clock runs 2 to 5 minutes.

![mid-round terminal](docs/images/game-round.png)

The terminal is live: an order book with depth bars, a price chart with your
own fills marked, a trade tape, and a PnL and equity readout that update on
every tick over the WebSocket feed. When the clock hits zero you get a
scorecard and a one-line grade.

![scorecard](docs/images/game-scorecard.png)

### The bots

Every bot has a real strategy and reasons about a private, noisy estimate of
the hidden fair value. They are named on the opponents panel during a round.

- **Drift** (noise trader): posts single orders on a random side at a random
  offset from a value that wanders far from the truth. This is the pickable
  liquidity everyone else feeds on.
- **Quotefill** (market maker): quotes both sides around its fair estimate to
  earn the spread. It widens when the tape is volatile and skews its quotes
  against its inventory to stay flat. On the hard tier it backs off when flow
  looks toxic, so it is not the one being picked off.
- **Chaser** (momentum): reads a short-term trend off the tape and takes
  liquidity in that direction, pushing price the way it is already moving.
- **Fade** (mean reversion): sells rallies and buys dips against its own
  moving average, supplying the counter-flow that anchors a quiet market.
- **Sniper** (informed, hard only): knows the fair value almost exactly and
  sweeps any resting order priced on the wrong side of it. This is the
  adverse selection that punishes a stale quote, yours included.

### The tiers

The tiers differ in ways that change how hard it is to make money, not just
in cosmetic numbers.

| Tier | Clock | Maker spread | Who is in the pit | Feel |
| --- | --- | --- | --- | --- |
| Easy | 1.1s | ~8c | 3 Drift, 1 Quotefill | Wide spreads and slow flow. Basic two-sided quoting captures the spread. |
| Medium | 0.7s | ~4c | 2 Drift, 1 Quotefill, 1 Chaser, 1 Fade | Tighter quotes and real trend pressure. You need an actual edge. |
| Hard | 0.4s | ~2c | 1 Drift, 2 Quotefill, 1 Chaser, 1 Sniper | Tight books that pull on toxic flow, plus a sniper that knows fair value. Lazy quoting gets adversely selected. |

### The scorecard

Everything is computed from your mark-to-market equity curve, sampled once per
tick, plus your trade count. Nothing is a made-up score.

- **PnL and return**: final equity minus starting equity, and that as a
  percent of where you started. Equity marks your inventory at the book
  midpoint, so holding a position while the fair value moves is real risk.
- **Sharpe**: the mean per-tick return over its standard deviation, scaled by
  the square root of the number of ticks so it reads on the usual Sharpe
  scale. A flat curve scores zero.
- **Max drawdown**: the worst peak-to-trough decline of the equity curve.
- **Grade**: a blunt one-liner from your return and Sharpe.

Your best runs per tier are kept in the browser via `localStorage` and shown
on the tier cards.

## Quickstart

```
git clone https://github.com/andreruizloera/exchangekit
cd exchangekit
docker compose up
```

Open http://localhost:3000, pick a tier, and play. The gateway API is on
http://localhost:8080.

Native, for faster iteration:

```
cargo run -p exchangekit-gateway          # gateway + engine on :8080
cd frontend && npm install && npm run dev # UI on :3000, proxies to :8080
```

Requirements: Docker (easiest), or a recent stable Rust toolchain (tested
with 1.97), Node 20+, and Python 3.12+ to run the pieces natively.

## Driving a round from code

The gateway exposes the game over plain HTTP, so you can start a round and
trade a bot against the pit without the UI:

```
# start a 3-minute hard round
curl -X POST localhost:8080/api/game/start -H 'content-type: application/json' \
  -d '{"tier":"hard","duration_secs":180}'

# read where it stands (clock, equity, PnL, opponents)
curl localhost:8080/api/game

# trade the round's market as the "player" account
curl -X POST localhost:8080/api/orders -H 'content-type: application/json' \
  -d '{"account":"player","market":"game-1","outcome":"YES","side":"BUY","price":50,"quantity":20}'

# the scorecard, once the clock runs out
curl localhost:8080/api/game/scorecard
```

| Method | Path | Purpose |
| --- | --- | --- |
| POST | /api/game/start | start a round: `{tier, duration_secs?, seed?}` |
| GET | /api/game | live round state: clock, equity, PnL, opponents |
| GET | /api/game/scorecard | the scorecard once the round has ended |

## The exchange underneath

The bots and your orders all go through one matching engine. The core
exchange is also usable on its own, with three seeded prediction markets, a
seeded market maker, and the full order lifecycle. That is what the Python
SDK and `./demo.sh` talk to.

`./demo.sh` against a fresh gateway, parts 1 to 4:

```
== markets ==
btc-100k      YES 63c  NO 37c  vol  105  Will Bitcoin close above $100,000 this year?
fed-cut-dec   YES 44c  NO 56c  vol  157  Will the Fed cut rates at its December meeting?
mars-2030     YES  8c  NO 92c  vol  209  Will humans land on Mars before 2030?

== order book: btc-100k YES (top 3) ==
  ask 66c  x 90
  ask 65c  x 260
  ask 64c  x 180
  ----
  bid 62c  x 180
  bid 61c  x 260
  bid 60c  x 120

== demo buys 10 YES at the best ask ==
  order 79: filled, filled 10/10
  trade: 10 YES @ 64c (demo bought from marketmaker)

== demo account after the trade ==
  balance $10,451.30 (available $10,451.30)
  position: 540 YES shares in btc-100k
```

The same thing through the SDK:

```python
from exchangekit import Client   # pip install ./sdk/python

client = Client("http://localhost:8080")
market = client.market("btc-100k")
client.buy(market=market.id, outcome="YES", price=0.62, quantity=10)
```

Prices are integer cents from 1 to 99. A binary contract settles at 0 or 100
cents, so buying YES at 63c risks $0.63 to win $1.00 per share. Orders are
limit orders; a marketable limit order fills immediately against resting
prices and any remainder rests in the book. Sells require shares; buys escrow
cash at the limit price.

Core REST API (the seeded exchange):

| Method | Path | Purpose |
| --- | --- | --- |
| GET | /api/markets | list seeded markets with price estimates and volume |
| GET | /api/markets/{id}/book?outcome=YES&depth=20 | aggregated bids and asks |
| GET | /api/markets/{id}/trades?limit=50 | recent trades, newest first |
| POST | /api/orders | place a limit order |
| DELETE | /api/orders/{id}?account=demo | cancel an open order |
| GET | /api/accounts/{id} | play-money balance |
| POST | /api/markets/{id}/resolve | settle the market: `{outcome}` |
| GET | /api/markets/{id}/settlement | the settlement report of a resolved market |
| WS | /ws | hello snapshot, then trade, book, market, and resolution events |

The Python SDK wraps the exchange with typed methods: `markets`, `market`,
`book`, `trades`, `buy`, `sell`, `place_order`, `cancel`, `order`,
`open_orders`, `positions`, `balance`, `resolve`, `settlement`. See
[sdk/python](sdk/python). The per-round game markets are hidden from
`GET /api/markets`, so the SDK and the demo only ever see the seeded
exchange.

## Settling a market

That "settles at 0 or 100 cents" is the whole point of a binary contract, so
the exchange can do it. Resolving a market pays 100 cents for every share of
the winning outcome, pays nothing for the other side, voids every resting
order, and closes the market for good. This is part 5 of `./demo.sh`, run
against a fresh gateway:

```
== mars-2030 before resolution ==
  status:     open
  collateral: $101,500.00 backing its outstanding shares
  demo holds: 556 YES, 500 NO

== resolve mars-2030 to NO ==
  winner:        NO
  paid out:      $101,500.00 to 4 account(s)
  shares:        101,500 winning, 101,500 losing
  collateral:    $101,500.00 held by the market
  unbacked cash: $0.00
  voided:        18 resting orders, releasing $1,624.56 and 3,634 shares of escrow
    alice           546 winning x $1.00 = $546.00
    bob             500 winning x $1.00 = $500.00
    demo            500 winning x $1.00 = $500.00
    marketmaker  99,954 winning x $1.00 = $99,954.00

== the market is closed ==
  book: 0 price levels left on either side
  new order:     400 market mars-2030 has resolved and no longer trades
  resolve again: 409 market mars-2030 has already resolved
```

Four things in that output are decisions worth naming.

**Orders are voided before anything is paid.** A resting sell has shares
locked against it and a resting buy has cash locked against it. Clearing
positions first would leave the escrow pointing at a position that no longer
exists, and the account would carry a lock it could never release on a market
that no longer trades. Releasing first means every share the payout sees is
one its owner holds free and clear, including the 3,634 that were sitting in
sell orders.

**A voided order is not a cancelled one.** Its status is `voided`: the market
resolved underneath it, which is not something its owner chose.

**Unbacked cash is reported, not hidden.** Trading conserves cash, because a
buyer's cents become a seller's cents. Settlement is different: it pays
against shares, and a share only funds itself if it was minted as a YES/NO
pair against 100 cents of collateral. The seeded exchange mints every share
that way, which is why the payout above exactly matches the collateral and
`unbacked cash` reads `$0.00`. The engine also has `grant_shares`, which
creates shares for free (the game hands out one-sided inventory that way),
and settling those creates cash out of nothing. The engine does not forbid
it. It refuses to hide it: the shortfall is computed on every settlement and
printed with a name.

**Resolution happens once.** A second call is a 409, not a second payout.

```python
settlement = client.resolve("mars-2030", "NO")
settlement.total_paid, settlement.unbacked_cash   # (10150000, 0)
client.market("mars-2030").resolved_outcome       # 'NO'
```

There is no authentication anywhere in the gateway, so anyone who can reach
it can settle a market. That is acceptable in a local simulator running on
your own machine and would not be acceptable anywhere else.

## Architecture

```mermaid
flowchart LR
    subgraph clients
        UI[React terminal<br/>tier select, book, tape, chart, scorecard]
        SDK[Python SDK<br/>exchangekit.Client]
        CURL[curl / anything HTTP]
    end
    subgraph gateway [gateway crate axum]
        REST[REST API]
        GAME[game runtime<br/>tick loop, fair value, bots]
        WS[WebSocket fanout<br/>broadcast channel]
    end
    ENGINE[engine crate<br/>price-time priority matching<br/>+ game module: bots, scoring, tiers]

    UI -->|HTTP| REST
    SDK -->|HTTP| REST
    CURL -->|HTTP| REST
    UI <-->|events| WS
    REST -->|"RwLock&lt;Exchange&gt;"| ENGINE
    GAME -->|bot orders every tick| ENGINE
    GAME -->|book, trade events| WS
    REST -->|trade, book, market events| WS
```

The engine is a synchronous, deterministic library with no I/O. Its `game`
module adds the pieces the round needs, all pure and seedable: a fair-value
process, the bot strategies, the difficulty tiers, and the scoring math. The
gateway wraps the engine in an `RwLock` and runs a background tick task: each
tick it advances the fair value, has every bot cancel and requote through the
real `place_order` path, samples your equity, and publishes the fresh book
over the broadcast channel. Because the game logic is pure, a round is
reproducible from a seed and every decision is unit-tested.

## Testing

```
cargo test                        # engine + game + gateway
cd frontend && npm test           # scoring and formatting unit tests
cd sdk/python && .venv/bin/pytest # SDK unit tests; integration tests
                                  # run when a gateway is up, skip otherwise
./demo.sh                         # needs a gateway; fails if any line it
                                  # prints has drifted from this README
```

Engine tests cover the matcher (price and time priority, partial and full
fills, marketable limits, cancels and escrow, conservation, snapshots),
settlement (payout arithmetic, voiding and escrow release on both books,
the closed market, resolving twice, cash conservation across a fully paired
market, and unbacked cash when shares were granted), and the game logic: the
seeded PRNG, the fair-value process staying in bounds, each bot strategy's
core decision from a fixed seed, the Sharpe and drawdown math against known
equity curves, and tier composition. Frontend tests cover the scorecard grade
thresholds and the number formatting. CI runs the demo and the SDK
integration tests against a real gateway, so the output pasted above cannot
drift from the tool without the build going red.

## Limitations

- No shorting: selling requires owning shares, so both you and the bots start
  each round with an inventory to work down. Managing that inventory is part
  of the game.
- YES and NO books are independent; the game trades a single side.
- A market resolves to YES or NO and nothing else. There is no void or refund
  outcome, because the engine records what a share is worth at settlement and
  not what anyone paid for it, so it has nothing to refund against.
- Settling a granted share creates cash. `grant_shares` mints shares with no
  collateral, so a market resolved while any are outstanding pays out more
  than it holds. The settlement reports that as `unbacked_cash` and the
  seeded exchange never does it, but the engine will not stop you.
- Game rounds do not settle. A round is scored by marking inventory to the
  book at the buzzer, and `POST /api/markets/game-N/resolve` is refused,
  because settling a round's market out from under the tick loop would
  rewrite the player's equity mid-game.
- State is in memory. The core exchange can restore from a JSON snapshot
  (`EXCHANGEKIT_SNAPSHOT`), but game rounds are ephemeral by design.
- No authentication: any client can act as any account, and that now includes
  resolving a market and paying out every holder. This is a local simulator,
  not a service.

See [ROADMAP.md](ROADMAP.md) for planned work.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). The short version: keep the engine
deterministic, add a test for every matching or strategy behavior change, and
nothing that touches real money will be merged.

## License

MIT. See [LICENSE](LICENSE).

GitHub topics: trading-game, market-microstructure, algorithmic-trading,
matching-engine, orderbook, prediction-markets.

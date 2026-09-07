# exchangekit (Python SDK)

Typed Python client for the exchangekit gateway, a local play-money
exchange simulator. No real money is involved anywhere.

```python
from exchangekit import Client

client = Client("http://localhost:8080")

market = client.market("btc-100k")
print(market.question, market.yes_price)

result = client.buy(market=market.id, outcome="YES", price=0.62, quantity=10)
print(result.order.status, result.trades)

print(client.balance())
print(client.positions())
```

Prices can be a fraction of a dollar (`0.62`) or integer cents (`62`).
All amounts returned by the API are integer play-money cents.

A YES share and a NO share of one market settle for 100 cents between
them, so bidding 44 for YES is offering NO at 56 and the two books trade
against each other. Two buyers on opposite outcomes cross by minting the
pair they are paying for; two sellers cross by burning one. Trades say
which happened in `kind`.

```python
client.buy(market="fed-cut-dec", outcome="YES", price=0.44, quantity=5)

other = Client("http://localhost:8080", account="alice")
trade = other.buy(market="fed-cut-dec", outcome="NO", price=0.56, quantity=5).trades[0]
print(trade.kind, trade.price)      # mint 56
print(trade.seller)                 # demo, who bought the other outcome
```

`mint` and `redeem` are the same thing without a counterparty: a pair costs
100 cents to create and returns 100 cents on demand, because exactly one of
its two shares wins.

```python
print(client.mint("fed-cut-dec", 10).collateral)    # up $10.00
print(client.redeem("fed-cut-dec", 10).collateral)  # and back down
```

Shares committed to a resting sell order cannot be redeemed; cancel the
order first.

A binary contract settles at 0 or 100 cents. `resolve` pays out every
winning share, voids every resting order, and closes the market; it cannot
be undone, and a second call raises `ExchangeKitError` with status 409.

```python
settlement = client.resolve("mars-2030", "NO")
print(settlement.total_paid, settlement.unbacked_cash)
for payout in settlement.payouts:
    print(payout.account, payout.winning_shares, payout.paid)

print(client.market("mars-2030").resolved_outcome)   # 'NO'
print(client.settlement("mars-2030").orders_voided)  # read it back later
```

`unbacked_cash` is the part of the payout no collateral stood behind. It
is zero when every share was minted as a YES/NO pair, which is how the
seeded exchange creates all of its shares.

Install from the repository root:

```
pip install ./sdk/python
```

See the top-level README at https://github.com/andreruizloera/exchangekit
for how to start the gateway.

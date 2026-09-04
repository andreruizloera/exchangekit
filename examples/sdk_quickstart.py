"""Minimal SDK tour: browse markets, read a book, trade, check accounts.

Run a gateway first (docker compose up), then:

    pip install ./sdk/python
    python examples/sdk_quickstart.py
"""

from exchangekit import Client

client = Client("http://localhost:8080", account="demo")

print("markets:")
for market in client.markets():
    print(f"  {market.id}: YES {market.yes_price}c  ({market.question})")

market = client.market("btc-100k")
book = client.book(market.id, outcome="YES")
print(f"\nbtc-100k YES book: best bid {book.bids[0].price}c, best ask {book.asks[0].price}c")

result = client.buy(market=market.id, outcome="YES", price=0.62, quantity=10)
print(f"\nplaced order {result.order.id}: {result.order.status}")
for trade in result.trades:
    print(f"  filled {trade.quantity} @ {trade.price}c against {trade.seller}")
if result.order.remaining > 0:
    print(f"  {result.order.remaining} resting at {result.order.price}c")
    client.cancel(result.order.id)
    print("  cancelled the remainder")

balance = client.balance()
print(f"\nbalance: {balance.balance / 100:,.2f} play dollars")
for position in client.positions():
    if position.market == market.id:
        print(f"position: {position.quantity} {position.outcome} shares in {position.market}")

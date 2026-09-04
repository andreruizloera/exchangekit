"""A tiny noise trader: every second, hit or join the book at random in
one market. Useful for making the UI feel alive while you develop.

    python examples/random_trader.py [market] [account]
"""

import random
import sys
import time

from exchangekit import Client, ExchangeKitError

market = sys.argv[1] if len(sys.argv) > 1 else "btc-100k"
account = sys.argv[2] if len(sys.argv) > 2 else "alice"
client = Client("http://localhost:8080", account=account)

print(f"trading {market} as {account}; ctrl-c to stop")
while True:
    book = client.book(market, outcome=random.choice(["YES", "NO"]))
    bid = book.bids[0].price if book.bids else 40
    ask = book.asks[0].price if book.asks else 60
    side = random.choice(["BUY", "SELL"])
    # Half the orders are marketable, half rest inside the spread.
    if side == "BUY":
        price = ask if random.random() < 0.5 else max(1, bid + random.randint(0, 1))
    else:
        price = bid if random.random() < 0.5 else min(99, ask - random.randint(0, 1))
    quantity = random.randint(1, 12)
    try:
        result = client.place_order(market, book.outcome, side, price, quantity)
        fills = sum(t.quantity for t in result.trades)
        print(f"{side} {quantity} {book.outcome} @ {price}c -> filled {fills}")
    except ExchangeKitError as e:
        print(f"rejected: {e.message}")
    time.sleep(1)

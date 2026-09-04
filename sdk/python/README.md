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

Install from the repository root:

```
pip install ./sdk/python
```

See the top-level README at https://github.com/andreruizloera/exchangekit
for how to start the gateway.

"""HTTP client for the exchangekit gateway.

Usage:

    from exchangekit import Client

    client = Client("http://localhost:8080")
    market = client.market("btc-100k")
    client.buy(market=market.id, outcome="YES", price=0.62, quantity=10)

Prices can be given either as a fraction of a dollar (0.62) or as
integer cents (62); both mean 62 play-money cents per share.
"""

from __future__ import annotations

from typing import Any

import httpx

from .models import (
    Balance,
    Book,
    Level,
    Market,
    Order,
    OrderResult,
    PairResult,
    Payout,
    Position,
    Settlement,
    Trade,
)


class ExchangeKitError(RuntimeError):
    """Raised when the gateway rejects a request."""

    def __init__(self, status_code: int, message: str) -> None:
        super().__init__(f"{status_code}: {message}")
        self.status_code = status_code
        self.message = message


def _to_cents(price: float | int) -> int:
    """Accept 0.62 (fraction of a dollar) or 62 (cents); return cents."""
    if isinstance(price, float) and 0 < price < 1:
        return round(price * 100)
    if float(price).is_integer() and 1 <= int(price) <= 99:
        return int(price)
    raise ValueError(f"price must be 0.01 to 0.99 or 1 to 99 cents, got {price!r}")


class Client:
    """Synchronous client bound to one account (default: 'demo')."""

    def __init__(
        self,
        base_url: str = "http://localhost:8080",
        account: str = "demo",
        *,
        timeout: float = 10.0,
        transport: httpx.BaseTransport | None = None,
    ) -> None:
        self.account = account
        self._http = httpx.Client(
            base_url=base_url.rstrip("/"), timeout=timeout, transport=transport
        )

    def close(self) -> None:
        self._http.close()

    def __enter__(self) -> Client:
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()

    # -- plumbing ----------------------------------------------------------

    def _request(self, method: str, path: str, **kwargs: Any) -> Any:
        resp = self._http.request(method, path, **kwargs)
        if resp.status_code >= 400:
            try:
                message = resp.json().get("error", resp.text)
            except ValueError:
                message = resp.text
            raise ExchangeKitError(resp.status_code, message)
        return resp.json()

    # -- markets -----------------------------------------------------------

    def markets(self) -> list[Market]:
        return [Market.from_dict(m) for m in self._request("GET", "/api/markets")]

    def market(self, market_id: str) -> Market:
        return Market.from_dict(self._request("GET", f"/api/markets/{market_id}"))

    def book(self, market: str, outcome: str = "YES", depth: int = 20) -> Book:
        data = self._request(
            "GET",
            f"/api/markets/{market}/book",
            params={"outcome": outcome, "depth": depth},
        )
        return Book.from_dict(data)

    def trades(self, market: str, limit: int = 50) -> list[Trade]:
        data = self._request("GET", f"/api/markets/{market}/trades", params={"limit": limit})
        return [Trade.from_dict(t) for t in data]

    # -- pairs -------------------------------------------------------------

    def mint(self, market: str, quantity: int) -> PairResult:
        """Buy ``quantity`` YES/NO pairs at 100 cents each.

        The account pays a dollar per pair and receives one share of each
        outcome; the cents become the market's collateral. This is the
        funded way to create shares, and it is why a seeded market settles
        without creating cash.
        """
        return PairResult.from_dict(
            self._request(
                "POST",
                f"/api/markets/{market}/mint",
                json={"account": self.account, "quantity": quantity},
            )
        )

    def redeem(self, market: str, quantity: int) -> PairResult:
        """Sell ``quantity`` YES/NO pairs back for 100 cents each.

        The inverse of :meth:`mint`, and the reason a pair is worth a dollar
        before the market resolves rather than only after. Shares committed
        to a resting sell order do not count; cancel the order first.
        """
        return PairResult.from_dict(
            self._request(
                "POST",
                f"/api/markets/{market}/redeem",
                json={"account": self.account, "quantity": quantity},
            )
        )

    # -- settlement --------------------------------------------------------

    def resolve(self, market: str, outcome: str) -> Settlement:
        """Settle a market: pay 100 cents per winning share, void every
        resting order, and close the market for good.

        This is an admin operation and the gateway has no authentication,
        so it is not bound to this client's account. It cannot be undone
        and a second call raises ``ExchangeKitError`` with status 409.
        """
        data = self._request(
            "POST",
            f"/api/markets/{market}/resolve",
            json={"outcome": outcome.upper()},
        )
        return Settlement.from_dict(data)

    def settlement(self, market: str) -> Settlement:
        """The settlement report of an already-resolved market."""
        return Settlement.from_dict(self._request("GET", f"/api/markets/{market}/settlement"))

    # -- orders ------------------------------------------------------------

    def place_order(
        self,
        market: str,
        outcome: str,
        side: str,
        price: float | int,
        quantity: int,
    ) -> OrderResult:
        body = {
            "account": self.account,
            "market": market,
            "outcome": outcome.upper(),
            "side": side.upper(),
            "price": _to_cents(price),
            "quantity": quantity,
        }
        return OrderResult.from_dict(self._request("POST", "/api/orders", json=body))

    def buy(self, market: str, outcome: str, price: float | int, quantity: int) -> OrderResult:
        return self.place_order(market, outcome, "BUY", price, quantity)

    def sell(self, market: str, outcome: str, price: float | int, quantity: int) -> OrderResult:
        return self.place_order(market, outcome, "SELL", price, quantity)

    def order(self, order_id: int) -> Order:
        return Order.from_dict(self._request("GET", f"/api/orders/{order_id}"))

    def cancel(self, order_id: int) -> Order:
        data = self._request("DELETE", f"/api/orders/{order_id}", params={"account": self.account})
        return Order.from_dict(data)

    def open_orders(self) -> list[Order]:
        data = self._request("GET", f"/api/accounts/{self.account}/orders")
        return [Order.from_dict(o) for o in data]

    # -- account -----------------------------------------------------------

    def balance(self) -> Balance:
        d = self._request("GET", f"/api/accounts/{self.account}")
        return Balance(
            account=d["id"], balance=d["balance"], locked=d["locked"], available=d["available"]
        )

    def positions(self) -> list[Position]:
        data = self._request("GET", f"/api/accounts/{self.account}/positions")
        return [
            Position(
                market=p["market"],
                outcome=p["outcome"],
                quantity=p["quantity"],
                locked=p["locked"],
            )
            for p in data
        ]


__all__ = [
    "Balance",
    "Book",
    "Client",
    "ExchangeKitError",
    "Level",
    "Market",
    "Order",
    "OrderResult",
    "PairResult",
    "Payout",
    "Position",
    "Settlement",
    "Trade",
]

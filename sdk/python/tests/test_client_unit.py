"""Unit tests against a mocked transport; no server required."""

from __future__ import annotations

import json

import httpx
import pytest

from exchangekit import Client, ExchangeKitError
from exchangekit.client import _to_cents

MARKET = {
    "id": "btc-100k",
    "question": "Will Bitcoin close above $100,000 this year?",
    "description": "",
    "created_at": 1000,
    "yes_price": 63,
    "no_price": 37,
    "volume": 105,
}

ORDER = {
    "id": 7,
    "account": "demo",
    "market": "btc-100k",
    "outcome": "YES",
    "side": "BUY",
    "price": 62,
    "quantity": 10,
    "filled": 4,
    "status": "open",
    "seq": 7,
    "created_at": 1000,
}

TRADE = {
    "id": 1,
    "market": "btc-100k",
    "outcome": "YES",
    "price": 62,
    "quantity": 4,
    "taker_side": "BUY",
    "buyer": "demo",
    "seller": "alice",
    "buy_order": 7,
    "sell_order": 3,
    "ts": 1000,
}


def make_client(handler: httpx.MockTransport) -> Client:
    return Client("http://test", account="demo", transport=handler)


def test_to_cents_accepts_fractions_and_cents() -> None:
    assert _to_cents(0.62) == 62
    assert _to_cents(0.01) == 1
    assert _to_cents(0.99) == 99
    assert _to_cents(62) == 62
    assert _to_cents(1) == 1
    for bad in (0, 100, 1.5, -0.2, 62.5):
        with pytest.raises(ValueError):
            _to_cents(bad)


def test_market_and_markets_parse() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path == "/api/markets":
            return httpx.Response(200, json=[MARKET])
        assert request.url.path == "/api/markets/btc-100k"
        return httpx.Response(200, json=MARKET)

    client = make_client(httpx.MockTransport(handler))
    markets = client.markets()
    assert len(markets) == 1
    market = client.market("btc-100k")
    assert market.id == "btc-100k"
    assert market.yes_price == 63
    assert market.volume == 105


def test_book_parses_levels() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.url.path == "/api/markets/btc-100k/book"
        assert request.url.params["outcome"] == "NO"
        assert request.url.params["depth"] == "5"
        return httpx.Response(
            200,
            json={
                "market": "btc-100k",
                "outcome": "NO",
                "bids": [{"price": 36, "quantity": 100}],
                "asks": [{"price": 38, "quantity": 50}],
            },
        )

    client = make_client(httpx.MockTransport(handler))
    book = client.book("btc-100k", outcome="NO", depth=5)
    assert book.bids[0].price == 36
    assert book.asks[0].quantity == 50


def test_buy_sends_correct_body_and_parses_result() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.method == "POST"
        assert request.url.path == "/api/orders"
        body = json.loads(request.content)
        assert body == {
            "account": "demo",
            "market": "btc-100k",
            "outcome": "YES",
            "side": "BUY",
            "price": 62,
            "quantity": 10,
        }
        return httpx.Response(200, json={"order": ORDER, "trades": [TRADE]})

    client = make_client(httpx.MockTransport(handler))
    result = client.buy(market="btc-100k", outcome="YES", price=0.62, quantity=10)
    assert result.order.id == 7
    assert result.order.remaining == 6
    assert result.trades[0].price == 62


def test_sell_uses_sell_side() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        body = json.loads(request.content)
        assert body["side"] == "SELL"
        assert body["price"] == 70
        return httpx.Response(200, json={"order": ORDER, "trades": []})

    client = make_client(httpx.MockTransport(handler))
    result = client.sell(market="btc-100k", outcome="yes", price=70, quantity=5)
    assert result.trades == []


def test_cancel_passes_account() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.method == "DELETE"
        assert request.url.path == "/api/orders/7"
        assert request.url.params["account"] == "demo"
        return httpx.Response(200, json={**ORDER, "status": "cancelled"})

    client = make_client(httpx.MockTransport(handler))
    assert client.cancel(7).status == "cancelled"


def test_balance_and_positions() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path == "/api/accounts/demo":
            return httpx.Response(
                200, json={"id": "demo", "balance": 1000, "locked": 100, "available": 900}
            )
        assert request.url.path == "/api/accounts/demo/positions"
        return httpx.Response(
            200,
            json=[{"market": "btc-100k", "outcome": "YES", "quantity": 40, "locked": 10}],
        )

    client = make_client(httpx.MockTransport(handler))
    bal = client.balance()
    assert bal.available == 900
    pos = client.positions()
    assert pos[0].available == 30


def test_gateway_errors_raise() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(400, json={"error": "quantity must be positive"})

    client = make_client(httpx.MockTransport(handler))
    with pytest.raises(ExchangeKitError) as exc:
        client.buy(market="btc-100k", outcome="YES", price=0.5, quantity=0)
    assert exc.value.status_code == 400
    assert "quantity" in exc.value.message

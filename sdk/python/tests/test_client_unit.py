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
    "status": "open",
    "resolved_outcome": None,
    "resolved_at": None,
    "collateral": 10_150_000,
}

SETTLEMENT = {
    "market": "mars-2030",
    "outcome": "NO",
    "resolved_at": 1700,
    "payouts": [
        {"account": "alice", "winning_shares": 480, "losing_shares": 520, "paid": 48_000},
        {
            "account": "marketmaker",
            "winning_shares": 100_020,
            "losing_shares": 99_980,
            "paid": 10_002_000,
        },
    ],
    "total_paid": 10_050_000,
    "winning_shares": 100_500,
    "losing_shares": 100_500,
    "orders_voided": 18,
    "cash_released": 123_456,
    "shares_released": 90,
    "collateral": 10_150_000,
    "unbacked_cash": 0,
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
    "kind": "match",
    "ts": 1000,
}

PAIR_RESULT = {
    "account": "demo",
    "market": "btc-100k",
    "quantity": 100,
    "balance": 1_055_130,
    "available": 1_055_130,
    "yes": 440,
    "no": 400,
    "collateral": 10_140_000,
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
    assert market.status == "open"
    assert not market.is_resolved
    assert market.collateral == 10_150_000


def test_a_resolved_market_reports_its_winner() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200,
            json={**MARKET, "status": "resolved", "resolved_outcome": "NO", "resolved_at": 1700},
        )

    client = make_client(httpx.MockTransport(handler))
    market = client.market("mars-2030")
    assert market.is_resolved
    assert market.resolved_outcome == "NO"


def test_market_fields_added_after_v0_1_have_defaults() -> None:
    """A gateway that predates settlement omits these keys entirely; the
    client must still parse its markets rather than raising KeyError."""
    old = {
        k: v
        for k, v in MARKET.items()
        if k not in {"status", "resolved_outcome", "resolved_at", "collateral"}
    }

    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(200, json=old)

    client = make_client(httpx.MockTransport(handler))
    market = client.market("btc-100k")
    assert market.status == "open"
    assert market.resolved_outcome is None
    assert market.collateral == 0


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


def test_a_complementary_trade_reports_how_the_shares_were_created() -> None:
    mint = {**TRADE, "kind": "mint", "outcome": "NO", "price": 37, "seller": "alice"}
    burn = {**TRADE, "kind": "burn", "outcome": "YES", "price": 63}

    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(200, json=[mint, burn, TRADE])

    client = make_client(httpx.MockTransport(handler))
    trades = client.trades("btc-100k")
    assert [t.kind for t in trades] == ["mint", "burn", "match"]
    assert [t.is_complementary for t in trades] == [True, True, False]
    # The mint still names a buyer and a seller: alice bought the other
    # outcome, which is the same trade seen from her side.
    assert trades[0].seller == "alice"


def test_a_trade_from_a_gateway_without_complementary_matching_reads_as_a_match() -> None:
    old = {k: v for k, v in TRADE.items() if k != "kind"}

    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(200, json=[old])

    client = make_client(httpx.MockTransport(handler))
    assert client.trades("btc-100k")[0].kind == "match"


def test_mint_sends_the_account_and_parses_the_result() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.method == "POST"
        assert request.url.path == "/api/markets/btc-100k/mint"
        assert json.loads(request.content) == {"account": "demo", "quantity": 100}
        return httpx.Response(200, json=PAIR_RESULT)

    client = make_client(httpx.MockTransport(handler))
    result = client.mint("btc-100k", 100)
    assert result.quantity == 100
    assert result.yes == 440
    assert result.collateral == 10_140_000


def test_redeem_hits_the_redeem_path() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.url.path == "/api/markets/btc-100k/redeem"
        assert json.loads(request.content)["quantity"] == 25
        return httpx.Response(200, json={**PAIR_RESULT, "quantity": 25})

    client = make_client(httpx.MockTransport(handler))
    assert client.redeem("btc-100k", 25).quantity == 25


def test_redeeming_more_than_the_market_holds_raises() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            400, json={"error": "insufficient collateral: need 100 cents, market holds 0"}
        )

    client = make_client(httpx.MockTransport(handler))
    with pytest.raises(ExchangeKitError) as exc:
        client.redeem("btc-100k", 1)
    assert exc.value.status_code == 400
    assert "insufficient collateral" in exc.value.message


def test_resolve_sends_the_outcome_and_parses_the_settlement() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.method == "POST"
        assert request.url.path == "/api/markets/mars-2030/resolve"
        assert json.loads(request.content) == {"outcome": "NO"}
        return httpx.Response(200, json=SETTLEMENT)

    client = make_client(httpx.MockTransport(handler))
    settlement = client.resolve("mars-2030", "no")
    assert settlement.outcome == "NO"
    assert settlement.total_paid == 10_050_000
    assert settlement.unbacked_cash == 0
    assert settlement.orders_voided == 18
    assert [p.account for p in settlement.payouts] == ["alice", "marketmaker"]
    assert settlement.payouts[0].paid == 48_000


def test_settlement_reads_an_already_resolved_market() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        assert request.method == "GET"
        assert request.url.path == "/api/markets/mars-2030/settlement"
        return httpx.Response(200, json=SETTLEMENT)

    client = make_client(httpx.MockTransport(handler))
    assert client.settlement("mars-2030").winning_shares == 100_500


def test_resolving_twice_raises_a_conflict() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(409, json={"error": "market mars-2030 has already resolved"})

    client = make_client(httpx.MockTransport(handler))
    with pytest.raises(ExchangeKitError) as exc:
        client.resolve("mars-2030", "YES")
    assert exc.value.status_code == 409
    assert "already resolved" in exc.value.message


def test_gateway_errors_raise() -> None:
    def handler(request: httpx.Request) -> httpx.Response:
        return httpx.Response(400, json={"error": "quantity must be positive"})

    client = make_client(httpx.MockTransport(handler))
    with pytest.raises(ExchangeKitError) as exc:
        client.buy(market="btc-100k", outcome="YES", price=0.5, quantity=0)
    assert exc.value.status_code == 400
    assert "quantity" in exc.value.message

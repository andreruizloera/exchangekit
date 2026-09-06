"""Integration tests against a running gateway.

Skipped automatically when nothing is listening on EXCHANGEKIT_URL
(default http://localhost:8080), so CI without a server stays green.
Start the stack with `docker compose up` or `cargo run -p
exchangekit-gateway` to run these for real.
"""

from __future__ import annotations

import os

import httpx
import pytest

from exchangekit import Client, ExchangeKitError

BASE_URL = os.environ.get("EXCHANGEKIT_URL", "http://localhost:8080")


def gateway_running() -> bool:
    try:
        return httpx.get(f"{BASE_URL}/api/health", timeout=1.0).status_code == 200
    except httpx.HTTPError:
        return False


pytestmark = pytest.mark.skipif(not gateway_running(), reason=f"no gateway at {BASE_URL}")


@pytest.fixture()
def client() -> Client:
    with Client(BASE_URL, account="demo") as c:
        yield c


def test_seeded_markets_are_visible(client: Client) -> None:
    markets = {m.id for m in client.markets()}
    assert {"btc-100k", "fed-cut-dec", "mars-2030"} <= markets
    market = client.market("btc-100k")
    assert market.yes_price is not None
    assert 1 <= market.yes_price <= 99


def test_book_has_two_sides(client: Client) -> None:
    book = client.book("btc-100k", outcome="YES")
    assert book.bids and book.asks
    assert book.bids[0].price < book.asks[0].price


def test_place_fill_and_positions_roundtrip(client: Client) -> None:
    before = client.balance()
    book = client.book("btc-100k", outcome="YES")
    ask = book.asks[0].price
    result = client.buy(market="btc-100k", outcome="YES", price=ask, quantity=1)
    assert result.trades, "marketable buy at the ask must fill"
    assert result.trades[0].price <= ask
    after = client.balance()
    assert after.balance == before.balance - result.trades[0].price
    assert any(p.market == "btc-100k" and p.outcome == "YES" for p in client.positions())


def test_resting_order_and_cancel(client: Client) -> None:
    # A 1 cent bid can never cross the seeded book; it must rest.
    result = client.buy(market="btc-100k", outcome="YES", price=0.01, quantity=1)
    assert result.trades == []
    assert result.order.status == "open"
    assert any(o.id == result.order.id for o in client.open_orders())
    cancelled = client.cancel(result.order.id)
    assert cancelled.status == "cancelled"
    assert all(o.id != result.order.id for o in client.open_orders())


def test_recent_trades_present(client: Client) -> None:
    trades = client.trades("btc-100k", limit=5)
    assert trades, "seeded market has trade history"
    assert all(1 <= t.price <= 99 for t in trades)


def test_seeded_market_is_open_and_fully_collateralized(client: Client) -> None:
    market = client.market("btc-100k")
    assert market.status == "open"
    assert not market.is_resolved
    assert market.resolved_outcome is None
    # Every seeded share was minted as a pair, so the market holds 100
    # cents for each outstanding pair and can settle without creating cash.
    assert market.collateral > 0


def test_an_open_market_has_no_settlement_to_read(client: Client) -> None:
    """These tests deliberately never resolve anything: resolution cannot
    be undone, and a shared gateway would be left settled for whatever ran
    next. The live resolve path is exercised by ./demo.sh."""
    with pytest.raises(ExchangeKitError) as exc:
        client.settlement("btc-100k")
    assert exc.value.status_code == 404
    assert "has not resolved" in exc.value.message


def test_resolving_a_game_market_is_refused(client: Client) -> None:
    with pytest.raises(ExchangeKitError) as exc:
        client.resolve("game-1", "YES")
    assert exc.value.status_code == 400
    assert "game round" in exc.value.message

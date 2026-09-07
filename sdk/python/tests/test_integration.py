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


#: Complementary tests use fed-cut-dec, which the other tests leave alone.
#: Its seeded book quotes YES 43/45 and NO 55/57, so 44 and 56 rest on both
#: sides and add up to exactly the 100 cents a pair is worth.
PAIR_MARKET = "fed-cut-dec"
YES_PRICE = 44
NO_PRICE = 56


@pytest.fixture()
def client() -> Client:
    with Client(BASE_URL, account="demo") as c:
        yield c


@pytest.fixture()
def other() -> Client:
    """A second account, so a complementary cross has two sides."""
    with Client(BASE_URL, account="alice") as c:
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


def test_an_ioc_order_never_rests(client: Client) -> None:
    # A 1 cent bid crosses nothing on the seeded book, so an ordinary limit
    # order would rest here and an immediate-or-cancel one must not.
    result = client.buy(
        market="btc-100k", outcome="YES", price=0.01, quantity=1, time_in_force="ioc"
    )
    assert result.trades == []
    assert result.order.status == "cancelled"
    assert result.order.time_in_force == "immediate_or_cancel"
    assert all(o.id != result.order.id for o in client.open_orders())


def test_a_fill_or_kill_beyond_the_books_is_rejected(client: Client) -> None:
    """The seeded market has real depth but not a million shares of it, so
    this one is killed. Nothing is escrowed and nothing rests."""
    before = client.balance()
    result = client.buy(
        market="btc-100k", outcome="YES", price=0.99, quantity=1_000_000, time_in_force="fok"
    )
    assert result.order.is_rejected
    assert result.trades == []
    assert client.balance() == before, "a killed order takes no escrow"
    assert all(o.id != result.order.id for o in client.open_orders())


def test_a_post_only_order_is_refused_rather_than_taking(client: Client) -> None:
    book = client.book("btc-100k", outcome="YES")
    ask = book.asks[0].price
    taking = client.buy(
        market="btc-100k", outcome="YES", price=ask, quantity=1, time_in_force="post_only"
    )
    assert taking.order.is_rejected
    assert taking.trades == []

    quoting = client.buy(
        market="btc-100k", outcome="YES", price=0.01, quantity=1, time_in_force="post_only"
    )
    assert quoting.order.status == "open"
    client.cancel(quoting.order.id)


def test_an_unknown_time_in_force_is_a_bad_request(client: Client) -> None:
    with pytest.raises(ExchangeKitError) as exc:
        client.buy(market="btc-100k", outcome="YES", price=0.01, quantity=1, time_in_force="asap")
    assert exc.value.status_code == 400
    assert "time in force" in exc.value.message


def test_recent_trades_present(client: Client) -> None:
    trades = client.trades("btc-100k", limit=5)
    assert trades, "seeded market has trade history"
    assert all(1 <= t.price <= 99 for t in trades)


def test_complementary_bids_mint_a_pair(client: Client, other: Client) -> None:
    """Bidding 44 for YES and 56 for NO is the same trade from both sides.
    Nobody has to already hold a share: the pair is created against the 100
    cents the two buyers pay between them."""
    before = client.market(PAIR_MARKET).collateral

    resting = client.buy(market=PAIR_MARKET, outcome="YES", price=YES_PRICE, quantity=5)
    assert resting.trades == [], "44 is inside the spread, so it rests"

    crossing = other.buy(market=PAIR_MARKET, outcome="NO", price=NO_PRICE, quantity=5)
    assert len(crossing.trades) == 1
    trade = crossing.trades[0]
    assert trade.kind == "mint"
    assert trade.is_complementary
    assert trade.outcome == "NO", "priced from the taker's side"
    assert trade.price == NO_PRICE
    assert trade.buyer == "alice"
    assert trade.seller == "demo", "bidding 44 for YES is offering NO at 56"
    assert client.market(PAIR_MARKET).collateral == before + 5 * 100


def test_complementary_asks_burn_a_pair(client: Client, other: Client) -> None:
    """The other direction: two sellers cross, the pair is destroyed, and
    the 100 cents behind it is released to pay them."""
    before = client.market(PAIR_MARKET).collateral

    resting = client.sell(market=PAIR_MARKET, outcome="YES", price=YES_PRICE, quantity=5)
    assert resting.trades == []

    crossing = other.sell(market=PAIR_MARKET, outcome="NO", price=NO_PRICE, quantity=5)
    assert len(crossing.trades) == 1
    trade = crossing.trades[0]
    assert trade.kind == "burn"
    assert trade.seller == "alice"
    assert trade.buyer == "demo", "offering NO at 56 is bidding 44 for YES"
    assert client.market(PAIR_MARKET).collateral == before - 5 * 100


def test_minting_and_redeeming_a_pair_is_a_round_trip(client: Client) -> None:
    start_cash = client.balance().balance
    start_collateral = client.market(PAIR_MARKET).collateral

    minted = client.mint(PAIR_MARKET, 10)
    assert minted.balance == start_cash - 1_000
    assert minted.collateral == start_collateral + 1_000

    redeemed = client.redeem(PAIR_MARKET, 10)
    assert redeemed.balance == start_cash
    assert redeemed.collateral == start_collateral
    assert redeemed.yes == minted.yes - 10
    assert redeemed.no == minted.no - 10


def test_redeeming_a_game_market_is_refused(client: Client) -> None:
    with pytest.raises(ExchangeKitError) as exc:
        client.redeem("game-1", 1)
    assert exc.value.status_code == 400
    assert "game round" in exc.value.message


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

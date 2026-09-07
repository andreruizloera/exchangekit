"""Typed views of gateway responses. All prices are integer play-money
cents (1 to 99); a binary contract settles at 0 or 100 cents."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class Market:
    id: str
    question: str
    description: str
    created_at: int
    yes_price: int | None
    no_price: int | None
    volume: int
    #: "open" while the market trades, "resolved" once it has settled.
    status: str = "open"
    #: The winning outcome, or None while the market is open. Note that
    #: yes_price and no_price keep reporting the last traded price after
    #: resolution; they describe the tape, not the settlement.
    resolved_outcome: str | None = None
    resolved_at: int | None = None
    #: Cents backing the market's outstanding shares, zero once paid out.
    collateral: int = 0

    @property
    def is_resolved(self) -> bool:
        return self.status == "resolved"

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> Market:
        return cls(
            id=d["id"],
            question=d["question"],
            description=d["description"],
            created_at=d["created_at"],
            yes_price=d.get("yes_price"),
            no_price=d.get("no_price"),
            volume=d["volume"],
            status=d.get("status", "open"),
            resolved_outcome=d.get("resolved_outcome"),
            resolved_at=d.get("resolved_at"),
            collateral=d.get("collateral", 0),
        )


@dataclass(frozen=True)
class Level:
    price: int
    quantity: int


@dataclass(frozen=True)
class Book:
    market: str
    outcome: str
    bids: list[Level]
    asks: list[Level]

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> Book:
        return cls(
            market=d["market"],
            outcome=d["outcome"],
            bids=[Level(x["price"], x["quantity"]) for x in d["bids"]],
            asks=[Level(x["price"], x["quantity"]) for x in d["asks"]],
        )


@dataclass(frozen=True)
class Order:
    id: int
    account: str
    market: str
    outcome: str
    side: str
    price: int
    quantity: int
    filled: int
    #: "open", "filled", "cancelled", "voided" (the market resolved
    #: underneath it), or "rejected" (its own terms refused it: a
    #: fill-or-kill that could not fill, or a post-only that would take).
    status: str
    created_at: int
    #: "good_till_cancelled" (the default), "immediate_or_cancel",
    #: "fill_or_kill", or "post_only". A gateway that predates order types
    #: omits the field.
    time_in_force: str = "good_till_cancelled"

    @property
    def remaining(self) -> int:
        return self.quantity - self.filled

    @property
    def is_rejected(self) -> bool:
        return self.status == "rejected"

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> Order:
        return cls(
            id=d["id"],
            account=d["account"],
            market=d["market"],
            outcome=d["outcome"],
            side=d["side"],
            price=d["price"],
            quantity=d["quantity"],
            filled=d["filled"],
            status=d["status"],
            created_at=d["created_at"],
            time_in_force=d.get("time_in_force", "good_till_cancelled"),
        )


@dataclass(frozen=True)
class Trade:
    id: int
    market: str
    #: The outcome this trade is priced in: the taker's side of it.
    outcome: str
    price: int
    quantity: int
    taker_side: str
    buyer: str
    seller: str
    ts: int
    #: Where the shares came from: "match" for an ordinary cross inside one
    #: book, "mint" when two buyers on opposite outcomes were crossed and
    #: the pair was created against collateral, "burn" when two sellers
    #: were crossed and the pair was destroyed. On a mint the "seller"
    #: bought the complementary outcome and never held this one; on a burn
    #: the "buyer" sold the complementary outcome and never received this
    #: one. A gateway that predates complementary matching omits the field.
    kind: str = "match"

    @property
    def is_complementary(self) -> bool:
        """True when this trade crossed the two books against each other."""
        return self.kind in ("mint", "burn")

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> Trade:
        return cls(
            id=d["id"],
            market=d["market"],
            outcome=d["outcome"],
            price=d["price"],
            quantity=d["quantity"],
            taker_side=d["taker_side"],
            buyer=d["buyer"],
            seller=d["seller"],
            kind=d.get("kind", "match"),
            ts=d["ts"],
        )


@dataclass(frozen=True)
class OrderResult:
    """An accepted order plus any trades it produced immediately."""

    order: Order
    trades: list[Trade]

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> OrderResult:
        return cls(
            order=Order.from_dict(d["order"]),
            trades=[Trade.from_dict(t) for t in d["trades"]],
        )


@dataclass(frozen=True)
class Position:
    market: str
    outcome: str
    quantity: int
    locked: int

    @property
    def available(self) -> int:
        return self.quantity - self.locked


@dataclass(frozen=True)
class Balance:
    account: str
    balance: int
    locked: int
    available: int


@dataclass(frozen=True)
class PairResult:
    """The state of one account in one market after minting or redeeming
    YES/NO pairs.

    A pair costs 100 cents to mint and returns 100 cents when redeemed,
    because exactly one of its two shares wins. The cents live in the
    market's ``collateral`` pool in between.
    """

    account: str
    market: str
    #: Pairs minted or redeemed by the call that returned this.
    quantity: int
    balance: int
    available: int
    yes: int
    no: int
    #: Cents the market holds against its outstanding shares, after the call.
    collateral: int

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> PairResult:
        return cls(
            account=d["account"],
            market=d["market"],
            quantity=d["quantity"],
            balance=d["balance"],
            available=d["available"],
            yes=d["yes"],
            no=d["no"],
            collateral=d["collateral"],
        )


@dataclass(frozen=True)
class Payout:
    """What one account received when a market resolved."""

    account: str
    winning_shares: int
    losing_shares: int
    paid: int

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> Payout:
        return cls(
            account=d["account"],
            winning_shares=d["winning_shares"],
            losing_shares=d["losing_shares"],
            paid=d["paid"],
        )


@dataclass(frozen=True)
class Settlement:
    """The report of a resolution: who was paid, what was voided, and
    whether the payout was funded.

    ``unbacked_cash`` is the part of ``total_paid`` that no collateral
    stood behind. It is zero when every share was minted as a YES/NO pair
    and positive when shares were granted for free, which is the engine's
    way of naming money it created rather than moved.
    """

    market: str
    outcome: str
    resolved_at: int
    payouts: list[Payout]
    total_paid: int
    winning_shares: int
    losing_shares: int
    orders_voided: int
    cash_released: int
    shares_released: int
    collateral: int
    unbacked_cash: int

    @classmethod
    def from_dict(cls, d: dict[str, Any]) -> Settlement:
        return cls(
            market=d["market"],
            outcome=d["outcome"],
            resolved_at=d["resolved_at"],
            payouts=[Payout.from_dict(p) for p in d["payouts"]],
            total_paid=d["total_paid"],
            winning_shares=d["winning_shares"],
            losing_shares=d["losing_shares"],
            orders_voided=d["orders_voided"],
            cash_released=d["cash_released"],
            shares_released=d["shares_released"],
            collateral=d["collateral"],
            unbacked_cash=d["unbacked_cash"],
        )

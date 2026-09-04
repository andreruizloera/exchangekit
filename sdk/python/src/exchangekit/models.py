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
    status: str
    created_at: int

    @property
    def remaining(self) -> int:
        return self.quantity - self.filled

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
        )


@dataclass(frozen=True)
class Trade:
    id: int
    market: str
    outcome: str
    price: int
    quantity: int
    taker_side: str
    buyer: str
    seller: str
    ts: int

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

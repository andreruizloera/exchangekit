"""exchangekit: Python client for a local play-money exchange simulator."""

from .client import Client, ExchangeKitError
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

__version__ = "0.1.0"

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
    "__version__",
]

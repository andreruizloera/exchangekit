import type { BookView, OutcomeId } from "../types";
import { formatQty } from "../lib/format";

interface Props {
  book: BookView | null;
  outcome: OutcomeId;
  onOutcomeChange: (o: OutcomeId) => void;
  onPriceClick: (price: number) => void;
}

export function OrderBook({ book, outcome, onOutcomeChange, onPriceClick }: Props) {
  const asks = book ? book.asks.slice(0, 9).reverse() : []; // best ask at the bottom
  const bids = book ? book.bids.slice(0, 9) : []; // best bid at the top
  const maxQty = Math.max(1, ...asks.map((l) => l.quantity), ...bids.map((l) => l.quantity));
  const bestAsk = book?.asks[0];
  const bestBid = book?.bids[0];
  const spread = bestAsk && bestBid ? bestAsk.price - bestBid.price : null;

  const row = (price: number, quantity: number, kind: "bid" | "ask") => (
    <button
      key={`${kind}-${price}`}
      className={`book-row book-${kind}`}
      onClick={() => onPriceClick(price)}
      title={`set ticket price to ${price}c`}
    >
      <span
        className="depth-bar"
        style={{ width: `${Math.min(100, (quantity / maxQty) * 100)}%` }}
      />
      <span className="book-price">{price}c</span>
      <span className="book-qty">{formatQty(quantity)}</span>
    </button>
  );

  return (
    <section className="panel book">
      <div className="panel-head">
        <span className="panel-title">Order book</span>
        <div className="tabs">
          {(["YES", "NO"] as const).map((o) => (
            <button
              key={o}
              className={o === outcome ? `tab tab-${o.toLowerCase()} active` : "tab"}
              onClick={() => onOutcomeChange(o)}
            >
              {o}
            </button>
          ))}
        </div>
      </div>
      <div className="book-cols">
        <span>price</span>
        <span>size</span>
      </div>
      <div className="book-side">{asks.map((l) => row(l.price, l.quantity, "ask"))}</div>
      <div className="book-spread">{spread !== null ? `spread ${spread}c` : "one-sided book"}</div>
      <div className="book-side">{bids.map((l) => row(l.price, l.quantity, "bid"))}</div>
    </section>
  );
}

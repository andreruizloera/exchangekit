import { useState } from "react";

import type { BookView, OutcomeId, PlaceResult, SideId } from "../types";
import { formatCash } from "../lib/format";

interface Props {
  outcome: OutcomeId;
  onOutcomeChange: (o: OutcomeId) => void;
  price: number;
  onPriceChange: (p: number) => void;
  book: BookView | null;
  onSubmit: (
    outcome: OutcomeId,
    side: SideId,
    price: number,
    quantity: number,
  ) => Promise<PlaceResult>;
}

export function OrderTicket({
  outcome,
  onOutcomeChange,
  price,
  onPriceChange,
  book,
  onSubmit,
}: Props) {
  const [side, setSide] = useState<SideId>("BUY");
  const [quantity, setQuantity] = useState(10);
  const [feedback, setFeedback] = useState<{ kind: "ok" | "err"; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const bestBid = book?.bids[0]?.price;
  const bestAsk = book?.asks[0]?.price;
  const cost = price * quantity;
  const validPrice = Number.isInteger(price) && price >= 1 && price <= 99;
  const validQty = Number.isInteger(quantity) && quantity > 0;

  const submit = async () => {
    if (!validPrice || !validQty || busy) return;
    setBusy(true);
    setFeedback(null);
    try {
      const result = await onSubmit(outcome, side, price, quantity);
      const filled = result.order.filled;
      const rest = result.order.quantity - filled;
      const fillNote =
        filled > 0
          ? `filled ${filled} @ ${result.trades.map((t) => `${t.price}c`).join(", ")}`
          : "";
      const restNote = rest > 0 ? `resting ${rest} @ ${result.order.price}c` : "";
      setFeedback({
        kind: "ok",
        text: [fillNote, restNote].filter(Boolean).join("; ") || "accepted",
      });
    } catch (e) {
      setFeedback({ kind: "err", text: e instanceof Error ? e.message : "order rejected" });
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="panel ticket">
      <div className="panel-title">Order ticket</div>
      <div className="tabs full">
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
      <div className="tabs full">
        {(["BUY", "SELL"] as const).map((s) => (
          <button
            key={s}
            className={s === side ? `tab tab-${s === "BUY" ? "yes" : "no"} active` : "tab"}
            onClick={() => setSide(s)}
          >
            {s === "BUY" ? "Buy" : "Sell"}
          </button>
        ))}
      </div>

      <label className="field">
        <span>Limit price (cents)</span>
        <input
          type="number"
          min={1}
          max={99}
          value={price}
          onChange={(e) => onPriceChange(Number(e.target.value))}
        />
      </label>
      <div className="quick-prices">
        {bestBid !== undefined && (
          <button onClick={() => onPriceChange(bestBid)}>bid {bestBid}c</button>
        )}
        {bestBid !== undefined && bestAsk !== undefined && (
          <button onClick={() => onPriceChange(Math.round((bestBid + bestAsk) / 2))}>mid</button>
        )}
        {bestAsk !== undefined && (
          <button onClick={() => onPriceChange(bestAsk)}>ask {bestAsk}c</button>
        )}
      </div>
      <label className="field">
        <span>Quantity (shares)</span>
        <input
          type="number"
          min={1}
          value={quantity}
          onChange={(e) => setQuantity(Number(e.target.value))}
        />
      </label>

      <div className="ticket-summary">
        <div>
          <span>{side === "BUY" ? "Max cost" : "Min proceeds"}</span>
          <span className="header-value">{formatCash(cost)}</span>
        </div>
        <div>
          <span>Payout if {outcome} wins</span>
          <span className="header-value">{formatCash(quantity * 100)}</span>
        </div>
      </div>

      <button
        className={side === "BUY" ? "submit submit-buy" : "submit submit-sell"}
        disabled={!validPrice || !validQty || busy}
        onClick={() => void submit()}
      >
        {busy ? "working" : `${side === "BUY" ? "Buy" : "Sell"} ${outcome} @ ${price}c`}
      </button>

      {feedback && (
        <div className={feedback.kind === "ok" ? "feedback ok" : "feedback err"}>
          {feedback.text}
        </div>
      )}
    </section>
  );
}

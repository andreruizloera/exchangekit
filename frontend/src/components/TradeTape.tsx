import type { Trade } from "../types";
import { formatQty, formatTime } from "../lib/format";

interface Props {
  trades: Trade[];
}

export function TradeTape({ trades }: Props) {
  return (
    <section className="panel tape">
      <div className="panel-title">Recent trades</div>
      <div className="tape-cols">
        <span>time</span>
        <span>outcome</span>
        <span>price</span>
        <span>size</span>
      </div>
      <div className="tape-rows">
        {trades.length === 0 && <div className="empty">no trades yet</div>}
        {trades.map((t) => (
          <div key={t.id} className="tape-row">
            <span className="tape-time">{formatTime(t.ts)}</span>
            <span className={t.outcome === "YES" ? "text-yes" : "text-no"}>{t.outcome}</span>
            <span className={t.taker_side === "BUY" ? "text-yes" : "text-no"}>{t.price}c</span>
            <span className="tape-qty">{formatQty(t.quantity)}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

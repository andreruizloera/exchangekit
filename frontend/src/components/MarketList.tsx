import type { MarketSummary } from "../types";
import { formatPrice, formatQty } from "../lib/format";

interface Props {
  markets: MarketSummary[];
  selected: string | null;
  onSelect: (id: string) => void;
}

export function MarketList({ markets, selected, onSelect }: Props) {
  return (
    <nav className="panel market-list">
      <div className="panel-title">Markets</div>
      {markets.map((m) => (
        <button
          key={m.id}
          className={m.id === selected ? "market-row selected" : "market-row"}
          onClick={() => onSelect(m.id)}
        >
          <div className="market-question">{m.question}</div>
          <div className="market-meta">
            <span className="chip chip-yes">YES {formatPrice(m.yes_price)}</span>
            <span className="chip chip-no">NO {formatPrice(m.no_price)}</span>
            <span className="market-volume">{formatQty(m.volume)} vol</span>
          </div>
        </button>
      ))}
    </nav>
  );
}

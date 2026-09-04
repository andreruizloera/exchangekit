import type { Scorecard as ScorecardData, TierId } from "../types";
import { formatCash, formatPct, formatSharpe, formatSignedCash, grade } from "../lib/format";

interface Props {
  tier: TierId;
  card: ScorecardData;
  fairValue: number | null;
  onAgain: () => void;
}

export function Scorecard({ tier, card, fairValue, onAgain }: Props) {
  const win = card.pnl >= 0;
  return (
    <div className="scorecard-overlay">
      <section className="scorecard">
        <div className="scorecard-head">
          <span className={`tier-tag tier-${tier}`}>{tier}</span>
          <span className="panel-title">round over</span>
        </div>

        <div className={win ? "scorecard-pnl text-yes" : "scorecard-pnl text-no"}>
          {formatSignedCash(card.pnl)}
        </div>
        <div className="scorecard-return">
          {formatPct(card.return_pct)} on {formatCash(card.starting_equity)}
        </div>
        <p className="scorecard-grade">{grade(card.return_pct, card.sharpe)}</p>

        <div className="scorecard-grid">
          <Stat label="Sharpe" value={formatSharpe(card.sharpe)} />
          <Stat label="Max drawdown" value={`${(card.max_drawdown * 100).toFixed(1)}%`} />
          <Stat label="Trades" value={card.trades.toLocaleString("en-US")} />
          <Stat label="Final equity" value={formatCash(card.final_equity)} />
          <Stat label="Ticks" value={card.ticks.toLocaleString("en-US")} />
          <Stat
            label="Fair value at close"
            value={fairValue === null ? "--" : `${fairValue}c (opened 50c)`}
          />
        </div>

        <button className="submit submit-buy scorecard-again" onClick={onAgain}>
          Back to tiers
        </button>
      </section>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="scorecard-stat">
      <span className="header-label">{label}</span>
      <span className="scorecard-stat-value">{value}</span>
    </div>
  );
}

import { useState } from "react";

import type { TierId } from "../types";
import { leaderboard } from "../lib/leaderboard";
import { formatPct, formatSharpe } from "../lib/format";

interface Props {
  onStart: (tier: TierId, durationSecs: number) => void;
  starting: boolean;
}

interface TierCard {
  tier: TierId;
  title: string;
  tagline: string;
  facts: string[];
  opponents: string[];
}

const TIERS: TierCard[] = [
  {
    tier: "easy",
    title: "Easy",
    tagline: "Wide spreads, a slow clock, forgiving flow.",
    facts: ["Tick 1.1s", "Maker quotes ~8c wide", "No informed flow"],
    opponents: ["3x Drift noise traders", "1x Quotefill market maker"],
  },
  {
    tier: "medium",
    title: "Medium",
    tagline: "Tighter quotes and real trend pressure. Bring an edge.",
    facts: ["Tick 0.7s", "Maker quotes ~4c wide", "Momentum and mean reversion"],
    opponents: ["2x Drift", "1x Quotefill", "1x Chaser", "1x Fade"],
  },
  {
    tier: "hard",
    title: "Hard",
    tagline: "Tight books that pull on toxic flow, and a sniper that knows fair value.",
    facts: ["Tick 0.4s", "Maker quotes ~2c wide", "Adverse selection on stale quotes"],
    opponents: ["1x Drift", "2x Quotefill", "1x Chaser", "1x Sniper"],
  },
];

const DURATIONS: { label: string; secs: number }[] = [
  { label: "2 min", secs: 120 },
  { label: "3 min", secs: 180 },
  { label: "5 min", secs: 300 },
];

export function StartScreen({ onStart, starting }: Props) {
  const [duration, setDuration] = useState(180);

  return (
    <div className="menu">
      <div className="menu-head">
        <div className="brand">
          <span className="brand-mark">EK</span>
          <span className="brand-name">exchangekit</span>
          <span className="badge">play money</span>
        </div>
        <h1 className="menu-title">Trade the tape. Beat the bots.</h1>
        <p className="menu-sub">
          A timed round against algorithmic traders on a live limit order book. One asset, a hidden
          fair value, and a scorecard at the buzzer. Pick a tier.
        </p>
      </div>

      <div className="menu-duration">
        <span className="header-label">Round length</span>
        <div className="tabs">
          {DURATIONS.map((d) => (
            <button
              key={d.secs}
              className={d.secs === duration ? "tab active tab-yes" : "tab"}
              onClick={() => setDuration(d.secs)}
            >
              {d.label}
            </button>
          ))}
        </div>
      </div>

      <div className="tier-grid">
        {TIERS.map((t) => {
          const best = leaderboard(t.tier);
          return (
            <section key={t.tier} className={`tier-card tier-${t.tier}`}>
              <div className="tier-card-head">
                <h2>{t.title}</h2>
                <span className="tier-dot" />
              </div>
              <p className="tier-tagline">{t.tagline}</p>
              <ul className="tier-facts">
                {t.facts.map((f) => (
                  <li key={f}>{f}</li>
                ))}
              </ul>
              <div className="tier-opponents">
                <span className="panel-title">In the pit</span>
                {t.opponents.map((o) => (
                  <div key={o} className="tier-opponent">
                    {o}
                  </div>
                ))}
              </div>
              {best.length > 0 && (
                <div className="tier-best">
                  <span className="panel-title">Your best</span>
                  {best.slice(0, 3).map((e, i) => (
                    <div key={i} className="tier-best-row">
                      <span className={e.return_pct >= 0 ? "text-yes" : "text-no"}>
                        {formatPct(e.return_pct)}
                      </span>
                      <span className="tape-time">Sharpe {formatSharpe(e.sharpe)}</span>
                    </div>
                  ))}
                </div>
              )}
              <button
                className="submit submit-buy tier-start"
                disabled={starting}
                onClick={() => onStart(t.tier, duration)}
              >
                {starting ? "dealing in" : `Play ${t.title}`}
              </button>
            </section>
          );
        })}
      </div>

      <p className="menu-disclaimer">
        exchangekit is a play-money simulator for education and research. No real money, custody, or
        payments. Nothing here is trading advice.
      </p>
    </div>
  );
}

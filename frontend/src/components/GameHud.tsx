import type { Balance, GameState, PositionRow } from "../types";
import { formatCash, formatClock, formatPct, formatSignedCash } from "../lib/format";

interface Props {
  game: GameState;
  balance: Balance | null;
  positions: PositionRow[];
  connected: boolean;
  onQuit: () => void;
}

export function GameHud({ game, balance, positions, connected, onQuit }: Props) {
  const equity = game.player_equity;
  const pnl = game.pnl;
  const returnPct = game.starting_equity ? (pnl / game.starting_equity) * 100 : 0;
  const shares = positions.reduce((sum, p) => sum + p.quantity, 0);
  const low = game.remaining_secs <= 15;

  return (
    <header className="hud">
      <div className="hud-left">
        <span className="brand-mark">EK</span>
        <span className={`tier-tag tier-${game.tier}`}>{game.tier}</span>
        <button className="hud-quit" onClick={onQuit} title="end this round">
          quit
        </button>
      </div>

      <div className={low ? "hud-clock hud-clock-low" : "hud-clock"}>
        {formatClock(game.remaining_secs)}
      </div>

      <div className="hud-right">
        <div className="hud-stat">
          <span className="header-label">equity</span>
          <span className="header-value">{formatCash(equity)}</span>
        </div>
        <div className="hud-stat">
          <span className="header-label">pnl</span>
          <span className={pnl >= 0 ? "header-value text-yes" : "header-value text-no"}>
            {formatSignedCash(pnl)} ({formatPct(returnPct)})
          </span>
        </div>
        <div className="hud-stat">
          <span className="header-label">inventory</span>
          <span className="header-value">{shares.toLocaleString("en-US")}</span>
        </div>
        <div className="hud-stat">
          <span className="header-label">cash</span>
          <span className="header-value">{balance ? formatCash(balance.available) : "--"}</span>
        </div>
        <span className={connected ? "ws-dot ws-on" : "ws-dot ws-off"} title="feed status" />
      </div>
    </header>
  );
}

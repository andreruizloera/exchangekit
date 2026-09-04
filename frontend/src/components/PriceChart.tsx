import type { Trade } from "../types";

interface Props {
  trades: Trade[]; // newest first, mixed outcomes
}

const W = 640;
const H = 190;
const PAD = 8;

/** Line chart of YES trade prices for the selected market. */
export function PriceChart({ trades }: Props) {
  const yes = trades
    .filter((t) => t.outcome === "YES")
    .slice()
    .reverse(); // oldest first

  if (yes.length < 2) {
    return (
      <div className="chart chart-empty">
        <span>waiting for trades</span>
      </div>
    );
  }

  const prices = yes.map((t) => t.price);
  const min = Math.min(...prices);
  const max = Math.max(...prices);
  const span = Math.max(max - min, 4);
  const lo = Math.max(0, min - span * 0.25);
  const hi = Math.min(100, max + span * 0.25);

  const x = (i: number) => PAD + (i * (W - 2 * PAD)) / (yes.length - 1);
  const y = (p: number) => H - PAD - ((p - lo) * (H - 2 * PAD)) / (hi - lo);
  const points = yes.map((t, i) => `${x(i).toFixed(1)},${y(t.price).toFixed(1)}`);
  const last = yes[yes.length - 1];
  const lastY = last ? y(last.price) : 0;

  const gridLines = [25, 50, 75].filter((g) => g > lo && g < hi);

  return (
    <div className="chart">
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img">
        <defs>
          <linearGradient id="fill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--yes)" stopOpacity="0.22" />
            <stop offset="100%" stopColor="var(--yes)" stopOpacity="0" />
          </linearGradient>
        </defs>
        {gridLines.map((g) => (
          <g key={g}>
            <line
              x1={PAD}
              x2={W - PAD}
              y1={y(g)}
              y2={y(g)}
              stroke="var(--border)"
              strokeDasharray="3 5"
            />
            <text x={W - PAD} y={y(g) - 3} textAnchor="end" className="chart-grid-label">
              {g}c
            </text>
          </g>
        ))}
        <polygon
          points={`${PAD},${H - PAD} ${points.join(" ")} ${W - PAD},${H - PAD}`}
          fill="url(#fill)"
        />
        <polyline
          points={points.join(" ")}
          fill="none"
          stroke="var(--yes)"
          strokeWidth="1.8"
          vectorEffect="non-scaling-stroke"
        />
        {last && (
          <circle cx={W - PAD} cy={lastY} r="3" fill="var(--yes)">
            <title>{`last YES trade ${last.price}c`}</title>
          </circle>
        )}
      </svg>
      {last && <div className="chart-caption">YES last {last.price}c</div>}
    </div>
  );
}

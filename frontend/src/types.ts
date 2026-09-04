export type OutcomeId = "YES" | "NO";
export type SideId = "BUY" | "SELL";

export interface MarketSummary {
  id: string;
  question: string;
  description: string;
  created_at: number;
  yes_price: number | null;
  no_price: number | null;
  volume: number;
}

export interface Level {
  price: number;
  quantity: number;
}

export interface BookView {
  market: string;
  outcome: OutcomeId;
  bids: Level[];
  asks: Level[];
}

export interface Order {
  id: number;
  account: string;
  market: string;
  outcome: OutcomeId;
  side: SideId;
  price: number;
  quantity: number;
  filled: number;
  status: "open" | "filled" | "cancelled";
  created_at: number;
}

export interface Trade {
  id: number;
  market: string;
  outcome: OutcomeId;
  price: number;
  quantity: number;
  taker_side: SideId;
  buyer: string;
  seller: string;
  ts: number;
}

export interface PlaceResult {
  order: Order;
  trades: Trade[];
}

export interface Balance {
  id: string;
  balance: number;
  locked: number;
  available: number;
}

export interface PositionRow {
  market: string;
  outcome: OutcomeId;
  quantity: number;
  locked: number;
}

export type WsEvent =
  | { type: "hello"; markets: MarketSummary[] }
  | { type: "market"; market: MarketSummary }
  | { type: "book"; book: BookView }
  | { type: "trade"; trade: Trade }
  | { type: "game_over" };

// ---- game ----------------------------------------------------------------

export type TierId = "easy" | "medium" | "hard";

export interface Opponent {
  name: string;
  hint: string;
  count: number;
}

export interface GameState {
  status: "idle" | "running" | "ended";
  tier: TierId;
  market: string;
  outcome: OutcomeId;
  player: string;
  started_ms: number;
  duration_secs: number;
  remaining_secs: number;
  tick_ms: number;
  starting_equity: number;
  player_equity: number;
  pnl: number;
  opponents: Opponent[];
}

export interface Scorecard {
  starting_equity: number;
  final_equity: number;
  pnl: number;
  return_pct: number;
  sharpe: number;
  max_drawdown: number;
  trades: number;
  ticks: number;
}

export interface ScorecardResponse {
  status: "idle" | "running" | "ended";
  tier?: TierId;
  fair_value?: number | null;
  scorecard?: Scorecard;
}

export interface LeaderboardEntry {
  tier: TierId;
  return_pct: number;
  pnl: number;
  sharpe: number;
  at: number;
}

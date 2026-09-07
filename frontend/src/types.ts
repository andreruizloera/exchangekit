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
  status: "open" | "resolved";
  resolved_outcome: OutcomeId | null;
  resolved_at: number | null;
  collateral: number;
}

export interface Payout {
  account: string;
  winning_shares: number;
  losing_shares: number;
  paid: number;
}

export interface Settlement {
  market: string;
  outcome: OutcomeId;
  resolved_at: number;
  payouts: Payout[];
  total_paid: number;
  winning_shares: number;
  losing_shares: number;
  orders_voided: number;
  cash_released: number;
  shares_released: number;
  collateral: number;
  unbacked_cash: number;
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
  // "voided" means the market resolved underneath the order, which is not
  // the same as its owner cancelling it.
  status: "open" | "filled" | "cancelled" | "voided";
  created_at: number;
}

// Where the shares in a trade came from. "match" is an ordinary cross
// inside one outcome's book. "mint" and "burn" are complementary crosses
// between the two books: on a mint the seller bought the other outcome and
// never held this one, and on a burn the buyer sold the other outcome and
// never received this one.
export type TradeKind = "match" | "mint" | "burn";

export interface Trade {
  id: number;
  market: string;
  // The outcome the trade is priced in: the taker's side of it.
  outcome: OutcomeId;
  price: number;
  quantity: number;
  taker_side: SideId;
  buyer: string;
  seller: string;
  kind: TradeKind;
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
  | { type: "resolution"; settlement: Settlement }
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

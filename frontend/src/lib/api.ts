import type {
  Balance,
  BookView,
  GameState,
  MarketSummary,
  Order,
  OutcomeId,
  PlaceResult,
  PositionRow,
  ScorecardResponse,
  SideId,
  TierId,
  Trade,
} from "../types";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(path, init);
  if (!resp.ok) {
    let message = `${resp.status}`;
    try {
      const body = (await resp.json()) as { error?: string };
      if (body.error) message = body.error;
    } catch {
      // non-JSON error body; keep the status code
    }
    throw new Error(message);
  }
  return (await resp.json()) as T;
}

export const api = {
  markets: () => request<MarketSummary[]>("/api/markets"),
  book: (market: string, outcome: OutcomeId, depth = 12) =>
    request<BookView>(`/api/markets/${market}/book?outcome=${outcome}&depth=${depth}`),
  trades: (market: string, limit = 60) =>
    request<Trade[]>(`/api/markets/${market}/trades?limit=${limit}`),
  balance: (account: string) => request<Balance>(`/api/accounts/${account}`),
  positions: (account: string) => request<PositionRow[]>(`/api/accounts/${account}/positions`),
  openOrders: (account: string) => request<Order[]>(`/api/accounts/${account}/orders`),
  placeOrder: (body: {
    account: string;
    market: string;
    outcome: OutcomeId;
    side: SideId;
    price: number;
    quantity: number;
    // Omitted means "gtc": fill what crosses, rest the remainder.
    time_in_force?: "gtc" | "ioc" | "fok" | "post_only";
  }) =>
    request<PlaceResult>("/api/orders", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    }),
  cancelOrder: (id: number, account: string) =>
    request<Order>(`/api/orders/${id}?account=${encodeURIComponent(account)}`, {
      method: "DELETE",
    }),
  gameStart: (tier: TierId, durationSecs: number) =>
    request<GameState>("/api/game/start", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ tier, duration_secs: durationSecs }),
    }),
  gameState: () => request<GameState>("/api/game"),
  gameScorecard: () => request<ScorecardResponse>("/api/game/scorecard"),
};

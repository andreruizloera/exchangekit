import { useCallback, useEffect, useRef, useState } from "react";

import type {
  Balance,
  BookView,
  GameState,
  Order,
  PlaceResult,
  PositionRow,
  Scorecard,
  SideId,
  TierId,
  Trade,
  WsEvent,
} from "../types";
import { api } from "./api";
import { addLeaderboardEntry } from "./leaderboard";

export type Phase = "menu" | "playing" | "ended";

const PLAYER = "player";

export interface GameHook {
  phase: Phase;
  game: GameState | null;
  scorecard: Scorecard | null;
  fairValue: number | null;
  book: BookView | null;
  trades: Trade[];
  balance: Balance | null;
  positions: PositionRow[];
  openOrders: Order[];
  connected: boolean;
  start: (tier: TierId, durationSecs: number) => Promise<void>;
  placeOrder: (side: SideId, price: number, quantity: number) => Promise<PlaceResult>;
  cancelOrder: (id: number) => Promise<void>;
  again: () => void;
}

export function useGame(): GameHook {
  const [phase, setPhase] = useState<Phase>("menu");
  const [game, setGame] = useState<GameState | null>(null);
  const [scorecard, setScorecard] = useState<Scorecard | null>(null);
  const [fairValue, setFairValue] = useState<number | null>(null);
  const [book, setBook] = useState<BookView | null>(null);
  const [trades, setTrades] = useState<Trade[]>([]);
  const [balance, setBalance] = useState<Balance | null>(null);
  const [positions, setPositions] = useState<PositionRow[]>([]);
  const [openOrders, setOpenOrders] = useState<Order[]>([]);
  const [connected, setConnected] = useState(false);

  const marketRef = useRef<string | null>(null);
  const phaseRef = useRef<Phase>("menu");
  phaseRef.current = phase;

  const refreshAccount = useCallback(async () => {
    const market = marketRef.current;
    if (!market) return;
    const [bal, pos, orders] = await Promise.all([
      api.balance(PLAYER),
      api.positions(PLAYER),
      api.openOrders(PLAYER),
    ]);
    setBalance(bal);
    setPositions(pos.filter((p) => p.market === market && p.outcome === "YES"));
    setOpenOrders(orders.filter((o) => o.market === market));
  }, []);

  const refreshMarket = useCallback(async (market: string) => {
    const [yesBook, recent] = await Promise.all([api.book(market, "YES"), api.trades(market)]);
    setBook(yesBook);
    setTrades(recent);
  }, []);

  // One long-lived WebSocket for the whole session; it filters to the
  // current round's market.
  useEffect(() => {
    const proto = location.protocol === "https:" ? "wss" : "ws";
    let ws: WebSocket | null = null;
    let closed = false;
    let retry: ReturnType<typeof setTimeout> | null = null;

    const connect = () => {
      ws = new WebSocket(`${proto}://${location.host}/ws`);
      ws.onopen = () => setConnected(true);
      ws.onclose = () => {
        setConnected(false);
        if (!closed) retry = setTimeout(connect, 2000);
      };
      ws.onmessage = (e: MessageEvent<string>) => {
        const event = JSON.parse(e.data) as WsEvent;
        const market = marketRef.current;
        if (!market) return;
        switch (event.type) {
          case "book":
            if (event.book.market === market && event.book.outcome === "YES") {
              setBook(event.book);
            }
            break;
          case "trade":
            if (event.trade.market === market) {
              setTrades((ts) => [event.trade, ...ts].slice(0, 80));
              if (event.trade.buyer === PLAYER || event.trade.seller === PLAYER) {
                refreshAccount().catch(() => undefined);
              }
            }
            break;
        }
      };
    };
    connect();
    return () => {
      closed = true;
      if (retry) clearTimeout(retry);
      ws?.close();
    };
  }, [refreshAccount]);

  // Poll the round state while playing: the clock, live equity, and the
  // transition to the scorecard.
  useEffect(() => {
    if (phase !== "playing") return;
    let stop = false;
    const tick = async () => {
      try {
        const state = await api.gameState();
        if (stop) return;
        setGame(state);
        if (state.status === "ended") {
          const card = await api.gameScorecard();
          if (card.scorecard) {
            setScorecard(card.scorecard);
            setFairValue(card.fair_value ?? null);
            addLeaderboardEntry({
              tier: state.tier,
              return_pct: card.scorecard.return_pct,
              pnl: card.scorecard.pnl,
              sharpe: card.scorecard.sharpe,
              at: Date.now(),
            });
          }
          setPhase("ended");
        }
      } catch {
        // transient; the next poll retries
      }
    };
    const id = setInterval(() => void tick(), 500);
    void tick();
    return () => {
      stop = true;
      clearInterval(id);
    };
  }, [phase]);

  const start = useCallback(
    async (tier: TierId, durationSecs: number) => {
      const state = await api.gameStart(tier, durationSecs);
      marketRef.current = state.market;
      setGame(state);
      setScorecard(null);
      setFairValue(null);
      setTrades([]);
      setBook(null);
      await refreshMarket(state.market);
      await refreshAccount();
      setPhase("playing");
    },
    [refreshMarket, refreshAccount],
  );

  const placeOrder = useCallback(
    async (side: SideId, price: number, quantity: number): Promise<PlaceResult> => {
      const market = marketRef.current;
      if (!market) throw new Error("no round in progress");
      const result = await api.placeOrder({
        account: PLAYER,
        market,
        outcome: "YES",
        side,
        price,
        quantity,
      });
      await refreshAccount();
      return result;
    },
    [refreshAccount],
  );

  const cancelOrder = useCallback(
    async (id: number) => {
      await api.cancelOrder(id, PLAYER);
      await refreshAccount();
    },
    [refreshAccount],
  );

  const again = useCallback(() => {
    setPhase("menu");
    setGame(null);
    setScorecard(null);
    setFairValue(null);
  }, []);

  return {
    phase,
    game,
    scorecard,
    fairValue,
    book,
    trades,
    balance,
    positions,
    openOrders,
    connected,
    start,
    placeOrder,
    cancelOrder,
    again,
  };
}

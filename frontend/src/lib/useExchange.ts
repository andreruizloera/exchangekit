import { useCallback, useEffect, useRef, useState } from "react";

import type {
  Balance,
  BookView,
  MarketSummary,
  Order,
  OutcomeId,
  PlaceResult,
  PositionRow,
  SideId,
  Trade,
  WsEvent,
} from "../types";
import { api } from "./api";

export interface ExchangeState {
  markets: MarketSummary[];
  selected: string | null;
  select: (id: string) => void;
  books: Record<OutcomeId, BookView | null>;
  trades: Trade[];
  balance: Balance | null;
  positions: PositionRow[];
  openOrders: Order[];
  connected: boolean;
  placeOrder: (
    outcome: OutcomeId,
    side: SideId,
    price: number,
    quantity: number,
  ) => Promise<PlaceResult>;
  cancelOrder: (id: number) => Promise<void>;
}

export function useExchange(account: string): ExchangeState {
  const [markets, setMarkets] = useState<MarketSummary[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [books, setBooks] = useState<Record<OutcomeId, BookView | null>>({
    YES: null,
    NO: null,
  });
  const [trades, setTrades] = useState<Trade[]>([]);
  const [balance, setBalance] = useState<Balance | null>(null);
  const [positions, setPositions] = useState<PositionRow[]>([]);
  const [openOrders, setOpenOrders] = useState<Order[]>([]);
  const [connected, setConnected] = useState(false);

  // Refs so the long-lived WebSocket handler sees current values.
  const selectedRef = useRef(selected);
  selectedRef.current = selected;
  const accountRef = useRef(account);
  accountRef.current = account;

  const refreshAccount = useCallback(async () => {
    const who = accountRef.current;
    const [bal, pos, orders] = await Promise.all([
      api.balance(who),
      api.positions(who),
      api.openOrders(who),
    ]);
    setBalance(bal);
    setPositions(pos);
    setOpenOrders(orders);
  }, []);

  const refreshMarket = useCallback(async (id: string) => {
    const [yes, no, recent] = await Promise.all([
      api.book(id, "YES"),
      api.book(id, "NO"),
      api.trades(id),
    ]);
    setBooks({ YES: yes, NO: no });
    setTrades(recent);
  }, []);

  // Initial load.
  useEffect(() => {
    api
      .markets()
      .then((ms) => {
        setMarkets(ms);
        setSelected((cur) => cur ?? ms[0]?.id ?? null);
      })
      .catch(() => setMarkets([]));
  }, []);

  useEffect(() => {
    refreshAccount().catch(() => undefined);
  }, [account, refreshAccount]);

  useEffect(() => {
    if (selected) refreshMarket(selected).catch(() => undefined);
  }, [selected, refreshMarket]);

  // Live updates over the gateway's WebSocket fanout.
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
        switch (event.type) {
          case "hello":
            setMarkets(event.markets);
            setSelected((cur) => cur ?? event.markets[0]?.id ?? null);
            break;
          case "market":
            setMarkets((ms) => ms.map((m) => (m.id === event.market.id ? event.market : m)));
            break;
          case "book":
            if (event.book.market === selectedRef.current) {
              setBooks((b) => ({ ...b, [event.book.outcome]: event.book }));
            }
            break;
          case "trade":
            if (event.trade.market === selectedRef.current) {
              setTrades((ts) => [event.trade, ...ts].slice(0, 60));
            }
            if (
              event.trade.buyer === accountRef.current ||
              event.trade.seller === accountRef.current
            ) {
              refreshAccount().catch(() => undefined);
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

  const placeOrder = useCallback(
    async (outcome: OutcomeId, side: SideId, price: number, quantity: number) => {
      const market = selectedRef.current;
      if (!market) throw new Error("no market selected");
      const result = await api.placeOrder({
        account: accountRef.current,
        market,
        outcome,
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
      await api.cancelOrder(id, accountRef.current);
      await refreshAccount();
    },
    [refreshAccount],
  );

  return {
    markets,
    selected,
    select: setSelected,
    books,
    trades,
    balance,
    positions,
    openOrders,
    connected,
    placeOrder,
    cancelOrder,
  };
}

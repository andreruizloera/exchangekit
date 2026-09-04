import { useState } from "react";

import { Header } from "./components/Header";
import { MarketList } from "./components/MarketList";
import { OrderBook } from "./components/OrderBook";
import { OrderTicket } from "./components/OrderTicket";
import { Portfolio } from "./components/Portfolio";
import { PriceChart } from "./components/PriceChart";
import { TradeTape } from "./components/TradeTape";
import { formatPrice, formatQty } from "./lib/format";
import { useExchange } from "./lib/useExchange";
import type { OutcomeId } from "./types";

export default function App() {
  const [account, setAccount] = useState("demo");
  const [outcome, setOutcome] = useState<OutcomeId>("YES");
  const [ticketPrice, setTicketPrice] = useState(50);
  const ex = useExchange(account);

  const market = ex.markets.find((m) => m.id === ex.selected) ?? null;
  const book = ex.books[outcome];

  return (
    <div className="app">
      <Header
        account={account}
        onAccountChange={setAccount}
        balance={ex.balance}
        connected={ex.connected}
      />
      <div className="layout">
        <MarketList markets={ex.markets} selected={ex.selected} onSelect={ex.select} />

        <main className="center">
          {market && (
            <section className="panel market-head">
              <div>
                <h1>{market.question}</h1>
                <p className="market-desc">{market.description}</p>
              </div>
              <div className="market-stats">
                <div className="stat">
                  <span className="stat-label">YES</span>
                  <span className="stat-value text-yes">{formatPrice(market.yes_price)}</span>
                </div>
                <div className="stat">
                  <span className="stat-label">NO</span>
                  <span className="stat-value text-no">{formatPrice(market.no_price)}</span>
                </div>
                <div className="stat">
                  <span className="stat-label">volume</span>
                  <span className="stat-value">{formatQty(market.volume)}</span>
                </div>
              </div>
            </section>
          )}
          <PriceChart trades={ex.trades} />
          <div className="center-grid">
            <OrderBook
              book={book}
              outcome={outcome}
              onOutcomeChange={setOutcome}
              onPriceClick={setTicketPrice}
            />
            <TradeTape trades={ex.trades} />
          </div>
        </main>

        <aside className="side">
          <OrderTicket
            outcome={outcome}
            onOutcomeChange={setOutcome}
            price={ticketPrice}
            onPriceChange={setTicketPrice}
            book={book}
            onSubmit={ex.placeOrder}
          />
          <Portfolio
            positions={ex.positions}
            openOrders={ex.openOrders}
            onCancel={(id) => void ex.cancelOrder(id)}
          />
        </aside>
      </div>
      <footer className="footer">
        exchangekit is a play-money exchange simulator for education and research. No real money,
        custody, or payments.
      </footer>
    </div>
  );
}

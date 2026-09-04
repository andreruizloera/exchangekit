import { useState } from "react";

import { GameHud } from "./components/GameHud";
import { Opponents } from "./components/Opponents";
import { OrderBook } from "./components/OrderBook";
import { OrderTicket } from "./components/OrderTicket";
import { Portfolio } from "./components/Portfolio";
import { PriceChart } from "./components/PriceChart";
import { Scorecard } from "./components/Scorecard";
import { StartScreen } from "./components/StartScreen";
import { TradeTape } from "./components/TradeTape";
import { useGame } from "./lib/useGame";

const DISCLAIMER =
  "exchangekit is a play-money simulator for education and research. No real money, custody, or payments.";

export default function App() {
  const g = useGame();
  const [ticketPrice, setTicketPrice] = useState(50);
  const [starting, setStarting] = useState(false);

  if (g.phase === "menu") {
    return (
      <StartScreen
        starting={starting}
        onStart={(tier, secs) => {
          setStarting(true);
          void g
            .start(tier, secs)
            .catch(() => undefined)
            .finally(() => setStarting(false));
        }}
      />
    );
  }

  const book = g.book;
  const bestBid = book?.bids[0]?.price;
  const bestAsk = book?.asks[0]?.price;
  const mid = bestBid !== undefined && bestAsk !== undefined ? (bestBid + bestAsk) / 2 : null;
  const spread = bestBid !== undefined && bestAsk !== undefined ? bestAsk - bestBid : null;

  return (
    <div className="app">
      {g.game && (
        <GameHud
          game={g.game}
          balance={g.balance}
          positions={g.positions}
          connected={g.connected}
          onQuit={g.again}
        />
      )}

      <div className="layout">
        <aside className="side">
          {g.game && <Opponents opponents={g.game.opponents} />}
          <Portfolio
            positions={g.positions}
            openOrders={g.openOrders}
            onCancel={(id) => void g.cancelOrder(id)}
          />
        </aside>

        <main className="center">
          <section className="panel market-head">
            <div>
              <h1>Synthetic asset</h1>
              <p className="market-desc">
                A hidden fair value drives the round. Read the book and the tape, infer where it is,
                and trade against the bots.
              </p>
            </div>
            <div className="market-stats">
              <div className="stat">
                <span className="stat-label">bid</span>
                <span className="stat-value text-yes">
                  {bestBid !== undefined ? `${bestBid}c` : "--"}
                </span>
              </div>
              <div className="stat">
                <span className="stat-label">mark</span>
                <span className="stat-value">{mid !== null ? `${mid.toFixed(1)}c` : "--"}</span>
              </div>
              <div className="stat">
                <span className="stat-label">ask</span>
                <span className="stat-value text-no">
                  {bestAsk !== undefined ? `${bestAsk}c` : "--"}
                </span>
              </div>
              <div className="stat">
                <span className="stat-label">spread</span>
                <span className="stat-value">{spread !== null ? `${spread}c` : "--"}</span>
              </div>
            </div>
          </section>

          <PriceChart trades={g.trades} player="player" />

          <div className="center-grid">
            <OrderBook
              book={book}
              outcome="YES"
              onOutcomeChange={() => undefined}
              onPriceClick={setTicketPrice}
              singleOutcome
            />
            <TradeTape trades={g.trades} />
          </div>
        </main>

        <aside className="side">
          <OrderTicket
            outcome="YES"
            onOutcomeChange={() => undefined}
            price={ticketPrice}
            onPriceChange={setTicketPrice}
            book={book}
            onSubmit={(_outcome, side, price, quantity) => g.placeOrder(side, price, quantity)}
            singleOutcome
          />
        </aside>
      </div>

      <footer className="footer">{DISCLAIMER}</footer>

      {g.phase === "ended" && g.scorecard && g.game && (
        <Scorecard
          tier={g.game.tier}
          card={g.scorecard}
          fairValue={g.fairValue}
          onAgain={g.again}
        />
      )}
    </div>
  );
}

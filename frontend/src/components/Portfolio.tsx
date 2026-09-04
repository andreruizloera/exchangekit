import type { Order, PositionRow } from "../types";
import { formatQty } from "../lib/format";

interface Props {
  positions: PositionRow[];
  openOrders: Order[];
  onCancel: (id: number) => void;
}

export function Portfolio({ positions, openOrders, onCancel }: Props) {
  return (
    <>
      <section className="panel">
        <div className="panel-title">Positions</div>
        {positions.length === 0 && <div className="empty">no positions</div>}
        {positions.length > 0 && (
          <table className="table">
            <thead>
              <tr>
                <th>market</th>
                <th>outcome</th>
                <th className="num">shares</th>
                <th className="num">locked</th>
              </tr>
            </thead>
            <tbody>
              {positions.map((p) => (
                <tr key={`${p.market}-${p.outcome}`}>
                  <td>{p.market}</td>
                  <td className={p.outcome === "YES" ? "text-yes" : "text-no"}>{p.outcome}</td>
                  <td className="num">{formatQty(p.quantity)}</td>
                  <td className="num">{formatQty(p.locked)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="panel">
        <div className="panel-title">Open orders</div>
        {openOrders.length === 0 && <div className="empty">no open orders</div>}
        {openOrders.length > 0 && (
          <table className="table">
            <thead>
              <tr>
                <th>market</th>
                <th>order</th>
                <th className="num">left</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {openOrders.map((o) => (
                <tr key={o.id}>
                  <td>{o.market}</td>
                  <td>
                    <span className={o.side === "BUY" ? "text-yes" : "text-no"}>
                      {o.side.toLowerCase()}
                    </span>{" "}
                    {o.outcome} @ {o.price}c
                  </td>
                  <td className="num">{formatQty(o.quantity - o.filled)}</td>
                  <td className="num">
                    <button className="cancel" onClick={() => onCancel(o.id)}>
                      cancel
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </>
  );
}

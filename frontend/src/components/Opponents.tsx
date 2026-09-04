import type { Opponent } from "../types";

interface Props {
  opponents: Opponent[];
}

export function Opponents({ opponents }: Props) {
  return (
    <section className="panel">
      <div className="panel-title">Opponents</div>
      {opponents.length === 0 && <div className="empty">no bots</div>}
      {opponents.map((o) => (
        <div key={o.name} className="opponent">
          <div className="opponent-head">
            <span className="opponent-name">{o.name}</span>
            {o.count > 1 && <span className="opponent-count">x{o.count}</span>}
          </div>
          <div className="opponent-hint">{o.hint}</div>
        </div>
      ))}
    </section>
  );
}

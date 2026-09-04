import type { Balance } from "../types";
import { formatCash } from "../lib/format";

const ACCOUNTS = ["demo", "alice", "bob"];

interface Props {
  account: string;
  onAccountChange: (a: string) => void;
  balance: Balance | null;
  connected: boolean;
}

export function Header({ account, onAccountChange, balance, connected }: Props) {
  return (
    <header className="header">
      <div className="brand">
        <span className="brand-mark">EK</span>
        <span className="brand-name">exchangekit</span>
        <span className="badge">play money</span>
      </div>
      <div className="header-right">
        <span className={connected ? "ws-dot ws-on" : "ws-dot ws-off"} title="WebSocket status" />
        <span className="header-label">{connected ? "live" : "offline"}</span>
        {balance && (
          <>
            <span className="header-label">available</span>
            <span className="header-value">{formatCash(balance.available)}</span>
            <span className="header-label">total</span>
            <span className="header-value">{formatCash(balance.balance)}</span>
          </>
        )}
        <select
          className="account-select"
          value={account}
          onChange={(e) => onAccountChange(e.target.value)}
        >
          {ACCOUNTS.map((a) => (
            <option key={a} value={a}>
              {a}
            </option>
          ))}
        </select>
      </div>
    </header>
  );
}

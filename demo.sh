#!/usr/bin/env bash
# Demo: browse markets, read the book, place a marketable buy, and show
# the resulting trade, balance, and position. Needs a running gateway
# (docker compose up, or cargo run -p exchangekit-gateway).
set -euo pipefail

BASE="${EXCHANGEKIT_URL:-http://localhost:8080}"

if ! curl -fsS "$BASE/api/health" >/dev/null 2>&1; then
    echo "error: no gateway at $BASE" >&2
    echo "start one with: docker compose up  (or: cargo run -p exchangekit-gateway)" >&2
    exit 1
fi

get() { curl -fsS "$BASE$1"; }

echo "== markets =="
get /api/markets | python3 -c '
import json, sys
for m in json.load(sys.stdin):
    print("%-13s YES %2dc  NO %2dc  vol %4d  %s"
          % (m["id"], m["yes_price"], m["no_price"], m["volume"], m["question"]))
'

echo
echo "== order book: btc-100k YES (top 3) =="
get "/api/markets/btc-100k/book?outcome=YES&depth=3" | python3 -c '
import json, sys
b = json.load(sys.stdin)
for l in reversed(b["asks"]):
    print("  ask %2dc  x %d" % (l["price"], l["quantity"]))
print("  ----")
for l in b["bids"]:
    print("  bid %2dc  x %d" % (l["price"], l["quantity"]))
'

echo
echo "== demo buys 10 YES at the best ask =="
ASK=$(get "/api/markets/btc-100k/book?outcome=YES&depth=1" | python3 -c 'import json,sys; print(json.load(sys.stdin)["asks"][0]["price"])')
curl -fsS -X POST "$BASE/api/orders" \
    -H 'content-type: application/json' \
    -d "{\"account\":\"demo\",\"market\":\"btc-100k\",\"outcome\":\"YES\",\"side\":\"BUY\",\"price\":$ASK,\"quantity\":10}" |
    python3 -c '
import json, sys
r = json.load(sys.stdin)
o = r["order"]
print("  order %d: %s, filled %d/%d" % (o["id"], o["status"], o["filled"], o["quantity"]))
for t in r["trades"]:
    print("  trade: %d YES @ %dc (%s bought from %s)"
          % (t["quantity"], t["price"], t["buyer"], t["seller"]))
'

echo
echo "== demo account after the trade =="
get /api/accounts/demo | python3 -c '
import json, sys
a = json.load(sys.stdin)
print("  balance $%s (available $%s)"
      % (format(a["balance"] / 100, ",.2f"), format(a["available"] / 100, ",.2f")))
'
get /api/accounts/demo/positions | python3 -c '
import json, sys
for p in json.load(sys.stdin):
    if p["market"] == "btc-100k" and p["outcome"] == "YES":
        print("  position: %d %s shares in %s" % (p["quantity"], p["outcome"], p["market"]))
'

#!/usr/bin/env bash
# Demo: browse markets, read the book, place a marketable buy, show the
# resulting trade, balance, and position, then settle a market and watch
# it pay out. Needs a running gateway (docker compose up, or
# cargo run -p exchangekit-gateway).
#
# Every data line this prints is checked at the end against the output
# pasted in the README, so the docs cannot drift from the tool without
# this script exiting nonzero.
set -euo pipefail

BASE="${EXCHANGEKIT_URL:-http://localhost:8080}"
OUT="$(mktemp)"
trap 'rm -f "$OUT"' EXIT

if ! curl -fsS "$BASE/api/health" >/dev/null 2>&1; then
    echo "error: no gateway at $BASE" >&2
    echo "start one with: docker compose up  (or: cargo run -p exchangekit-gateway)" >&2
    exit 1
fi

get() { curl -fsS "$BASE$1"; }
say() { tee -a "$OUT"; }

echo "== markets ==" | say
get /api/markets | python3 -c '
import json, sys
for m in json.load(sys.stdin):
    print("%-13s YES %2dc  NO %2dc  vol %4d  %s"
          % (m["id"], m["yes_price"], m["no_price"], m["volume"], m["question"]))
' | say

echo | say
echo "== order book: btc-100k YES (top 3) ==" | say
get "/api/markets/btc-100k/book?outcome=YES&depth=3" | python3 -c '
import json, sys
b = json.load(sys.stdin)
for l in reversed(b["asks"]):
    print("  ask %2dc  x %d" % (l["price"], l["quantity"]))
print("  ----")
for l in b["bids"]:
    print("  bid %2dc  x %d" % (l["price"], l["quantity"]))
' | say

echo | say
echo "== demo buys 10 YES at the best ask ==" | say
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
' | say

echo | say
echo "== demo account after the trade ==" | say
get /api/accounts/demo | python3 -c '
import json, sys
a = json.load(sys.stdin)
print("  balance $%s (available $%s)"
      % (format(a["balance"] / 100, ",.2f"), format(a["available"] / 100, ",.2f")))
' | say
get /api/accounts/demo/positions | python3 -c '
import json, sys
for p in json.load(sys.stdin):
    if p["market"] == "btc-100k" and p["outcome"] == "YES":
        print("  position: %d %s shares in %s" % (p["quantity"], p["outcome"], p["market"]))
' | say

# ---- settlement ----------------------------------------------------------
#
# A binary contract is only worth what it settles for. mars-2030 is the
# market the demo picks because the demo never trades it, so its numbers
# are the same on every fresh gateway. Resolution cannot be undone, so if
# it has already happened this run reads the recorded settlement instead.

MARS_STATUS=$(get /api/markets/mars-2030 | python3 -c 'import json,sys; print(json.load(sys.stdin)["status"])')

echo | say
echo "== mars-2030 before resolution ==" | say
get /api/markets/mars-2030 | python3 -c '
import json, sys
m = json.load(sys.stdin)
print("  status:     %s" % m["status"])
print("  collateral: $%s backing its outstanding shares"
      % format(m["collateral"] / 100, ",.2f"))
' | say
get /api/accounts/demo/positions | python3 -c '
import json, sys
held = {p["outcome"]: p["quantity"] for p in json.load(sys.stdin) if p["market"] == "mars-2030"}
print("  demo holds: %d YES, %d NO" % (held.get("YES", 0), held.get("NO", 0)))
' | say

if [ "$MARS_STATUS" = "resolved" ]; then
    echo | say
    echo "  (already resolved; a market settles once. Restart the gateway to run this live.)" | say
    SETTLEMENT=$(get /api/markets/mars-2030/settlement)
else
    echo | say
    echo "== resolve mars-2030 to NO ==" | say
    SETTLEMENT=$(curl -fsS -X POST "$BASE/api/markets/mars-2030/resolve" \
        -H 'content-type: application/json' -d '{"outcome":"NO"}')
fi

printf '%s' "$SETTLEMENT" | python3 -c '
import json, sys
s = json.load(sys.stdin)
d = lambda c: "$" + format(c / 100, ",.2f")
print("  winner:        %s" % s["outcome"])
print("  paid out:      %s to %d account(s)" % (d(s["total_paid"]), len(s["payouts"])))
print("  shares:        %s winning, %s losing"
      % (format(s["winning_shares"], ","), format(s["losing_shares"], ",")))
print("  collateral:    %s held by the market" % d(s["collateral"]))
print("  unbacked cash: %s" % d(s["unbacked_cash"]))
print("  voided:        %d resting orders, releasing %s and %s shares of escrow"
      % (s["orders_voided"], d(s["cash_released"]), format(s["shares_released"], ",")))
for p in s["payouts"]:
    print("    %-12s %6s winning x $1.00 = %s"
          % (p["account"], format(p["winning_shares"], ","), d(p["paid"])))
' | say

echo | say
echo "== the market is closed ==" | say
BOOK_LEVELS=$(get "/api/markets/mars-2030/book?outcome=NO&depth=20" | python3 -c '
import json, sys
b = json.load(sys.stdin)
print(len(b["bids"]) + len(b["asks"]))
')
echo "  book: $BOOK_LEVELS price levels left on either side" | say

ORDER_ERR=$(curl -sS -o - -w '\n%{http_code}' -X POST "$BASE/api/orders" \
    -H 'content-type: application/json' \
    -d '{"account":"demo","market":"mars-2030","outcome":"NO","side":"BUY","price":50,"quantity":1}')
echo "  new order:     $(echo "$ORDER_ERR" | tail -n1) $(echo "$ORDER_ERR" | head -n1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["error"])')" | say

AGAIN=$(curl -sS -o - -w '\n%{http_code}' -X POST "$BASE/api/markets/mars-2030/resolve" \
    -H 'content-type: application/json' -d '{"outcome":"YES"}')
echo "  resolve again: $(echo "$AGAIN" | tail -n1) $(echo "$AGAIN" | head -n1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["error"])')" | say

# ---- check the README against what actually ran --------------------------
#
# The README pastes a run against a FRESH gateway. Some of these lines
# advance if you run the demo twice against the same one: it buys 10 more
# shares every time, and mars-2030 only resolves once. mars-2030 being
# open when this script started is exactly the test for "this gateway has
# not run the demo before", so the lines that move are checked only then.
# Everything below that is a fact about the tool, not about how many times
# it has been run, and is checked on every run.

FAIL=0
check() {
    if ! grep -Fq -- "$1" "$OUT"; then
        echo "demo check failed, this line is in the README but not in the output:" >&2
        echo "  $1" >&2
        FAIL=1
    fi
}

if [ "$MARS_STATUS" = "open" ]; then
    check "btc-100k      YES 63c  NO 37c  vol  105  Will Bitcoin close above \$100,000 this year?"
    check "fed-cut-dec   YES 44c  NO 56c  vol  157  Will the Fed cut rates at its December meeting?"
    check "mars-2030     YES  8c  NO 92c  vol  209  Will humans land on Mars before 2030?"
    check "  ask 66c  x 90"
    check "  ask 65c  x 260"
    check "  ask 64c  x 180"
    check "  bid 62c  x 180"
    check "  bid 61c  x 260"
    check "  bid 60c  x 120"
    check "  order 79: filled, filled 10/10"
    check "  trade: 10 YES @ 64c (demo bought from marketmaker)"
    check "  balance \$10,451.30 (available \$10,451.30)"
    check "  position: 540 YES shares in btc-100k"
    check "  status:     open"
    check "  collateral: \$101,500.00 backing its outstanding shares"
    check "  demo holds: 556 YES, 500 NO"
fi

check "  winner:        NO"
check "  paid out:      \$101,500.00 to 4 account(s)"
check "  shares:        101,500 winning, 101,500 losing"
check "  collateral:    \$101,500.00 held by the market"
check "  unbacked cash: \$0.00"
check "  voided:        18 resting orders, releasing \$1,624.56 and 3,634 shares of escrow"
check "    alice           546 winning x \$1.00 = \$546.00"
check "    bob             500 winning x \$1.00 = \$500.00"
check "    demo            500 winning x \$1.00 = \$500.00"
check "    marketmaker  99,954 winning x \$1.00 = \$99,954.00"
check "  book: 0 price levels left on either side"
check "  new order:     400 market mars-2030 has resolved and no longer trades"
check "  resolve again: 409 market mars-2030 has already resolved"

if [ "$FAIL" -ne 0 ]; then
    echo >&2
    echo "The README pastes output this run did not produce. Update one or the other." >&2
    exit 1
fi

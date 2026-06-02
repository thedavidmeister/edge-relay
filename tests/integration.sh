#!/usr/bin/env bash
# Integration test: run the worker in workerd (via `wrangler dev`) against a
# local stub of the Lovense + Telegram HTTP APIs, and assert both the route
# wiring AND the outbound request shape — the latter is the core behaviour
# (does /vibrate actually POST the right Lovense command?). glue.rs is wasm-only,
# so host unit tests can't reach any of this.
#
# Run inside the nix dev shell:  nix develop --command tests/integration.sh
set -uo pipefail
cd "$(dirname "$0")/.."

PORT=8787
STUB_PORT=8788
SECRET=testsecret
base="http://127.0.0.1:$PORT"
stub="http://127.0.0.1:$STUB_PORT"
REQ_LOG="$(mktemp)"
export REQ_LOG STUB_PORT

WPID=""
SPID=""
created_devvars=0
cleanup() {
  [ -n "$WPID" ] && kill "$WPID" 2>/dev/null
  [ -n "$SPID" ] && kill "$SPID" 2>/dev/null
  pkill -f workerd 2>/dev/null
  [ "$created_devvars" = 1 ] && rm -f .dev.vars
  rm -f "$REQ_LOG"
}
trap cleanup EXIT

# Local secrets + base-URL overrides so the worker calls our stub, not the real
# Lovense/Telegram. In `wrangler dev`, .dev.vars feeds both ctx.secret and
# ctx.var. Refuse to clobber a real one.
if [ -f .dev.vars ]; then echo "refusing to overwrite existing .dev.vars"; exit 1; fi
cat > .dev.vars <<EOF
TG_WEBHOOK_SECRET=$SECRET
LOVENSE_TOKEN=tok123
LOVENSE_SALT=salt
ALLOWED_USER_ID=42
BOT_TOKEN=bottoken
LOVENSE_API_BASE=$stub
TELEGRAM_API_BASE=$stub
EOF
created_devvars=1

echo "== starting stub API on :$STUB_PORT =="
python3 tests/stub_server.py &
SPID=$!

echo "== starting wrangler dev on :$PORT (first run builds wasm) =="
WRANGLER_SEND_METRICS=false wrangler dev --port "$PORT" --ip 127.0.0.1 >/tmp/wdev.log 2>&1 &
WPID=$!

ready=0
for _ in $(seq 1 120); do
  if curl -fsS -o /dev/null "$base/" 2>/dev/null; then ready=1; break; fi
  if ! kill -0 "$WPID" 2>/dev/null; then echo "wrangler dev exited early:"; tail -40 /tmp/wdev.log; exit 1; fi
  sleep 2
done
[ "$ready" = 1 ] || { echo "timed out waiting for wrangler dev:"; tail -40 /tmp/wdev.log; exit 1; }

fails=0
pass() { echo "ok   - $1"; }
fail() { echo "FAIL - $1"; fails=$((fails + 1)); }
expect_code() { # label expected method path [curl args...]
  local label=$1 exp=$2 method=$3 path=$4; shift 4
  local act; act=$(curl -s -o /dev/null -w '%{http_code}' -X "$method" "$base$path" "$@")
  [ "$act" = "$exp" ] && pass "$label ($act)" || fail "$label: expected $exp got $act"
}

echo "== route wiring + webhook-secret gate =="
expect_code "GET / health"                200 GET  /                 -H 'accept: */*'
[ "$(curl -s "$base/")" = "edge-relay: ok" ] && pass "GET / body" || fail "GET / body"
expect_code "POST /telegram no secret"    403 POST /telegram         -d '{}'
expect_code "POST /telegram wrong secret" 403 POST /telegram         -H 'X-Telegram-Bot-Api-Secret-Token: nope' -d '{}'
expect_code "POST /lovense-callback ok"   200 POST /lovense-callback -d '{"uid":"gf","toys":{}}'
expect_code "POST /lovense-callback junk" 200 POST /lovense-callback -d 'not json'

echo "== outbound: /vibrate 9 from the authorized user =="
vibe=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$base/telegram" \
  -H "X-Telegram-Bot-Api-Secret-Token: $SECRET" -H 'content-type: application/json' \
  -d '{"message":{"text":"/vibrate 9","from":{"id":42},"chat":{"id":5}}}')
[ "$vibe" = "200" ] && pass "/vibrate accepted ($vibe)" || fail "/vibrate: got $vibe"

# The worker awaits its outbound calls before responding, so the stub has them.
cmd=$(grep '/api/lan/command' "$REQ_LOG" || true)
if [ -n "$cmd" ]; then
  echo "$cmd" | grep -q 'Vibrate:9' && pass "Lovense command action = Vibrate:9" || fail "action wrong: $cmd"
  echo "$cmd" | grep -q 'tok123'    && pass "Lovense command carries dev token" || fail "token missing: $cmd"
  echo "$cmd" | grep -q 'gf'        && pass "Lovense command uid = gf" || fail "uid missing: $cmd"
else
  fail "no POST to /api/lan/command recorded"
fi
msg=$(grep 'sendMessage' "$REQ_LOG" || true)
echo "$msg" | grep -q 'Vibrating at 9' && pass "Telegram reply = ack text" || fail "telegram reply wrong: $msg"

echo "== /pair (Telegram, authorized) triggers getQrCode and replies the link =="
curl -s -o /dev/null -X POST "$base/telegram" \
  -H "X-Telegram-Bot-Api-Secret-Token: $SECRET" -H 'content-type: application/json' \
  -d '{"message":{"text":"/pair","from":{"id":42},"chat":{"id":5}}}'
grep -q '/api/lan/getQrCode' "$REQ_LOG" && pass "getQrCode was called" || fail "no getQrCode POST recorded"
grep 'sendMessage' "$REQ_LOG" | grep -q 'qrcode.png' && pass "reply contains the QR url" || fail "reply missing QR url"

echo "== the unauthenticated GET /pair route is gone =="
expect_code "GET /pair removed -> 404" 404 GET /pair

# Count Lovense command POSTs recorded so far (grep -c prints 0 / exits 1 when none).
commands() { grep -c '/api/lan/command' "$REQ_LOG" 2>/dev/null || true; }

echo "== safety: a stranger's command must NOT reach the toy =="
before=$(commands)
unauth=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$base/telegram" \
  -H "X-Telegram-Bot-Api-Secret-Token: $SECRET" -H 'content-type: application/json' \
  -d '{"message":{"text":"/vibrate 20","from":{"id":999},"chat":{"id":5}}}')
after=$(commands)
[ "$unauth" = "200" ] && pass "unauthorized request accepted + ignored ($unauth)" || fail "unauthorized: got $unauth"
[ "$before" = "$after" ] && pass "unauthorized command did NOT reach Lovense" || fail "unauthorized LEAKED a command ($before -> $after)"

echo "== /stop (safe-word) sends action=Stop =="
curl -s -o /dev/null -X POST "$base/telegram" \
  -H "X-Telegram-Bot-Api-Secret-Token: $SECRET" -H 'content-type: application/json' \
  -d '{"message":{"text":"/stop","from":{"id":42},"chat":{"id":5}}}'
grep '/api/lan/command' "$REQ_LOG" | grep -q 'Stop' && pass "/stop sent action=Stop" || fail "/stop did not send Stop"

echo "== invalid command replies usage, sends no command =="
before=$(commands)
curl -s -o /dev/null -X POST "$base/telegram" \
  -H "X-Telegram-Bot-Api-Secret-Token: $SECRET" -H 'content-type: application/json' \
  -d '{"message":{"text":"/vibrate abc","from":{"id":42},"chat":{"id":5}}}'
after=$(commands)
[ "$before" = "$after" ] && pass "invalid command sent no Lovense command" || fail "invalid command leaked a command"
grep 'sendMessage' "$REQ_LOG" | grep -q '0-20' && pass "invalid command got a usage reply" || fail "no usage reply for invalid command"

echo "== malformed update body is ignored (200, no command) =="
before=$(commands)
mal=$(curl -s -o /dev/null -w '%{http_code}' -X POST "$base/telegram" \
  -H "X-Telegram-Bot-Api-Secret-Token: $SECRET" -d 'not json at all')
after=$(commands)
[ "$mal" = "200" ] && pass "malformed body -> 200 ($mal)" || fail "malformed body: got $mal"
[ "$before" = "$after" ] && pass "malformed body sent no command" || fail "malformed body leaked a command"

echo
if [ "$fails" -eq 0 ]; then
  echo "integration: ALL PASS"
else
  echo "integration: $fails FAILED"
  echo "--- recorded stub requests ---"; cat "$REQ_LOG"
  exit 1
fi

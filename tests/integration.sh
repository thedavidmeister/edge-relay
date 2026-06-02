#!/usr/bin/env bash
# Integration test: run the worker in workerd (via `wrangler dev`) and exercise
# the HTTP routes that need no outbound calls. Covers route wiring and the
# Telegram webhook-secret gate — surface that host unit tests can't reach
# (glue.rs is wasm-only). Routes that call Lovense/Telegram (/pair, a valid
# /telegram command) are intentionally out of scope: they need network stubs.
#
# Run inside the nix dev shell:  nix develop --command tests/integration.sh
set -uo pipefail
cd "$(dirname "$0")/.."

PORT=8787
SECRET=testsecret
base="http://127.0.0.1:$PORT"

created_devvars=0
WPID=""
cleanup() {
  [ -n "$WPID" ] && kill "$WPID" 2>/dev/null
  pkill -f workerd 2>/dev/null
  [ "$created_devvars" = 1 ] && rm -f .dev.vars
}
trap cleanup EXIT

# wrangler dev reads secrets from .dev.vars; provide the webhook secret.
if [ ! -f .dev.vars ]; then
  echo "TG_WEBHOOK_SECRET=$SECRET" > .dev.vars
  created_devvars=1
fi

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
check_code() { # label expected method path [curl-args...]
  local label=$1 exp=$2 method=$3 path=$4; shift 4
  local act
  act=$(curl -s -o /dev/null -w '%{http_code}' -X "$method" "$base$path" "$@")
  if [ "$act" = "$exp" ]; then echo "ok   - $label ($act)"; else echo "FAIL - $label: expected $exp got $act"; fails=$((fails + 1)); fi
}
check_body() { # label expected path
  local label=$1 exp=$2 path=$3 act
  act=$(curl -s "$base$path")
  if [ "$act" = "$exp" ]; then echo "ok   - $label"; else echo "FAIL - $label: expected [$exp] got [$act]"; fails=$((fails + 1)); fi
}

echo "== assertions =="
check_code "GET / health"                 200 GET  /                 -H 'accept: */*'
check_body "GET / body"                   "edge-relay: ok" /
check_code "POST /telegram no secret"     403 POST /telegram         -d '{}'
check_code "POST /telegram wrong secret"  403 POST /telegram         -H 'X-Telegram-Bot-Api-Secret-Token: nope' -d '{}'
check_code "POST /lovense-callback valid" 200 POST /lovense-callback -H 'content-type: application/json' -d '{"uid":"gf","toys":{}}'
check_code "POST /lovense-callback junk"  200 POST /lovense-callback -d 'not json'

echo
if [ "$fails" -eq 0 ]; then
  echo "integration: ALL PASS"
else
  echo "integration: $fails FAILED"
  exit 1
fi

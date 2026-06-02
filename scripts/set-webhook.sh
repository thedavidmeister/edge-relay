#!/usr/bin/env bash
# Register / inspect / remove the Telegram webhook for edge-relay.
#
# Usage:
#   BOT_TOKEN=… TG_WEBHOOK_SECRET=… WORKER_URL=https://edge-relay.<sub>.workers.dev \
#     scripts/set-webhook.sh set
#   BOT_TOKEN=… scripts/set-webhook.sh info
#   BOT_TOKEN=… scripts/set-webhook.sh delete
set -euo pipefail

: "${BOT_TOKEN:?set BOT_TOKEN}"
api="https://api.telegram.org/bot${BOT_TOKEN}"

case "${1:-set}" in
  set)
    : "${WORKER_URL:?set WORKER_URL}"
    : "${TG_WEBHOOK_SECRET:?set TG_WEBHOOK_SECRET}"
    curl -fsS "${api}/setWebhook" \
      -d "url=${WORKER_URL%/}/telegram" \
      -d "secret_token=${TG_WEBHOOK_SECRET}" \
      -d 'allowed_updates=["message"]'
    ;;
  info)
    curl -fsS "${api}/getWebhookInfo"
    ;;
  delete)
    curl -fsS "${api}/deleteWebhook"
    ;;
  *)
    echo "usage: $0 {set|info|delete}" >&2
    exit 2
    ;;
esac
echo

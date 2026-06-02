# edge-relay

A small Cloudflare Worker (Rust → WASM) that relays Telegram webhooks to a
third-party device-control HTTP API (the [Lovense developer API](https://github.com/lovense/Standard_solutions)).

It runs on the free tier, is **always-warm** (no cold start in front of a stop
command), and is **stateless** — the target device is addressed by a fixed id
chosen at pairing time, so there's no database.

## Architecture

```
You ──/vibrate 15──▶ Telegram ──webhook──▶ edge-relay (Worker) ──HTTPS──▶ Lovense Cloud ──▶ device
                                                ▲
        device app ──scans QR once──▶ /pair ────┘  (callback → /lovense-callback)
```

Routes:

| Route                  | Method | Purpose                                            |
|------------------------|--------|----------------------------------------------------|
| `/`                    | GET    | Health check                                       |
| `/pair`                | GET    | Request a pairing QR code                          |
| `/lovense-callback`    | POST   | Pairing confirmation / device-online status        |
| `/telegram`            | POST   | Telegram webhook — incoming commands               |

## Setup

Prereqs: a [Lovense developer token](https://developer.lovense.com), a Telegram
bot token from [@BotFather](https://t.me/BotFather), and `wrangler`.

```sh
# secrets (never committed)
wrangler secret put BOT_TOKEN          # Telegram bot token
wrangler secret put TG_WEBHOOK_SECRET  # random string for webhook verification
wrangler secret put LOVENSE_TOKEN      # Lovense developer token
wrangler secret put ALLOWED_USER_ID    # your Telegram numeric user id

wrangler deploy
```

Then point Telegram at the Worker (uses `TG_WEBHOOK_SECRET` to reject forged calls):

```sh
curl "https://api.telegram.org/bot<BOT_TOKEN>/setWebhook" \
  -d "url=https://edge-relay.<subdomain>.workers.dev/telegram" \
  -d "secret_token=<TG_WEBHOOK_SECRET>"
```

Set the Lovense developer-dashboard callback URL to
`https://edge-relay.<subdomain>.workers.dev/lovense-callback`.

## Status

Skeleton: routes are stubbed. Implemented next: `/pair` (getQrCode), command
dispatch in `/telegram`, and a `/stop` safe-word. Only `ALLOWED_USER_ID` may
issue commands.

## License

MIT

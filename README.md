# edge-relay

[![CI](https://github.com/thedavidmeister/edge-relay/actions/workflows/ci.yml/badge.svg)](https://github.com/thedavidmeister/edge-relay/actions/workflows/ci.yml)

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

| Route               | Method | Purpose                                     |
|---------------------|--------|---------------------------------------------|
| `/`                 | GET    | Health check                                |
| `/pair`             | GET    | Request a pairing QR code                   |
| `/lovense-callback` | POST   | Pairing confirmation / device-online status |
| `/telegram`         | POST   | Telegram webhook — incoming commands        |

### Code layout

The pure logic is isolated from the Worker runtime so it can be unit-tested on
the host with no WASM toolchain — `worker` is a **wasm32-only dependency**.

| File             | Tested? | Responsibility                                    |
|------------------|---------|---------------------------------------------------|
| `src/command.rs` | ✅      | Lovense command model + server JSON body          |
| `src/telegram.rs`| ✅      | Parse `/vibrate`, `/stop`, durations, aliases     |
| `src/lovense.rs` | ✅      | `getQrCode` body, `md5` utoken, callback parsing  |
| `src/auth.rs`    | ✅      | Single-user authorization gate                    |
| `src/glue.rs`    | wasm    | Router + secrets + outbound HTTP (cfg-gated)      |

## Bot commands

```
/vibrate <0-20> [secs|30s|2m|1h]   strength 0–20, optional duration (default: until /stop)
/stop                              stop all motors
/pair                              get a QR code to link a device
/help                              show help
```

Only the Telegram user in `ALLOWED_USER_ID` may issue commands.

## Develop

With [nix](https://nixos.org) + [direnv](https://direnv.net) the dev shell is
automatic (`direnv allow`); otherwise `nix develop`.

```sh
cargo test                 # native unit tests
cargo clippy --all-targets -- -D warnings
cargo tarpaulin --engine ptrace --out Stdout --exclude-files 'src/glue.rs'
cargo build --target wasm32-unknown-unknown   # check the worker compiles
wrangler dev               # run locally
```

## Deploy

Prereqs: a [Lovense developer token](https://developer.lovense.com), a Telegram
bot token from [@BotFather](https://t.me/BotFather).

```sh
wrangler secret put BOT_TOKEN          # Telegram bot token
wrangler secret put TG_WEBHOOK_SECRET  # random string for webhook verification
wrangler secret put LOVENSE_TOKEN      # Lovense developer token
wrangler secret put LOVENSE_SALT       # salt for the pairing utoken
wrangler secret put ALLOWED_USER_ID    # your Telegram numeric user id

wrangler deploy
```

Point Telegram at the Worker (the secret rejects forged calls):

```sh
curl "https://api.telegram.org/bot<BOT_TOKEN>/setWebhook" \
  -d "url=https://edge-relay.<subdomain>.workers.dev/telegram" \
  -d "secret_token=<TG_WEBHOOK_SECRET>"
```

Set the Lovense developer-dashboard callback URL to
`https://edge-relay.<subdomain>.workers.dev/lovense-callback`.

## License

MIT

//! Cloudflare Worker entrypoint and HTTP glue. Compiled only for `wasm32`.
//!
//! This is the thin, side-effecting shell around the pure logic in the sibling
//! modules. The decision of what to do with an update lives in
//! [`telegram::dispatch`]; here we only read secrets, verify the webhook, and
//! perform the outbound HTTP.

use crate::auth;
use crate::command::{self, Command};
use crate::lovense::{self, QrRequest};
use crate::telegram::{self, BotCommand, Dispatch};
use worker::*;

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    Router::new()
        .get("/", |_, _| Response::ok("edge-relay: ok"))
        .post_async("/telegram", on_telegram)
        .post_async("/lovense-callback", on_callback)
        .get_async("/pair", on_pair)
        .run(req, env)
        .await
}

/// Telegram webhook: verify the secret, then act on the dispatch decision.
async fn on_telegram(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Reject anything that doesn't carry our shared webhook secret.
    let want = ctx.secret("TG_WEBHOOK_SECRET")?.to_string();
    let got = req
        .headers()
        .get("x-telegram-bot-api-secret-token")?
        .unwrap_or_default();
    if got != want {
        return Response::error("forbidden", 403);
    }

    let body = req.text().await?;
    let Ok(update) = serde_json::from_str::<telegram::Update>(&body) else {
        return Response::ok("ignored");
    };
    let allowed =
        auth::parse_allowed_id(&ctx.secret("ALLOWED_USER_ID")?.to_string()).unwrap_or(i64::MIN);

    match telegram::dispatch(&update, allowed) {
        Dispatch::Ignore => Response::ok("ignored"),
        Dispatch::Command { chat_id, command } => {
            let text = run_command(&ctx, command).await?;
            reply(&ctx, chat_id, &text).await.ok();
            Response::ok("ok")
        }
        Dispatch::Invalid { chat_id, error } => {
            reply(&ctx, chat_id, &telegram::invalid_reply(&error))
                .await
                .ok();
            Response::ok("ok")
        }
    }
}

/// Perform the side effects for a recognized command and return the reply text.
async fn run_command(ctx: &RouteContext<()>, command: BotCommand) -> Result<String> {
    // Pair is the only reply that needs a network result (the QR URL).
    if let BotCommand::Pair = command {
        return Ok(format!("Scan to pair: {}", request_qr(ctx).await?));
    }
    // Vibrate/Stop drive the toy; Help/Status have no effect.
    if let Some(cmd) = command.to_lovense() {
        send_lovense(ctx, cmd).await?;
    }
    Ok(command.ack().unwrap_or_default())
}

/// Lovense pairing callback — log the toy status and acknowledge.
async fn on_callback(mut req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let body = req.text().await?;
    if let Ok(cb) = serde_json::from_str::<lovense::Callback>(&body) {
        console_log!(
            "lovense callback: uid={} online_toys={}",
            cb.uid,
            cb.online_toys().len()
        );
    }
    Response::ok("ok")
}

/// Manual pairing helper: returns the QR image URL.
async fn on_pair(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    Response::ok(request_qr(&ctx).await?)
}

async fn send_lovense(ctx: &RouteContext<()>, cmd: Command) -> Result<()> {
    let token = ctx.secret("LOVENSE_TOKEN")?.to_string();
    let uid = ctx.var("LOVENSE_UID")?.to_string();
    let body = to_json(&cmd.to_server_body(&token, &uid))?;
    post_json(command::COMMAND_URL, &body).await?;
    Ok(())
}

async fn request_qr(ctx: &RouteContext<()>) -> Result<String> {
    let token = ctx.secret("LOVENSE_TOKEN")?.to_string();
    let uid = ctx.var("LOVENSE_UID")?.to_string();
    let salt = ctx
        .secret("LOVENSE_SALT")
        .map(|s| s.to_string())
        .unwrap_or_default();
    let body = to_json(&QrRequest::new(&token, &uid, "telegram", &salt))?;
    let resp = post_json(lovense::QR_URL, &body).await?;
    // Fall back to the raw response if the QR field isn't where we expect.
    Ok(lovense::extract_qr_url(&resp).unwrap_or(resp))
}

async fn reply(ctx: &RouteContext<()>, chat_id: i64, text: &str) -> Result<()> {
    let token = ctx.secret("BOT_TOKEN")?.to_string();
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let body = to_json(&serde_json::json!({ "chat_id": chat_id, "text": text }))?;
    post_json(&url, &body).await?;
    Ok(())
}

async fn post_json(url: &str, body: &str) -> Result<String> {
    let headers = Headers::new();
    headers.set("content-type", "application/json")?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(body.to_string().into()));

    let request = Request::new_with_init(url, &init)?;
    let mut resp = Fetch::Request(request).send().await?;
    resp.text().await
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| Error::RustError(e.to_string()))
}

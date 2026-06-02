//! Cloudflare Worker entrypoint and HTTP glue. Compiled only for `wasm32`.
//!
//! This is the thin, side-effecting shell around the pure logic in the sibling
//! modules: it reads secrets, verifies the Telegram webhook, parses the update,
//! gates on the allowed user, and performs the outbound HTTP calls.

use crate::auth;
use crate::command::{self, Command};
use crate::lovense::{self, QrRequest};
use crate::telegram::{self, BotCommand};
use serde::Deserialize;
use worker::*;

const HELP: &str = "Commands:\n\
    /vibrate <0-20> [secs|30s|2m|1h]\n\
    /stop\n\
    /pair\n\
    /help";

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

#[derive(Deserialize)]
struct Update {
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    text: String,
    from: Option<TgUser>,
    chat: Chat,
}

#[derive(Deserialize)]
struct TgUser {
    id: i64,
}

#[derive(Deserialize)]
struct Chat {
    id: i64,
}

/// Telegram webhook: verify, authorize, parse, dispatch.
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
    let Ok(update) = serde_json::from_str::<Update>(&body) else {
        return Response::ok("ignored");
    };
    let Some(msg) = update.message else {
        return Response::ok("ignored");
    };

    let allowed =
        auth::parse_allowed_id(&ctx.secret("ALLOWED_USER_ID")?.to_string()).unwrap_or(i64::MIN);
    let user_id = msg.from.as_ref().map(|u| u.id).unwrap_or_default();
    if !auth::is_allowed(user_id, allowed) {
        reply(&ctx, msg.chat.id, "Not authorized.").await.ok();
        return Response::ok("forbidden");
    }

    let response_text = match telegram::parse(&msg.text) {
        Ok(BotCommand::Vibrate { strength, time_sec }) => {
            send_lovense(&ctx, Command::vibrate(strength, time_sec)).await?;
            format!("Vibrate {strength} for {time_sec}s")
        }
        Ok(BotCommand::Stop) => {
            send_lovense(&ctx, Command::stop()).await?;
            "Stopped.".to_string()
        }
        Ok(BotCommand::Pair) => format!("Scan to pair: {}", request_qr(&ctx).await?),
        Ok(BotCommand::Help | BotCommand::Status) => HELP.to_string(),
        Err(e) => format!("Couldn't parse that: {e:?}"),
    };
    reply(&ctx, msg.chat.id, &response_text).await.ok();

    Response::ok("ok")
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
    let value: serde_json::Value = serde_json::from_str(&resp).unwrap_or_default();
    Ok(value["data"]["qr"]
        .as_str()
        .unwrap_or(resp.as_str())
        .to_string())
}

async fn reply(ctx: &RouteContext<()>, chat_id: i64, text: &str) -> Result<()> {
    let token = ctx.secret("BOT_TOKEN")?.to_string();
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let body = to_json(&serde_json::json!({ "chat_id": chat_id, "text": text }))?;
    post_json(&url, &body).await?;
    Ok(())
}

async fn post_json(url: &str, body: &str) -> Result<String> {
    let mut headers = Headers::new();
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

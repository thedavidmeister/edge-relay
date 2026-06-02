use worker::*;

/// edge-relay — a Cloudflare Worker (Rust/WASM) that bridges a Telegram bot to
/// the Lovense developer API. Stateless: the toy is addressed by a fixed
/// `LOVENSE_UID` chosen at pairing time, so no database is required.
///
/// Routes:
///   GET  /                 health check
///   POST /telegram         Telegram webhook (incoming commands)
///   POST /lovense-callback Lovense pairing callback (toy online status)
///   GET  /pair             request a Lovense QR code to pair a device
#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    Router::new()
        .get("/", |_, _| Response::ok("edge-relay: ok"))
        .post_async("/telegram", telegram_webhook)
        .post_async("/lovense-callback", lovense_callback)
        .get_async("/pair", pair)
        .run(req, env)
        .await
}

/// Incoming Telegram updates. Next: verify the secret-token header, parse the
/// Update, gate on ALLOWED_USER_ID, and translate /vibrate /stop etc. into
/// Lovense commands.
async fn telegram_webhook(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    // TODO: implement command dispatch.
    Response::ok("ok")
}

/// Lovense posts here after a user scans the pairing QR. Useful for confirming
/// the toy is online; not required to send commands in the cloud-command flow.
async fn lovense_callback(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    // TODO: log/confirm pairing.
    Response::ok("ok")
}

/// Ask Lovense for a pairing QR code, return the image URL.
async fn pair(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    // TODO: POST https://api.lovense.com/api/lan/getQrCode and return data.qr
    Response::ok("pair: todo")
}

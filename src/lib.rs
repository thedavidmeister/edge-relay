//! edge-relay — a Cloudflare Worker (Rust/WASM) that bridges a Telegram bot to
//! the Lovense developer API.
//!
//! The pure logic lives in [`command`], [`telegram`], [`lovense`] and [`auth`]
//! and is unit-tested on the host. The Worker HTTP glue in `glue` depends on the
//! `worker` crate and is compiled only for `wasm32`.

pub mod auth;
pub mod command;
pub mod lovense;
pub mod telegram;

#[cfg(target_arch = "wasm32")]
mod glue;

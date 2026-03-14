// SPDX-License-Identifier: GPL-3.0-only
mod cors;
mod handler;
mod models;
mod package_helper;

use axum::{routing::post, Router};
use clap::Parser;
use handler::AppState;
use std::sync::Arc;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Query Store Links — Rust API server.
///
/// Resolves Microsoft Store product information and package download links.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None, disable_help_flag = true)]
struct Args {
    /// Host address to listen on.
    #[arg(short = 'h', long, default_value = "0.0.0.0", env = "HOST")]
    host: String,

    /// Port to listen on.
    #[arg(short = 'p', long, default_value_t = 5236, env = "PORT")]
    port: u16,

    /// Comma-separated list of allowed CORS origins.
    /// Supports wildcard subdomains, e.g. `https://*.example.com`.
    #[arg(long, default_value = "https://*.krnl64.win", env = "ALLOWED_ORIGINS")]
    allowed_origins: String,

    /// Enable development mode: allows loopback origins (localhost, 127.0.0.1)
    /// in addition to the configured allowed origins.
    #[arg(long, env = "DEV")]
    dev: bool,

    /// Log level filter (e.g. `error`, `warn`, `info`, `debug`, `trace`).
    #[arg(long, default_value = "info", env = "LOG_LEVEL")]
    log_level: String,

    /// Print help information.
    #[arg(long, action = clap::ArgAction::Help)]
    help: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(EnvFilter::try_new(&args.log_level).unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let allowed_origins: Arc<Vec<String>> = Arc::new(
        args.allowed_origins
            .split(',')
            .map(|s| s.trim().to_string())
            .collect(),
    );

    let is_dev = args.dev;

    let cors = {
        let origins = Arc::clone(&allowed_origins);
        CorsLayer::new()
            .allow_origin(AllowOrigin::predicate(move |origin, _req| {
                let s = origin.to_str().unwrap_or("");
                (is_dev && cors::is_loopback(s)) || cors::is_origin_allowed(s, &origins)
            }))
            .allow_headers(tower_http::cors::Any)
            .allow_methods(tower_http::cors::Any)
    };

    let state = Arc::new(AppState {
        client: reqwest::Client::builder()
            .user_agent("qsl_rs")
            .build()
            .expect("failed to build HTTP client"),
    });

    let app = Router::new()
        .route("/api/links/resolve-all", post(handler::resolve_all))
        .layer(cors)
        .with_state(state);

    let addr = format!("{}:{}", args.host, args.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    info!("Listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}

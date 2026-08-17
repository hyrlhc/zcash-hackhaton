//! Live Zcash testnet, as JSON for the demo site.
//!
//! Does not decrypt shielded payments — the chain does not publish them.
//! Returns tip, consensus branch, orchard tree size, and the current root.
//!
//! `LightwalletClient` owns its own Tokio runtime, so all calls run on a
//! blocking thread. Calling it directly from axum's runtime panics.

use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use ff::PrimeField;
use serde::Serialize;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use zclaim_zcash::{LightwalletClient, TESTNET_ENDPOINT};

struct App {
    endpoint: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChainJson {
    endpoint: String,
    server_version: String,
    chain: String,
    tip_height: u32,
    consensus_branch: String,
    block_hash: String,
    orchard_leaves: u64,
    orchard_anchor: String,
    what_the_chain_hides: [&'static str; 4],
}

#[derive(Serialize)]
struct ErrorJson {
    error: String,
}

fn hex_field(f: pasta_curves::pallas::Base) -> String {
    hex::encode(f.to_repr())
}

fn snapshot(endpoint: &str) -> Result<ChainJson, String> {
    let client = LightwalletClient::connect(endpoint).map_err(|e| e.to_string())?;
    let info = client.chain_info().map_err(|e| e.to_string())?;
    let tree = client
        .tree_state(info.tip_height)
        .map_err(|e| e.to_string())?;
    Ok(ChainJson {
        endpoint: endpoint.to_string(),
        server_version: info.server_version,
        chain: info.chain,
        tip_height: info.tip_height,
        consensus_branch: info.consensus_branch,
        block_hash: tree.block_hash.clone(),
        orchard_leaves: tree.size(),
        orchard_anchor: hex_field(tree.anchor()),
        what_the_chain_hides: [
            "who paid",
            "who received",
            "exact amount",
            "memo / other payments",
        ],
    })
}

async fn chain(
    State(app): State<Arc<App>>,
) -> Result<Json<ChainJson>, (StatusCode, Json<ErrorJson>)> {
    let endpoint = app.endpoint.clone();
    let result = tokio::task::spawn_blocking(move || snapshot(&endpoint))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorJson {
                    error: e.to_string(),
                }),
            )
        })?;
    match result {
        Ok(body) => Ok(Json(body)),
        Err(e) => Err((StatusCode::BAD_GATEWAY, Json(ErrorJson { error: e }))),
    }
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    let endpoint = std::env::var("ZCLAIM_LIGHTWALLETD")
        .unwrap_or_else(|_| TESTNET_ENDPOINT.to_string());
    let app = Arc::new(App { endpoint });
    let router = Router::new()
        .route("/health", get(health))
        .route("/api/chain", get(chain))
        .layer(CorsLayer::permissive())
        .with_state(app);

    let bind = "127.0.0.1:8787";
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .expect("bind 8787");
    eprintln!("chain-api listening on http://{bind}  (Zcash testnet)");
    axum::serve(listener, router).await.expect("server");
}

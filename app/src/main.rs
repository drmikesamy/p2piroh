use std::{net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use iroh::{endpoint::presets, Endpoint, EndpointAddr};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tower_http::services::ServeDir;

const ALPN: &[u8] = b"p2piroh/text/v1";
const MAX_MSG: usize = 1024 * 1024;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    join: Option<String>,
    #[arg(long, default_value = "0.0.0.0:8080")]
    http: SocketAddr,
}

#[derive(Clone)]
struct AppState {
    text: Arc<RwLock<String>>,
    ws_tx: broadcast::Sender<String>,
    p2p_tx: broadcast::Sender<String>,
}

#[derive(Serialize, Deserialize)]
struct Ticket(EndpointAddr);

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let endpoint = Endpoint::builder(presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    endpoint.online().await;

    let ticket = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&Ticket(endpoint.addr()))?);
    println!("share this ticket with the other side:\n{}\n", ticket);

    let (ws_tx, _) = broadcast::channel(64);
    let (p2p_tx, _) = broadcast::channel(64);
    let state = AppState {
        text: Arc::new(RwLock::new(String::new())),
        ws_tx,
        p2p_tx,
    };

    let p2p_state = state.clone();
    match args.join {
        Some(t) => {
            let addr = parse_ticket(&t)?;
            tokio::spawn(async move {
                if let Err(e) = run_p2p_client(endpoint, addr, p2p_state).await {
                    eprintln!("p2p client error: {e:#}");
                }
            });
        }
        None => {
            tokio::spawn(async move {
                if let Err(e) = run_p2p_host(endpoint, p2p_state).await {
                    eprintln!("p2p host error: {e:#}");
                }
            });
        }
    }

    let app = Router::new()
        .route("/ws", get(ws))
        .with_state(state)
        .fallback_service(ServeDir::new("static").append_index_html_on_directories(true));

    let listener = tokio::net::TcpListener::bind(args.http).await?;
    println!("open http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

fn parse_ticket(t: &str) -> Result<EndpointAddr> {
    let raw = URL_SAFE_NO_PAD.decode(t)?;
    Ok(serde_json::from_slice::<Ticket>(&raw)?.0)
}

async fn run_p2p_host(endpoint: Endpoint, state: AppState) -> Result<()> {
    let conn = endpoint
        .accept()
        .await
        .context("waiting for incoming iroh connection")?
        .await
        .context("accepting iroh connection")?;
    run_conn(conn, state).await
}

async fn run_p2p_client(endpoint: Endpoint, addr: EndpointAddr, state: AppState) -> Result<()> {
    let conn = endpoint.connect(addr, ALPN).await?;
    run_conn(conn, state).await
}

async fn run_conn(conn: iroh::endpoint::Connection, state: AppState) -> Result<()> {
    let mut out = state.p2p_tx.subscribe();
    let conn_send = conn.clone();

    let send_task = tokio::spawn(async move {
        while let Ok(text) = out.recv().await {
            if let Ok(mut s) = conn_send.open_uni().await {
                let _ = s.write_all(text.as_bytes()).await;
                let _ = s.finish();
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        loop {
            let Ok(mut s) = conn.accept_uni().await else { break };
            let Ok(bytes) = s.read_to_end(MAX_MSG).await else { continue };
            let Ok(text) = String::from_utf8(bytes) else { continue };
            apply_remote(&state, text).await;
        }
    });

    let _ = tokio::join!(send_task, recv_task);
    Ok(())
}

async fn ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_session(socket, state))
}

async fn ws_session(socket: WebSocket, state: AppState) {
    let (mut tx, mut rx) = socket.split();
    let mut sub = state.ws_tx.subscribe();

    let init = state.text.read().await.clone();
    let _ = tx.send(Message::Text(init.into())).await;

    let writer = tokio::spawn(async move {
        while let Ok(next) = sub.recv().await {
            if tx.send(Message::Text(next.into())).await.is_err() {
                break;
            }
        }
    });

    let reader_state = state.clone();
    let reader = tokio::spawn(async move {
        while let Some(Ok(Message::Text(txt))) = rx.next().await {
            apply_local(&reader_state, txt.to_string()).await;
        }
    });

    let _ = tokio::join!(writer, reader);
}

async fn apply_local(state: &AppState, text: String) {
    let mut cur = state.text.write().await;
    if *cur != text {
        *cur = text.clone();
        let _ = state.ws_tx.send(text.clone());
        let _ = state.p2p_tx.send(text);
    }
}

async fn apply_remote(state: &AppState, text: String) {
    let mut cur = state.text.write().await;
    if *cur != text {
        *cur = text.clone();
        let _ = state.ws_tx.send(text);
    }
}

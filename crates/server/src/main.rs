//! libaw-server [ADDR]: serve the socket API (docs/socket-api.md) on ADDR (default
//! 127.0.0.1:8765), e.g. ws://127.0.0.1:8765.

#[tokio::main]
async fn main() {
    let addr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:8765".into());
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap_or_else(|e| panic!("bind {addr}: {e}"));
    eprintln!("libaw-server: protocol v{} on ws://{addr}", libaw_server::PROTOCOL);
    libaw_server::serve(listener).await;
}

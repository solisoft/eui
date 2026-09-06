//! `counter-server [addr]` — serve the counter on `ws://127.0.0.1:5090`.

#[tokio::main]
async fn main() {
    let addr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:5090".to_owned());
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("counter-server: cannot bind {addr}: {e}");
            std::process::exit(1);
        }
    };
    println!("counter-server: ws://{addr}  (run: EUI_ALLOW_INSECURE_LOOPBACK=1 eui ws://{addr})");
    counter_server::serve(listener).await;
}

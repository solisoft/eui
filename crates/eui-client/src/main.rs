//! `eui <wss://host/_eui/session>` — open an EUI application.

fn main() {
    let url = match std::env::args().nth(1) {
        Some(u) => u,
        None => {
            eprintln!("usage: eui <wss://host/_eui/session>");
            std::process::exit(2);
        }
    };
    if let Err(e) = eui_client::app::run(url) {
        eprintln!("eui: {e}");
        std::process::exit(1);
    }
}

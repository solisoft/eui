//! `eui [<wss://host/_eui/session/app>...] [--allow cap,cap]`.
//!
//! With no address, the shell: one window with a tab strip and an empty tab
//! to type into, where every application opened gets a tab beside the
//! others. With addresses, one chromeless window each — which is what an
//! embedding host gets, and what a packaged desktop application is.
//!
//! `--allow` names the capabilities of spec 01 §2.1 the person grants if an
//! application asks for them; nothing is granted otherwise, and the same
//! grant covers every address on the line.
//!
//! Either way it is one process: one GPU device, one set of pipelines, one
//! runtime. Each application still gets its own confined worker.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Spawned by a window as its worker: take that role and nothing else.
    if let Some(code) = eui_client::worker::entry(&args) {
        std::process::exit(code);
    }
    let mut urls: Vec<String> = Vec::new();
    let mut allowed = 0u32;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--allow" {
            let Some(list) = it.next() else { usage() };
            for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                match eui_proto::caps::from_name(name) {
                    Some(bit) => allowed |= bit,
                    None => {
                        eprintln!("eui: unknown capability {name:?}");
                        usage();
                    }
                }
            }
        } else {
            urls.push(a.clone());
        }
    }
    // No address: open the shell — one window with a tab strip, and an
    // empty tab to type into. With addresses, one chromeless window each,
    // which is what it has always done and what an embedding host gets.
    let result = if urls.is_empty() { eui_client::app::shell() } else { eui_client::app::launch_all(urls.into_iter().map(|u| eui_client::app::Launch::new(u, allowed)).collect()) };
    if let Err(e) = result {
        eprintln!("eui: {e}");
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!("usage: eui <wss://host/_eui/session/app>... [--allow camera,microphone,clipboard.read,clipboard.write,notifications,location,fs.pick]");
    std::process::exit(2);
}

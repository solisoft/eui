//! `eui <wss://host/_eui/session/app>... [--allow cap,cap]` — open one EUI
//! application per URL. `--allow` names the capabilities of spec 01 §2.1 the
//! person grants if an application asks for them; nothing is granted
//! otherwise, and the same grant covers every URL on the line.
//!
//! Several URLs share one process: one GPU device, one set of pipelines,
//! one runtime. Each still gets its own window and its own confined worker.

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
    if urls.is_empty() {
        usage();
    }
    let launches = urls.into_iter().map(|u| eui_client::app::Launch::new(u, allowed)).collect();
    if let Err(e) = eui_client::app::launch_all(launches) {
        eprintln!("eui: {e}");
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!("usage: eui <wss://host/_eui/session/app>... [--allow camera,microphone,clipboard.read,clipboard.write,notifications,location,fs.pick]");
    std::process::exit(2);
}

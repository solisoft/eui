//! `eui <wss://host/_eui/session/app> [--allow cap,cap]` — open an EUI
//! application. `--allow` names the capabilities of spec 01 §2.1 the person
//! grants if the application asks for them; nothing is granted otherwise.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut url = None;
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
        } else if url.is_none() {
            url = Some(a.clone());
        } else {
            usage();
        }
    }
    let Some(url) = url else { usage() };
    if let Err(e) = eui_client::app::run(url, allowed) {
        eprintln!("eui: {e}");
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!("usage: eui <wss://host/_eui/session/app> [--allow camera,microphone,clipboard.read,clipboard.write,notifications,location,fs.pick]");
    std::process::exit(2);
}

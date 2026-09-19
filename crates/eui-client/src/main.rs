//! `eui [<wss://host/_eui/session/app>...] [--allow cap,cap]`, and the
//! three flags that put an application in the desktop's own launcher
//! rather than opening it.
//!
//! With no address, the shell: one window with a tab strip and an empty tab
//! to type into, where every application opened gets a tab beside the
//! others. With addresses, one chromeless window each — which is what an
//! embedding host gets, and what a packaged desktop application is.
//!
//! `--allow` names the capabilities of spec 01 §2.1 the person grants if an
//! application asks for them; nothing is granted otherwise, and the same
//! grant covers every address on the line — and, in the shell, every tab
//! opened afterwards by typing one.
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
    // Whether to keep to ourselves. A window process is shared by default
    // now (`instance.rs`); this is the way back to one process per launch,
    // for a build under a debugger, a session that must not share the fate
    // of the others, or two builds side by side.
    let mut alone = std::env::var_os("EUI_STANDALONE").is_some();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        // The three that do their work and leave, rather than opening a
        // window. They are handled in the loop and not after it because
        // each takes the rest of the line as its own subject and there is
        // nothing to open afterwards.
        #[cfg(has_launchers)]
        if a == "--install" || a == "--uninstall" || a == "--installed" {
            std::process::exit(launcher(a, it.next()));
        }
        if a == "--standalone" {
            alone = true;
        } else if a == "--allow" {
            let Some(list) = it.next() else { usage() };
            for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                // `all` is the ten names without typing the ten names. It
                // grants nothing the list would not: the point of it is that
                // a person who has decided to trust an application should
                // not have to spell out the decision to be allowed to make
                // it, and a launcher that pre-grants is a great deal easier
                // to read as `--allow all` than as a line that wraps.
                if name == "all" {
                    allowed |= eui_proto::caps::ALL;
                    continue;
                }
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
    // The grant goes to the shell too. It used to be parsed and then
    // dropped on the floor here, which made `eui --allow fs.pick` a flag
    // that did nothing at all: the shell took no grant, so every tab it
    // opened asked for a capability it could never be given, and a file
    // dialog that will not open looks exactly like a button that is not
    // wired to anything.
    let chrome = urls.is_empty();
    let launches: Vec<eui_client::app::Launch> = urls.into_iter().map(|u| eui_client::app::Launch::new(u, allowed)).collect();
    // The instance already running opens it, if there is one and it
    // answers. Nothing is lost when it does not: this process goes on and
    // opens the window itself, which is what every build before this did.
    #[cfg(has_instance)]
    if !alone && eui_client::instance::hand_over(&launches, chrome, allowed) {
        return;
    }
    #[cfg(has_instance)]
    let result = if alone {
        if chrome {
            eui_client::app::shell(allowed)
        } else {
            eui_client::app::launch_all(launches)
        }
    } else {
        eui_client::app::joined(launches, chrome, allowed)
    };
    #[cfg(not(has_instance))]
    let result = {
        let _ = alone;
        if chrome {
            eui_client::app::shell(allowed)
        } else {
            eui_client::app::launch_all(launches)
        }
    };
    if let Err(e) = result {
        eprintln!("eui: {e}");
        std::process::exit(1);
    }
}

/// `--install <address>`, `--uninstall <app id>`, `--installed`.
///
/// Installing writes a launcher entry that runs this binary with the
/// address — so the application is the session it always was, started by
/// an icon. See [`eui_client::install`].
#[cfg(has_launchers)]
fn launcher(flag: &str, arg: Option<&String>) -> i32 {
    use eui_client::install;
    match flag {
        "--installed" => {
            let entries = install::list();
            if entries.is_empty() {
                println!("nothing is installed");
            }
            for e in entries {
                println!("{}\t{}\t{}", e.app_id, e.name, e.url);
            }
            0
        }
        "--uninstall" => {
            let Some(app_id) = arg else { usage() };
            match install::uninstall(app_id) {
                Ok(gone) if gone.is_empty() => {
                    eprintln!("eui: {app_id} was not installed");
                    0
                }
                Ok(gone) => {
                    for f in gone {
                        println!("removed {}", f.display());
                    }
                    0
                }
                Err(e) => {
                    eprintln!("eui: {e}");
                    1
                }
            }
        }
        _ => {
            let Some(url) = arg else { usage() };
            #[cfg(all(has_pins, has_native_net))]
            match install::from_url(url).and_then(|app| {
                let name = app.name.clone();
                install::install(&app).map(|files| (name, files))
            }) {
                Ok((name, files)) => {
                    for f in files {
                        println!("wrote {}", f.display());
                    }
                    println!("{name} is in the launcher");
                    0
                }
                Err(e) => {
                    eprintln!("eui: {e}");
                    1
                }
            }
            #[cfg(not(all(has_pins, has_native_net)))]
            {
                let _ = url;
                eprintln!("eui: this build cannot verify a manifest, so it will not install one");
                1
            }
        }
    }
}

fn usage() -> ! {
    // From `caps::NAMES` rather than written out: this line was already
    // two capabilities behind the protocol -- it named neither `nfc` nor
    // `scene` -- and a usage message that omits the flag somebody needs is
    // worse than none, because it reads as a list of everything there is.
    let all = eui_proto::caps::NAMES.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(",");
    eprintln!("usage: eui <wss://host/_eui/session/app>... [--allow all|{all}] [--standalone]");
    eprintln!("       eui --install <wss://host/...> | --uninstall <app id> | --installed");
    std::process::exit(2);
}

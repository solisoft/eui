//! The applications this person has actually opened, for the shell's
//! new-tab page.
//!
//! Only an address that *answered* is remembered — one whose session sent a
//! frame — so the list is somewhere to click rather than a log of typing
//! mistakes. It holds ten; the eleventh pushes the oldest out.
//!
//! It is deliberately not a browser history. There is no timestamp, no
//! visit count and no path beyond the session URL, because nothing here
//! needs them and each one would be a thing to explain to whoever reads the
//! file. It lives beside the pin store, in the person's own config
//! directory, as one line per application.

use std::io::Write;
use std::path::PathBuf;

/// How many are kept. The eleventh drops the oldest.
pub const KEEP: usize = 10;

/// An application worth offering again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    /// The session URL, whole — what opening it needs.
    pub url: String,
    /// What to call it: the manifest's name where the session gave one.
    pub name: String,
}

/// Where the list lives: beside the pin store, which is already the client's
/// corner of the person's configuration.
fn path() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("EUI_RECENT_FILE") {
        return Some(PathBuf::from(d));
    }
    // Neither phone has XDG, and neither has a home directory in the sense
    // meant below: the application's own sandbox is the only place it may
    // write.
    #[cfg(target_os = "android")]
    let dir = crate::android::data_dir()?;
    #[cfg(target_os = "ios")]
    let dir = crate::ios::data_dir()?;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let dir = if let Some(d) = std::env::var_os("XDG_CONFIG_HOME") {
        PathBuf::from(d).join("eui")
    } else if let Some(d) = std::env::var_os("APPDATA") {
        PathBuf::from(d).join("eui")
    } else {
        PathBuf::from(std::env::var_os("HOME")?).join(".config").join("eui")
    };
    Some(dir.join("recent"))
}

/// Parse the file's own format: `url` and `name` separated by a tab, one
/// application per line. A line that does not parse is skipped rather than
/// discarding the rest — a truncated write should cost one entry, not all
/// of them.
fn parse(text: &str) -> Vec<Recent> {
    text.lines()
        .filter_map(|line| {
            let (url, name) = line.split_once('\t')?;
            let (url, name) = (url.trim(), name.trim());
            if url.is_empty() {
                return None;
            }
            Some(Recent { url: url.to_owned(), name: name.to_owned() })
        })
        .take(KEEP)
        .collect()
}

/// The list, most recently opened first. Empty when there is no file, which
/// is the ordinary state of a fresh installation.
pub fn load() -> Vec<Recent> {
    let Some(p) = path() else {
        return Vec::new();
    };
    std::fs::read_to_string(p).map(|t| parse(&t)).unwrap_or_default()
}

/// Put `url` at the front, under `name`, and write the list back.
///
/// Returns the list as it now stands, so a caller that is about to redraw
/// does not have to read the file again. A failure to write is not reported:
/// losing this list costs a little convenience and nothing else, and there
/// is nowhere useful to say so from.
pub fn remember(url: &str, name: &str) -> Vec<Recent> {
    let mut list = load();
    list.retain(|r| r.url != url);
    list.insert(0, Recent { url: url.to_owned(), name: name.to_owned() });
    list.truncate(KEEP);
    if let Some(p) = path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // A tab or a newline in either field would make a second line out of
        // one entry, so neither is allowed through.
        let body: String = list.iter().map(|r| format!("{}\t{}\n", clean(&r.url), clean(&r.name))).collect();
        if let Ok(mut f) = std::fs::File::create(&p) {
            let _ = f.write_all(body.as_bytes());
        }
    }
    list
}

fn clean(s: &str) -> String {
    s.chars().filter(|c| *c != '\t' && *c != '\n' && *c != '\r').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_is_first_and_a_repeat_does_not_appear_twice() {
        let list = parse("a\tA\nb\tB\n");
        assert_eq!(list.len(), 2);
        assert_eq!(list.first().map(|r| r.url.as_str()), Some("a"));

        // What `remember` does to a list, without touching a file.
        let mut list = list;
        list.retain(|r| r.url != "b");
        list.insert(0, Recent { url: "b".into(), name: "B".into() });
        assert_eq!(list.iter().map(|r| r.url.as_str()).collect::<Vec<_>>(), ["b", "a"]);
    }

    #[test]
    fn a_line_that_does_not_parse_costs_only_itself() {
        let list = parse("good\tGood\nrubbish-with-no-tab\n\tnameless\nalso\tFine\n");
        assert_eq!(list.iter().map(|r| r.url.as_str()).collect::<Vec<_>>(), ["good", "also"]);
    }

    #[test]
    fn it_holds_ten() {
        let text: String = (0..40).map(|i| format!("u{i}\tn{i}\n")).collect();
        assert_eq!(parse(&text).len(), KEEP);
    }

    #[test]
    fn a_tab_in_a_name_cannot_forge_a_second_entry() {
        assert_eq!(clean("we\tird\nname"), "weirdname");
    }
}

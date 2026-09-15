//! `text/uri-list`: what a hand full of files looks like on the wire.

use std::path::PathBuf;

use eui_wayland::uris::paths;

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn one_file_from_gtk() {
    // What Nautilus and every GTK file manager send: CRLF, and a trailing
    // one on the last line.
    assert_eq!(paths(b"file:///home/a/b.csv\r\n"), vec![p("/home/a/b.csv")]);
}

#[test]
fn several_keep_the_order_they_came_in() {
    let body = b"file:///tmp/one.txt\r\nfile:///tmp/two.txt\r\nfile:///tmp/three.txt\r\n";
    assert_eq!(paths(body), vec![p("/tmp/one.txt"), p("/tmp/two.txt"), p("/tmp/three.txt")]);
}

#[test]
fn a_bare_newline_is_a_line_too() {
    // Not what the RFC says, and what a shell script piping `echo` sends.
    assert_eq!(paths(b"file:///tmp/a\nfile:///tmp/b"), vec![p("/tmp/a"), p("/tmp/b")]);
}

#[test]
fn comments_and_blank_lines_are_not_files() {
    let body = b"# a comment\r\n\r\nfile:///tmp/a\r\n\r\n";
    assert_eq!(paths(body), vec![p("/tmp/a")]);
}

#[test]
fn localhost_is_this_machine_and_so_is_nothing() {
    assert_eq!(paths(b"file://localhost/tmp/a\r\n"), vec![p("/tmp/a")]);
    assert_eq!(paths(b"FILE://LOCALHOST/tmp/a\r\n"), vec![p("/tmp/a")]);
}

#[test]
fn a_single_slash_is_accepted_too() {
    // Nothing well-behaved sends `file:/path`, and it costs one branch to
    // take from whatever does.
    assert_eq!(paths(b"file:/tmp/a\r\n"), vec![p("/tmp/a")]);
}

#[test]
fn another_host_is_not_a_file_here() {
    assert!(paths(b"file://other-host/tmp/a\r\n").is_empty());
}

#[test]
fn only_files_are_taken() {
    let body = b"http://example.com/x\r\nfile:///tmp/a\r\nmailto:someone@example.com\r\n";
    assert_eq!(paths(body), vec![p("/tmp/a")]);
}

#[test]
fn the_scheme_is_case_insensitive() {
    assert_eq!(paths(b"FILE:///tmp/a\r\n"), vec![p("/tmp/a")]);
}

#[test]
fn percent_escapes_are_decoded() {
    assert_eq!(paths(b"file:///tmp/a%20b.csv\r\n"), vec![p("/tmp/a b.csv")]);
    assert_eq!(paths(b"file:///tmp/caf%C3%A9\r\n"), vec![p("/tmp/caf\u{e9}")]);
    // `%25` is how a literal `%` is spelt.
    assert_eq!(paths(b"file:///tmp/100%25.txt\r\n"), vec![p("/tmp/100%.txt")]);
}

#[test]
fn a_truncated_escape_keeps_its_percent() {
    // Malformed either way; a filename with a `%` in it is likelier than a
    // sender that meant something else, so the line survives.
    assert_eq!(paths(b"file:///tmp/a%A\r\n"), vec![p("/tmp/a%A")]);
    assert_eq!(paths(b"file:///tmp/a%\r\n"), vec![p("/tmp/a%")]);
    assert_eq!(paths(b"file:///tmp/a%zz\r\n"), vec![p("/tmp/a%zz")]);
}

#[test]
fn a_path_that_is_not_utf8_arrives_intact() {
    use std::os::unix::ffi::OsStrExt;

    // The whole reason the decoding hands back bytes: this name is a real
    // one a kernel will open, and there is no `String` it survives.
    let want = PathBuf::from(std::ffi::OsStr::from_bytes(b"/tmp/\xff\xfe.bin"));
    assert_eq!(paths(b"file:///tmp/%FF%FE.bin\r\n"), vec![want]);
}

#[test]
fn a_nul_is_not_a_path() {
    // A byte that cannot reach the kernel is a URI to put down, not one to
    // truncate into a path that names something else entirely.
    assert!(paths(b"file:///tmp/a%00b\r\n").is_empty());
}

#[test]
fn an_empty_path_is_not_one() {
    // No authority and no path at all: nothing was named.
    assert!(paths(b"file://\r\n").is_empty());
    assert!(paths(b"file:\r\n").is_empty());
    // `file:///` names the root directory, which is a path — just never a
    // useful one to have been handed. Taken, and refused upstream by the
    // same `metadata` call every other path goes through.
    assert_eq!(paths(b"file:///\r\n"), vec![p("/")]);
}

#[test]
fn nothing_at_all_is_no_files() {
    assert!(paths(b"").is_empty());
    assert!(paths(b"\r\n").is_empty());
}

#[test]
fn a_very_long_list_is_bounded_work() {
    let mut body = Vec::new();
    for i in 0..10_000 {
        body.extend_from_slice(format!("file:///tmp/f{i}\r\n").as_bytes());
    }
    assert_eq!(paths(&body).len(), 10_000);
}

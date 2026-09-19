//! The shell's chrome answers a pointer and a keyboard.
//!
//! It is drawn by the client for the client, so nothing on the wire covers
//! it and no server test can reach it. What these check is the whole of the
//! mechanism: a click lands on a node, the node id says what the click
//! meant, and the address field's committed text comes back as a URL to
//! open. None of it needs a GPU — layout and hit-testing are the driver's,
//! and the pixels are somebody else's problem.

use eui_client::chrome::{Action, Chrome, TabView, Trust};
use eui_client::Input;

const W: f32 = 900.0;
const H: f32 = 560.0;

fn tabs() -> Vec<TabView<'static>> {
    vec![
        TabView {
            title: "Vitrine",
            origin: "wss://vitrine.example",
            path: "/_eui/session/gallery",
            trust: Some(Trust::Pinned),
            link: None,
            can_back: false,
            can_forward: false,
            grants: None,
            installed: None,
        },
        TabView {
            title: "Needle",
            origin: "wss://needle.example",
            path: "/_eui/session/music",
            trust: Some(Trust::Pinned),
            link: None,
            can_back: false,
            can_forward: false,
            grants: None,
            installed: None,
        },
        TabView {
            title: "Feedx",
            origin: "ws://127.0.0.1:5090",
            path: "/_eui/session/feed",
            trust: Some(Trust::Local),
            link: None,
            can_back: false,
            can_forward: false,
            grants: None,
            installed: None,
        },
    ]
}

/// The padlock is there when an application asked for something, and it is
/// what reopens the question.
///
/// The sheet is shown once, before the first connection, and never again on
/// its own — so without this a person who said yes quickly, or who wants the
/// camera back, has nowhere to go. It must also stay absent for a tab whose
/// application asked for nothing, because an icon that opens an empty sheet
/// is worse than no icon.
#[test]
fn the_address_row_offers_the_permissions_back() {
    let mut chrome = Chrome::new(W, H, 1.0);

    let mut asking = tabs();
    if let Some(first) = asking.first_mut() {
        first.grants = Some(eui_proto::caps::CAMERA | eui_proto::caps::FS_PICK);
    }
    chrome.rebuild(&asking, 0);
    let found = (0..40).find_map(|i| {
        let x = 100.0 + f64::from(i) as f32 * 4.0;
        let got = click(&mut chrome, x, 54.0);
        got.contains(&Action::Permissions).then_some(x)
    });
    assert!(found.is_some(), "no padlock anywhere along the address row");

    // And nothing to press for an application that asked for nothing.
    chrome.rebuild(&tabs(), 0);
    for i in 0..40 {
        let x = 100.0 + f64::from(i) as f32 * 4.0;
        assert!(!click(&mut chrome, x, 54.0).contains(&Action::Permissions), "a padlock at {x} for a tab that was never asked anything");
    }
}

/// Press and release at a point, and say what the chrome made of it.
fn click(chrome: &mut Chrome, x: f32, y: f32) -> Vec<Action> {
    let _ = chrome.input(Input::PointerMove(x, y));
    let _ = chrome.input(Input::PointerDown(0));
    chrome.input(Input::PointerUp(0))
}

#[test]
fn a_click_on_a_tab_selects_it_and_one_on_its_cross_closes_it() {
    let mut chrome = Chrome::new(W, H, 1.0);
    chrome.rebuild(&tabs(), 0);

    // Tabs are laid out from the left of the strip, each growing to its
    // 200 px cap. The first is under the pointer at 60 px in, the second
    // past 200. Both are in the strip's own 36 px band.
    assert_eq!(click(&mut chrome, 60.0, 18.0), vec![Action::Select(0)], "the first tab");
    assert_eq!(click(&mut chrome, 260.0, 18.0), vec![Action::Select(1)], "the second tab");
    assert_eq!(click(&mut chrome, 460.0, 18.0), vec![Action::Select(2)], "the third tab");

    // The cross sits at the tab's right edge, inside its padding. Closing
    // is not selecting: the two must not both fire.
    let closed = click(&mut chrome, 190.0, 18.0);
    assert_eq!(closed, vec![Action::Close(0)], "the first tab's cross, and nothing else");
}

#[test]
fn the_new_tab_button_is_past_the_last_tab() {
    let mut chrome = Chrome::new(W, H, 1.0);
    chrome.rebuild(&tabs(), 0);
    // Three tabs at 200 px and a 2 px gap each leaves the button just
    // past 610.
    assert_eq!(click(&mut chrome, 622.0, 18.0), vec![Action::NewTab]);
}

#[test]
fn an_address_typed_into_an_empty_tab_comes_back_as_a_url_to_open() {
    let mut chrome = Chrome::new(W, H, 1.0);
    // A tab with no application: the chrome owns the window below the
    // strip and gives its own field the focus, so the first keystroke is
    // already part of an address.
    chrome.rebuild(&[TabView { title: "New tab", origin: "", path: "", trust: None, link: None, can_back: false, can_forward: false, grants: None, installed: None }], 0);
    assert!(chrome.is_blank());
    assert!(!chrome.content_top().is_finite(), "no application has room in an empty tab");

    let typed = "wss://vitrine.example/_eui/session/gallery";
    let out = chrome.input(Input::Text(typed.to_owned()));
    assert!(out.is_empty(), "typing is not opening");

    // Enter commits the field, which is what carries the text back.
    let out = chrome.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true });
    assert_eq!(out, vec![Action::Open(typed.to_owned())], "the address, whole");
}

#[test]
fn an_empty_address_opens_nothing() {
    let mut chrome = Chrome::new(W, H, 1.0);
    chrome.rebuild(&[TabView { title: "New tab", origin: "", path: "", trust: None, link: None, can_back: false, can_forward: false, grants: None, installed: None }], 0);
    let _ = chrome.input(Input::Text("   ".to_owned()));
    let out = chrome.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true });
    assert!(out.is_empty(), "whitespace is not an address");
}

#[test]
fn a_click_below_the_chrome_is_not_the_chromes() {
    let mut chrome = Chrome::new(W, H, 1.0);
    chrome.rebuild(&tabs(), 0);
    // 80 px of chrome — a 36 px strip over a 44 px address row — and
    // everything under it belongs to the application. The shell routes on
    // this number; the chrome itself must also find nothing there.
    assert_eq!(chrome.content_top(), 80.0);
    assert!(click(&mut chrome, 400.0, 300.0).is_empty(), "the application's half of the window");
}

#[test]
fn the_address_bar_shows_the_active_tabs_origin_and_switches_with_it() {
    let mut chrome = Chrome::new(W, H, 1.0);
    let t = tabs();
    chrome.rebuild(&t, 0);
    assert!(!chrome.is_blank(), "a tab with an application in it is not blank");

    // Clicking the field is what puts it into editing — the origin and the
    // path are two text nodes until then, so that the origin can be the
    // legible half.
    let out = click(&mut chrome, 400.0, 58.0);
    assert_eq!(out, vec![Action::EditAddress], "the address bar, in its 44 px row");

    // And it follows whichever tab is active.
    chrome.rebuild(&t, 2);
    assert_eq!(click(&mut chrome, 400.0, 58.0), vec![Action::EditAddress]);
}

/// Clicking into the address bar, changing nothing, and pressing Enter has
/// to give the keyboard back.
///
/// The driver emits `Change` only when the text actually moved, so an
/// unchanged address produced no action at all: the bar stayed in editing,
/// nothing happened, and there was no way out of it.
#[test]
fn enter_on_an_unchanged_address_leaves_the_bar() {
    let mut chrome = Chrome::new(W, H, 1.0);
    let t = tabs();
    chrome.rebuild(&t, 0);
    assert!(!chrome.holds_keys(), "a loaded tab leaves the keyboard to its application");

    assert_eq!(click(&mut chrome, 400.0, 58.0), vec![Action::EditAddress]);
    chrome.edit_address();
    chrome.rebuild(&t, 0);
    assert!(chrome.holds_keys(), "the bar being edited holds the keyboard");

    // Enter with the address as it was.
    let out = chrome.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true });
    assert_eq!(out, vec![Action::LeaveAddress], "unchanged: leave, and open nothing");
}

/// Editing the address and pressing Enter opens the new one *and* leaves.
#[test]
fn a_changed_address_opens_and_then_leaves() {
    let mut chrome = Chrome::new(W, H, 1.0);
    let t = tabs();
    chrome.edit_address();
    chrome.rebuild(&t, 0);

    let typed = "wss://elsewhere.example/_eui/session/x";
    let _ = chrome.input(Input::Text(typed.to_owned()));
    let out = chrome.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true });
    assert!(out.iter().any(|a| matches!(a, Action::Open(_))), "the address is opened: {out:?}");
    assert!(out.contains(&Action::LeaveAddress), "and the bar is left too: {out:?}");
}

/// The keyboard belongs to the application until the bar is entered, and
/// again as soon as it is left.
#[test]
fn the_keyboard_changes_hands_with_the_address_bar() {
    let mut chrome = Chrome::new(W, H, 1.0);
    let t = tabs();
    chrome.rebuild(&t, 0);
    assert!(!chrome.holds_keys());

    chrome.edit_address();
    chrome.rebuild(&t, 0);
    assert!(chrome.holds_keys(), "editing takes it");

    chrome.leave_address();
    chrome.rebuild(&t, 0);
    assert!(!chrome.holds_keys(), "leaving gives it back");

    // An empty tab always holds it: its page is the chrome's own.
    chrome.rebuild(&[TabView { title: "New tab", origin: "", path: "", trust: None, link: None, can_back: false, can_forward: false, grants: None, installed: None }], 0);
    assert!(chrome.holds_keys(), "an empty tab has no application to give it to");
}

/// The chrome draws a box the size of the window under everything, so its
/// background is the ground the application stands on wherever the
/// application paints none of its own — which is most pages, since a root
/// with no `bg` is the ordinary case.
///
/// It therefore has to follow the palette the page is in. The viewer can
/// change that from inside the application, with a local `theme.toggle()`
/// that never reaches the window's own theme handling; before the shell
/// forwarded it, a page switched to light sat on a floor still in the dark.
#[test]
fn the_chromes_ground_follows_the_palette_it_is_put_in() {
    let mut chrome = Chrome::new(W, H, 1.0);
    chrome.rebuild(&tabs(), 0);
    let dark = {
        let _ = chrome.input(Input::Mode(eui_proto::ThemeMode::Dark));
        chrome.paint(W as u32, H as u32).clear
    };
    let light = {
        let _ = chrome.input(Input::Mode(eui_proto::ThemeMode::Light));
        chrome.paint(W as u32, H as u32).clear
    };
    assert_ne!(dark, light, "the two palettes must not share a background");
    // And back, exactly: the mode is a choice, not an accumulation.
    let _ = chrome.input(Input::Mode(eui_proto::ThemeMode::Dark));
    assert_eq!(chrome.paint(W as u32, H as u32).clear, dark);
}

fn blank() -> Vec<TabView<'static>> {
    vec![TabView { title: "New tab", origin: "", path: "", trust: None, link: None, can_back: false, can_forward: false, grants: None, installed: None }]
}

fn recents() -> Vec<eui_client::recent::Recent> {
    [("Vitrine", "wss://a.example/_eui/session/gallery"), ("Needle", "wss://b.example/_eui/session/music"), ("Feedx", "ws://127.0.0.1:5090/_eui/session/feed")]
        .into_iter()
        .map(|(n, u)| eui_client::recent::Recent { url: u.to_owned(), name: n.to_owned() })
        .collect()
}

fn press(chrome: &mut Chrome, key: &str) -> Vec<Action> {
    chrome.input(Input::Key { key: key.into(), modifiers: 0, down: true })
}

#[test]
fn the_arrows_walk_the_recent_list_and_enter_opens_what_they_are_on() {
    let mut chrome = Chrome::new(W, H, 1.0);
    chrome.set_recents(recents());
    chrome.rebuild(&blank(), 0);

    // Enter with the keyboard still in the field opens nothing: there is
    // nothing typed and nothing picked.
    assert!(press(&mut chrome, "Enter").is_empty());

    assert_eq!(press(&mut chrome, "ArrowDown"), vec![Action::Rebuild], "onto the first");
    chrome.rebuild(&blank(), 0);
    assert_eq!(press(&mut chrome, "ArrowDown"), vec![Action::Rebuild], "onto the second");
    chrome.rebuild(&blank(), 0);
    assert_eq!(press(&mut chrome, "Enter"), vec![Action::Open("wss://b.example/_eui/session/music".to_owned())]);

    // Up off the first row gives the keyboard back to the field, and Enter
    // there is the field's again.
    chrome.rebuild(&blank(), 0);
    let _ = press(&mut chrome, "ArrowDown");
    chrome.rebuild(&blank(), 0);
    let _ = press(&mut chrome, "ArrowUp");
    chrome.rebuild(&blank(), 0);
    assert!(press(&mut chrome, "Enter").is_empty(), "back in an empty field");
}

#[test]
fn delete_takes_the_picked_entry_off_the_list() {
    let mut chrome = Chrome::new(W, H, 1.0);
    chrome.set_recents(recents());
    chrome.rebuild(&blank(), 0);
    assert!(press(&mut chrome, "Delete").is_empty(), "nothing picked, nothing deleted");

    let _ = press(&mut chrome, "ArrowDown");
    chrome.rebuild(&blank(), 0);
    let _ = press(&mut chrome, "ArrowDown");
    chrome.rebuild(&blank(), 0);
    assert_eq!(press(&mut chrome, "Delete"), vec![Action::Forget("wss://b.example/_eui/session/music".to_owned())]);

    // The shell answers a `Forget` by writing the list back and handing it
    // over; the pick stands on what moved up into the gap.
    let mut left = recents();
    left.remove(1);
    chrome.set_recents(left);
    chrome.rebuild(&blank(), 0);
    assert_eq!(press(&mut chrome, "Enter"), vec![Action::Open("ws://127.0.0.1:5090/_eui/session/feed".to_owned())]);
}

#[test]
fn typing_takes_the_keyboard_back_and_keeps_what_was_typed() {
    let mut chrome = Chrome::new(W, H, 1.0);
    chrome.set_recents(recents());
    chrome.rebuild(&blank(), 0);
    let _ = chrome.input(Input::Text("wss://c.exa".to_owned()));
    let _ = press(&mut chrome, "ArrowDown");
    chrome.rebuild(&blank(), 0);

    // A character now means the field, not the list — and the rebuild that
    // drops the highlight must not drop the address with it.
    let out = chrome.input(Input::Text("m".to_owned()));
    assert_eq!(out, vec![Action::Rebuild], "the highlight goes");
    chrome.rebuild(&blank(), 0);
    assert_eq!(press(&mut chrome, "Enter"), vec![Action::Open("wss://c.exam".to_owned())], "the address survived");
}

fn one_tab(back: bool, forward: bool) -> Vec<TabView<'static>> {
    vec![TabView {
        title: "Vitrine",
        origin: "wss://vitrine.example",
        path: "/_eui/session/gallery",
        trust: Some(Trust::Pinned),
        link: None,
        can_back: back,
        can_forward: forward,
        grants: None,
        installed: None,
    }]
}

/// Back and forward sit before the address, in that order, and keep their
/// place when there is nowhere to go — an arrow that vanishes takes the
/// address bar sideways with it.
#[test]
fn the_arrows_before_the_address_step_through_the_tabs_trail() {
    let mut chrome = Chrome::new(W, H, 1.0);
    chrome.rebuild(&one_tab(true, true), 0);
    // The address row is the 44 px band under the 36 px strip; the two boxes
    // are 28 px wide from the row's 6 px padding, with a 4 px gap.
    let (back, forward) = ((20.0, 58.0), (52.0, 58.0));
    assert_eq!(click(&mut chrome, back.0, back.1), vec![Action::Back]);
    assert_eq!(click(&mut chrome, forward.0, forward.1), vec![Action::Forward]);

    // Nowhere to go: the same two points answer nothing, and the address
    // field has not moved.
    let field = click(&mut chrome, 300.0, 58.0);
    chrome.rebuild(&one_tab(false, false), 0);
    assert!(click(&mut chrome, back.0, back.1).is_empty(), "no trail behind");
    assert!(click(&mut chrome, forward.0, forward.1).is_empty(), "none ahead");
    assert_eq!(click(&mut chrome, 300.0, 58.0), field, "and the address is where it was");
    assert_eq!(field, vec![Action::EditAddress], "which is the address bar");
}

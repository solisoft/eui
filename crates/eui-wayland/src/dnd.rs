//! The data device, on winit's own connection.

use std::collections::HashMap;
use std::os::fd::{AsFd, OwnedFd};
use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::thread::JoinHandle;

use wayland_backend::client::{Backend, ObjectId};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::{wl_data_device, wl_data_offer, wl_registry, wl_seat, wl_surface};
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};

use crate::Drag;

/// The mime this client asks for and the only one it takes. A drag that
/// does not offer it is a drag of something that is not files.
const URI_LIST: &str = "text/uri-list";

/// The most a `text/uri-list` may be before the sender is assumed to be
/// answering a different question. A hundred paths is under 8 KiB.
const MAX_LIST: usize = 64 * 1024;

/// How long to wait, in total, for the other client to write the list.
/// This read happens on the dispatch thread, so a wedge here is a thread
/// that never gets back to its queue.
const TRANSFER_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

/// A thread watching one window's data device, and the pipe that stops it.
#[derive(Debug)]
pub struct Dnd {
    /// The write end. A byte on it, or its close, ends the loop.
    stop: Option<OwnedFd>,
    thread: Option<JoinHandle<()>>,
}

/// A `wl_display` on its way to the thread. libwayland's display is shared
/// between threads by design — that is what `wl_display_prepare_read_queue`
/// is for — so the pointer may cross, and the `Send` says only that.
struct DisplayPtr(*mut core::ffi::c_void);

// SAFETY: see `DisplayPtr`. The pointer is used only to build a `Backend`,
// which takes its own lock on everything it touches.
unsafe impl Send for DisplayPtr {}

impl Dnd {
    /// Watch `surface` for dragged and dropped files, on the connection
    /// `display` is already running.
    ///
    /// `wake` is called after each message is queued, so the caller's event
    /// loop can come and take what is waiting.
    ///
    /// `None` when there is no libwayland, no `wl_data_device_manager`, no
    /// seat, or no thread to be had — a window with no file drop, which is
    /// where every Wayland desktop has been since this client shipped.
    ///
    /// # Safety
    ///
    /// `display` must be a live `wl_display` and `surface` a live
    /// `wl_surface` **on that display**, both belonging to this process's
    /// own window, and both must outlive the returned `Dnd`. All three hold
    /// at the one call site: the pointers come straight out of winit's
    /// window handle, and the `Dnd` is dropped in `Shell::close`, before the
    /// window it points into.
    #[must_use]
    pub fn start(display: *mut core::ffi::c_void, surface: *mut core::ffi::c_void, wake: Box<dyn Fn() + Send>) -> Option<(Self, std::sync::mpsc::Receiver<Drag>)> {
        if display.is_null() || surface.is_null() {
            return None;
        }
        // Our surface's identity, taken here on the main thread and moved
        // in. Made once and never again: winit owns that proxy, and the
        // thread must never reach into it.
        //
        // SAFETY: the caller's contract, above.
        let ours = unsafe { ObjectId::from_ptr(wl_surface::WlSurface::interface(), surface.cast()) }.ok()?;
        let (stop_r, stop_w) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC).ok()?;
        let (tx, rx) = std::sync::mpsc::channel();
        let display = DisplayPtr(display.cast());
        let thread = std::thread::Builder::new()
            .name("eui-wayland-dnd".into())
            .spawn(move || {
                let display = display;
                // SAFETY: the caller's contract, carried across the spawn
                // by `DisplayPtr`.
                let backend = unsafe { Backend::from_foreign_display(display.0.cast()) };
                run(&Connection::from_backend(backend), ours, tx, wake, &stop_r);
            })
            .ok()?;
        Some((Self { stop: Some(stop_w), thread: Some(thread) }, rx))
    }
}

impl Drop for Dnd {
    fn drop(&mut self) {
        // Closing the write end is what the thread's poll is waiting for.
        // The byte is sent first all the same: a descriptor this process
        // duplicated somewhere would keep the close from being seen, and
        // a join that never returns is a window that will not shut.
        if let Some(fd) = self.stop.take() {
            let _ = rustix::io::write(&fd, b"x");
            drop(fd);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A seat, and the data device asked of it.
struct Seat {
    seat: wl_seat::WlSeat,
    /// `None` only between a seat arriving and the manager arriving. The
    /// registry announces its globals in whatever order it likes, and a
    /// seat seen first would otherwise have no device for the whole run —
    /// which on a compositor that happens to announce them that way is a
    /// window that takes no files at all.
    device: Option<wl_data_device::WlDataDevice>,
}

/// The drag currently over our surface.
struct Over {
    offer: wl_data_offer::WlDataOffer,
    at: (f32, f32),
    /// Whether the offer named `text/uri-list` and was accepted.
    ///
    /// A drag we declined is still held, and this is why: the offer is
    /// ours to destroy whatever we said to it, and one dropped on the
    /// floor here is an object leaked on winit's connection every time
    /// somebody drags a paragraph of text across the window.
    taking: bool,
}

/// What an offer has said about itself. Kept on the object because the
/// `offer` events arrive *before* the `enter` that says which surface they
/// are for, and the `action` arrives later still.
#[derive(Default)]
struct OfferData(Mutex<OfferState>);

#[derive(Default)]
struct OfferState {
    /// It named `text/uri-list`.
    uris: bool,
    /// The action the compositor settled on. `finish` may not be sent
    /// before one has arrived — that is `invalid_finish`, and an
    /// `invalid_finish` is the whole connection.
    action: Option<DndAction>,
}

/// Everything the thread keeps.
struct Watch {
    /// Our window's surface. The test that keeps every other window in this
    /// process — and every drag that is merely near ours — out.
    ours: ObjectId,
    tx: Sender<Drag>,
    wake: Box<dyn Fn() + Send>,
    manager: Option<WlDataDeviceManager>,
    /// The seats, by registry name so one going away can take its device
    /// with it.
    seats: HashMap<u32, Seat>,
    over: Option<Over>,
    stop: bool,
}

impl Watch {
    /// Ask each seat for its data device, for any that has not got one.
    ///
    /// Called from both halves of the registry, because either global may
    /// be the one that arrives second.
    fn ensure_devices(&mut self, qh: &QueueHandle<Self>) {
        let Some(m) = self.manager.clone() else { return };
        for s in self.seats.values_mut() {
            if s.device.is_none() {
                s.device = Some(m.get_data_device(&s.seat, qh, ()));
            }
        }
    }

    /// Say something, and wake the loop that is waiting to hear it.
    fn say(&mut self, what: Drag) {
        if self.tx.send(what).is_err() {
            // Nobody is listening any more: the window went. Stop rather
            // than go on talking to a closed channel.
            self.stop = true;
            return;
        }
        (self.wake)();
    }

    /// Put the drag out, destroying the offer it was carrying.
    fn clear(&mut self) {
        let Some(o) = self.over.take() else { return };
        o.offer.destroy();
        // Only if the box was ever lit. A declined drag was never
        // announced, and announcing its leaving would put out a highlight
        // nothing put on.
        if o.taking {
            self.say(Drag::Over(None));
        }
    }
}

/// The thread: bind what is needed, then read until told to stop.
fn run(conn: &Connection, ours: ObjectId, tx: Sender<Drag>, wake: Box<dyn Fn() + Send>, stop: &OwnedFd) {
    let mut queue = conn.new_event_queue::<Watch>();
    let qh = queue.handle();
    let display = conn.display();
    let _registry = display.get_registry(&qh, ());
    let mut watch = Watch { ours, tx, wake, manager: None, seats: HashMap::new(), over: None, stop: false };
    // One roundtrip for the globals, a second because a seat bound in the
    // first is a data device asked for in the first and answered in the
    // second.
    if queue.roundtrip(&mut watch).is_err() || queue.roundtrip(&mut watch).is_err() {
        return;
    }
    if watch.manager.is_none() || !watch.seats.values().any(|s| s.device.is_some()) {
        // A compositor with no data device manager, or a seat-less one: a
        // window that will never be dropped on, and nothing to wait for.
        return;
    }

    // Hand-rolled rather than `blocking_dispatch`, and this is why: a
    // thread holding a `ReadEventsGuard` while it blocks on anything other
    // than the wayland socket stalls every other thread sitting in
    // `wl_display_read_events` — winit's included, which is the window
    // frozen. The guard here is taken immediately before the poll, the poll
    // watches the socket and the stop pipe and nothing else, and everything
    // that can take its time — the transfer read, the channel, the wake —
    // happens inside `dispatch_pending`, where no guard exists.
    loop {
        if conn.flush().is_err() {
            break;
        }
        if queue.dispatch_pending(&mut watch).is_err() || watch.stop {
            break;
        }
        let Some(guard) = conn.prepare_read() else {
            // Events already queued: go round and dispatch them.
            continue;
        };
        let wayland_fd = guard.connection_fd();
        let mut fds = [rustix::event::PollFd::new(&wayland_fd, rustix::event::PollFlags::IN), rustix::event::PollFd::new(stop, rustix::event::PollFlags::IN)];
        if rustix::event::poll(&mut fds, None).is_err() {
            break;
        }
        let asked_to_stop = fds.get(1).is_some_and(|f| !f.revents().is_empty());
        if asked_to_stop {
            // `drop` is `wl_display_cancel_read`: the reader count has to
            // come back down or every other thread waits on us for ever.
            drop(guard);
            break;
        }
        if guard.read().is_err() {
            break;
        }
    }
    // Whatever happened, the compositor is owed a tidy exit: an offer left
    // undestroyed is an object leaked on a connection that outlives us.
    if let Some(o) = watch.over.take() {
        o.offer.destroy();
    }
    for s in watch.seats.values() {
        release(s);
    }
    let _ = conn.flush();
}

/// Give a seat and its device back, where the versions have a way to.
fn release(s: &Seat) {
    if let Some(d) = s.device.as_ref() {
        if d.version() >= 2 {
            d.release();
        }
    }
    if s.seat.version() >= 5 {
        s.seat.release();
    }
}

/// The paths a `drop` is carrying, read off the pipe the other client
/// writes into.
fn slurp(conn: &Connection, offer: &wl_data_offer::WlDataOffer) -> Vec<std::path::PathBuf> {
    let Ok((read, write)) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC) else {
        return Vec::new();
    };
    offer.receive(URI_LIST.to_owned(), write.as_fd());
    // Mandatory, and the classic omission: without it the request sits in
    // this connection's buffer, the sender never learns anything is wanted,
    // and the read below waits for a write that will never come.
    if conn.flush().is_err() {
        return Vec::new();
    }
    // Ours must go or the read never sees the end of the file.
    drop(write);

    let mut body = Vec::new();
    let deadline = std::time::Instant::now().checked_add(TRANSFER_BUDGET);
    let mut chunk = [0_u8; 4096];
    loop {
        let left = deadline.and_then(|d| d.checked_duration_since(std::time::Instant::now()));
        let Some(left) = left else { break };
        let timeout = rustix::event::Timespec { tv_sec: i64::try_from(left.as_secs()).unwrap_or(i64::MAX), tv_nsec: i64::from(left.subsec_nanos()) };
        let mut fds = [rustix::event::PollFd::new(&read, rustix::event::PollFlags::IN)];
        match rustix::event::poll(&mut fds, Some(&timeout)) {
            // Out of time. A sender that has said nothing in two seconds is
            // not going to, and the window is not going to wait on it.
            Ok(0) => break,
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => break,
        }
        match rustix::io::read(&read, &mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                let Some(part) = chunk.get(..n) else { break };
                body.extend_from_slice(part);
                if body.len() >= MAX_LIST {
                    // Not a list of files any more, whatever it is.
                    break;
                }
            }
            Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => {}
            Err(_) => break,
        }
    }
    crate::uris::paths(&body)
}

// ------------------------------------------------------------- dispatch

impl Dispatch<wl_registry::WlRegistry, ()> for Watch {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, (): &(), _conn: &Connection, qh: &QueueHandle<Self>) {
        match event {
            wl_registry::Event::Global { name, interface, version } if interface == "wl_data_device_manager" => {
                // Never 4: its one addition is a source-side affordance
                // this client has no use for, and every version bound is
                // another request that can be got wrong.
                let want = version.min(3);
                state.manager = Some(registry.bind::<WlDataDeviceManager, _, _>(name, want, qh, ()));
                state.ensure_devices(qh);
            }
            // Every seat, not the first. A machine with two is a machine
            // where the first one is as likely as not to be the one the
            // person is not using.
            wl_registry::Event::Global { name, interface, version } if interface == "wl_seat" => {
                let want = version.min(5);
                let seat = registry.bind::<wl_seat::WlSeat, _, _>(name, want, qh, ());
                state.seats.insert(name, Seat { seat, device: None });
                state.ensure_devices(qh);
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(s) = state.seats.remove(&name) {
                    release(&s);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlDataDeviceManager, ()> for Watch {
    fn event(_: &mut Self, _: &WlDataDeviceManager, _: <WlDataDeviceManager as Proxy>::Event, (): &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_seat::WlSeat, ()> for Watch {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, (): &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_data_offer::WlDataOffer, OfferData> for Watch {
    fn event(_state: &mut Self, _offer: &wl_data_offer::WlDataOffer, event: wl_data_offer::Event, data: &OfferData, _conn: &Connection, _qh: &QueueHandle<Self>) {
        let Ok(mut s) = data.0.lock() else { return };
        match event {
            wl_data_offer::Event::Offer { mime_type } => s.uris |= mime_type == URI_LIST,
            wl_data_offer::Event::Action { dnd_action } => s.action = dnd_action.into_result().ok(),
            _ => {}
        }
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for Watch {
    fn event(state: &mut Self, _device: &wl_data_device::WlDataDevice, event: wl_data_device::Event, (): &(), conn: &Connection, _qh: &QueueHandle<Self>) {
        match event {
            // The offer arrives before anything says what it is for. Its
            // mimes land in its own user data; there is nothing to do here.
            wl_data_device::Event::DataOffer { .. } => {}

            wl_data_device::Event::Enter { serial, surface, x, y, id } => {
                let Some(offer) = id else { return };
                if surface.id() != state.ours {
                    // Another window of this process. Not ours to answer,
                    // and not ours to destroy either — the device that owns
                    // it will hear its own `leave`.
                    return;
                }
                let uris = offer.data::<OfferData>().and_then(|d| d.0.lock().ok()).is_some_and(|s| s.uris);
                let at = (x as f32, y as f32);
                if uris {
                    offer.accept(serial, Some(URI_LIST.to_owned()));
                    if offer.version() >= 3 {
                        // Exactly one bit in the preferred action: two is
                        // `invalid_action`, and an `invalid_action` is the
                        // whole connection.
                        offer.set_actions(DndAction::Copy, DndAction::Copy);
                    }
                } else {
                    // Dragged text, a colour, a browser tab. Saying no is
                    // what stops a drop zone lighting for something it
                    // could never take. The offer is kept all the same,
                    // for `leave` to destroy.
                    offer.accept(serial, None);
                }
                state.over = Some(Over { offer, at, taking: uris });
                if uris {
                    state.say(Drag::Over(Some(at)));
                }
            }

            wl_data_device::Event::Motion { x, y, .. } => {
                let Some(o) = state.over.as_mut() else { return };
                let at = (x as f32, y as f32);
                if o.at == at || !o.taking {
                    return;
                }
                o.at = at;
                state.say(Drag::Over(Some(at)));
            }

            wl_data_device::Event::Leave => state.clear(),

            wl_data_device::Event::Drop => {
                let Some(o) = state.over.take() else { return };
                if !o.taking {
                    // Let go of something we said no to. Nothing to read,
                    // nothing to say, and an offer to put down.
                    o.offer.destroy();
                    return;
                }
                let paths = slurp(conn, &o.offer);
                // `finish` only inside its window: version 3 or better, and
                // only once the compositor has settled an action. Outside
                // it the compositor answers `invalid_finish`, and that
                // takes winit's connection with it.
                let settled = o.offer.data::<OfferData>().and_then(|d| d.0.lock().ok()).and_then(|s| s.action).is_some_and(|a| a != DndAction::None);
                if o.offer.version() >= 3 && settled && !paths.is_empty() {
                    o.offer.finish();
                }
                o.offer.destroy();
                // Out first, then the arrival: the box stops being lit
                // because the file landed, not as well as.
                state.say(Drag::Over(None));
                if !paths.is_empty() {
                    state.say(Drag::Dropped { at: o.at, paths });
                }
            }

            // Not a drag at all — this is the clipboard. Destroyed rather
            // than ignored: an offer left alone here is an object leaked on
            // every copy anybody makes for as long as the window is open.
            wl_data_device::Event::Selection { id: Some(offer) } => offer.destroy(),

            _ => {}
        }
    }

    event_created_child!(Watch, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, OfferData::default()),
    ]);
}

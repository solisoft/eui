// A live EUI session beside its own source, on a documentation page.
//
// Three rules, and the whole file is them:
//
//   1. Nothing is fetched until somebody asks. The module is some two
//      megabytes compressed, and a page whose argument is a byte budget
//      should not spend that on a reader who is scrolling past.
//   2. Nothing hides the poster except a frame the client says it drew.
//      The canvas is revealed over it and the poster goes at the same
//      instant, so every failure — including one nobody thought of — lands
//      on a real picture with a sentence under it, and "never a blank
//      canvas" is structural rather than a code path anyone has to
//      remember. Stopping a session puts the poster back.
//   3. One module and one session per page. Eight embeds share one
//      instantiation, and starting a second stops the first: eight sockets
//      and eight GPU surfaces is a thing the reader's fan would tell them
//      about.

// The module is served `immutable` for a year: right for bytes that never
// change, fatal for bytes that do. A deploy that replaced the client would
// never be fetched again, and the page would go on running a year-old one
// with nothing to say so.
//
// The version comes from the query the page put on *this* script's own URL,
// which the server read out of the manifest `xtask-web` writes beside the
// module. One string, stamped once, carried to all three files — and the
// page that carries it is server-rendered and never cached, which is what
// makes the rest of it safe to cache forever.
const V = new URL(import.meta.url).search;
const BASE = "/eui/";

/** The one instantiation, kept as its promise so the second caller waits on the first. */
let loading = null;
/** The session that is running, if any. */
let live = null;

const dark = window.matchMedia("(prefers-color-scheme: dark)");

function load() {
  loading ??= import(BASE + "eui_web.js" + V).then((m) =>
    m.default({ module_or_path: BASE + "eui_web_bg.wasm" + V }).then(() => m));
  return loading;
}

/** What this browser cannot do, or null if it can. Asked before a byte is fetched. */
function unsupported() {
  if (typeof WebAssembly !== "object") {
    return "This browser cannot run the EUI client. The picture is a real render, taken off the wire by the repository's own snapshot tool.";
  }
  if (!navigator.gpu && !document.createElement("canvas").getContext("webgl2")) {
    return "The EUI client needs WebGPU or WebGL2 to draw, and this browser offers neither. The picture is a real render from the native client.";
  }
  return null;
}

function fail(fig, why) {
  fig.querySelector(".demo__canvas").hidden = true;
  const poster = fig.querySelector(".demo__poster");
  if (poster) poster.hidden = false;
  const note = fig.querySelector(".demo__note");
  note.hidden = false;
  note.textContent = why;
  const run = fig.querySelector(".demo__run");
  run.hidden = false;
  run.disabled = false;
  run.textContent = "Try again";
}

// The poster follows the page, so the still image is never the wrong theme.
function dressPoster(img) {
  const wanted = dark.matches ? img.dataset.dark : img.dataset.light;
  if (wanted && img.getAttribute("src") !== wanted) img.setAttribute("src", wanted);
}

async function run(fig) {
  const stage = fig.querySelector(".demo__stage");
  const canvas = fig.querySelector(".demo__canvas");
  const note = fig.querySelector(".demo__note");
  const button = fig.querySelector(".demo__run");

  const no = unsupported();
  if (no) { button.hidden = true; note.hidden = false; note.textContent = no; return; }

  button.disabled = true;
  note.hidden = false;
  note.textContent = "Fetching the client…";

  try {
    const m = await load();
    if (live) { live.stop(); live = null; }

    // Same origin as the page, always: the server refuses a WebSocket
    // upgrade whose Origin is not its own, which is what stops a page
    // elsewhere from opening a session here.
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    const url = fig.dataset.url || `${scheme}://${location.host}/_eui/session/${fig.dataset.component}`;

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    // `ceil` and not `round`, and it is the stylesheet's `width: 100%` that
    // makes the difference visible. The client sets the canvas's CSS size
    // from the backing store it is given, `backing / dpr`, as an inline
    // style — which beats the stylesheet. So rounding *down* hands back a
    // canvas fractionally smaller than the box it is supposed to fill: at
    // 853 px and dpr 1.65, `round(1407.45)` is 1407 and the canvas comes
    // back 852.727 px, leaving a sliver of the stage showing along the
    // right edge and the bottom. Rounding up can only overshoot, and the
    // stage is `overflow: hidden`, so the excess is a fraction of a pixel
    // nobody sees.
    canvas.width = Math.max(1, Math.ceil(stage.clientWidth * dpr));
    canvas.height = Math.max(1, Math.ceil(stage.clientHeight * dpr));
    canvas.hidden = false;
    // Laid out, so the client can measure it — but not yet *shown*. The
    // canvas sits over the poster, and until the session has drawn its first
    // frame it has nothing on it: revealing it here would replace a real
    // render of the application with an empty rectangle, which is the one
    // thing this file exists to prevent.
    canvas.style.opacity = "0";
    // Two frames, so the client measures a canvas the page has actually laid
    // out. `hidden = false` only schedules that; reading a width in the same
    // turn can still answer zero, and a window opened at zero is a session
    // drawn into one pixel.
    //
    // Raced against a timer, because `requestAnimationFrame` does not fire in
    // a hidden tab at all. Without the race a reader who opened this in a
    // background tab — or who switched away while two megabytes arrived —
    // waits for ever on a frame that is never painted, and the button says
    // "Fetching the client…" until the page is closed. The fallback is cheap
    // and the measurement is still sound: a hidden tab has laid the canvas
    // out, it simply is not drawing it, and the client falls back to the
    // parent's box and then to a default if it ever reads a zero.
    await Promise.race([
      new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))),
      new Promise((r) => setTimeout(r, 250)),
    ]);
    note.textContent = "Connecting…";

    // Revealed when the client says it has drawn, and not on a timer.
    //
    // A timer looked right and was wrong in the one case the poster exists
    // for: a session that never connects would uncover an empty canvas after
    // however long the timer was, which is exactly the blank rectangle the
    // still is there to prevent. The client dispatches `eui:frame` on its
    // canvas at its first paint; until that arrives the reader goes on
    // looking at a real render of the application.
    //
    // Armed before `start`, because on a warm cache the first frame can land
    // in the same turn.
    // The poster goes here and only here. An EUI surface is composited
    // with alpha — a node draws a background only where it has one (08 §3),
    // so the canvas is see-through wherever the interface is not — and a
    // still of the same component underneath it therefore shows *through*
    // the live render rather than behind it. The page ghosts: every label
    // twice, a pixel or two apart, because the still was taken at a
    // different size. It reads as a rendering bug in EUI, which is the
    // worst thing a page arguing for EUI could do.
    const poster = fig.querySelector(".demo__poster");
    canvas.addEventListener(
      "eui:frame",
      () => {
        note.hidden = true;
        button.hidden = true;
        canvas.style.opacity = "";
        if (poster) poster.hidden = true;
      },
      { once: true },
    );

    // Nothing is granted. A documentation page may not ask a reader for a
    // camera, and the server's manifest requests none either.
    m.start(canvas.id, url, "");

    live = {
      stop: () => {
        canvas.hidden = true;
        canvas.style.opacity = "0";
        // Back to the picture it was covering, or starting a second embed
        // would leave the first one an empty box — the blank canvas again,
        // by the one route that does not go through `fail`.
        if (poster) poster.hidden = false;
        button.hidden = false;
        button.disabled = false;
      },
    };
  } catch (e) {
    fail(fig, `${e && e.message ? e.message : e}`);
  }
}

for (const fig of document.querySelectorAll("[data-eui]")) {
  const img = fig.querySelector(".demo__poster");
  if (img) {
    dressPoster(img);
    dark.addEventListener("change", () => dressPoster(img));
  }
  fig.querySelector(".demo__run")?.addEventListener("click", () => run(fig));
}

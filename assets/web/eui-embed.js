// A live EUI session beside its own source, on a documentation page.
//
// Three rules, and the whole file is them:
//
//   1. Nothing is fetched until somebody asks. The module is some two
//      megabytes compressed, and a page whose argument is a byte budget
//      should not spend that on a reader who is scrolling past.
//   2. The poster is never removed — the canvas is *revealed* over it. So
//      every failure lands on a real picture with a sentence under it, and
//      "never a blank canvas" is structural rather than a code path anyone
//      has to remember.
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
    canvas.width = Math.max(1, Math.round(stage.clientWidth * dpr));
    canvas.height = Math.max(1, Math.round(stage.clientHeight * dpr));
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
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    note.textContent = "Connecting…";

    // Nothing is granted. A documentation page may not ask a reader for a
    // camera, and the server's manifest requests none either.
    m.start(canvas.id, url, "");

    live = {
      stop: () => {
        canvas.hidden = true;
        canvas.style.opacity = "0";
        button.hidden = false;
        button.disabled = false;
      },
    };
    // Long enough for a Welcome and a first Batch on a loopback or a fast
    // link, and the poster is what shows until then. A slower one reveals a
    // frame or two late, which reads as the page settling rather than as
    // anything wrong.
    window.setTimeout(() => {
      note.hidden = true;
      button.hidden = true;
      canvas.style.opacity = "";
    }, 600);
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

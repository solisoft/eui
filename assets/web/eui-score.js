// <eui-score> — music engraved in the page, read only.
//
//   <script type="module" src="/eui-score/eui-score.js?v=…"></script>
//   <eui-score src="/scores/frere.ly"></eui-score>
//   <eui-score paper="a4" page="1">{ \clef treble c'4 d'4 e'4 f'4 }</eui-score>
//   <eui-score><script type="text/x-lilypond">{ <c' e' g'>2 }</script></eui-score>
//
// The engraver the EUI client and the catalogue's `score()` share, compiled
// to WebAssembly (`crates/eui-score-web`), drawing SVG in `currentColor`: no
// session, no server, no GPU. The source is the file `src` names, else a
// `<script type="text/x-lilypond">` inside the element, else its text — and
// a chord's `<c' e' g'>` written as plain text is a tag to the HTML parser,
// so a source with chords goes in the script, or in a file. The music is
// laid out to the element's width and laid out again when that changes.
//
// Attributes, all optional:
//   src          a URL to fetch the LilyPond source from, instead of the text
//   paper        "a4" for A4 sheets scaled to the width; a screen otherwise
//   staff-space  px between two staff lines on a screen; 8 when absent
//   page-height  px a screen page may take; one page as tall as the music when absent
//   page         the page drawn, from 0
//
// It sets `data-pages` and fires `eui-score:engraved` ({ pages, page,
// refusal }) after each drawing, so a page can build its own pager.

// The module is served immutable: its version rides on this script's own
// URL (`?v=…`) and goes to the two files beside it, as eui-embed.js does.
const V = new URL(import.meta.url).search;
const BASE = new URL(".", import.meta.url).href;

/** The one instantiation, kept as its promise so the second caller waits on the first. */
let loading = null;
function load() {
  loading ??= import(BASE + "eui_score_web.js" + V).then((m) =>
    m.default({ module_or_path: BASE + "eui_score_web_bg.wasm" + V }).then(() => m));
  return loading;
}

class EuiScore extends HTMLElement {
  static observedAttributes = ["src", "paper", "staff-space", "page-height", "page"];

  constructor() {
    super();
    this.root = this.attachShadow({ mode: "open" });
    this.root.innerHTML = "<style>:host{display:block}svg{display:block;max-width:100%;height:auto}p{margin:0;font:12px/1.4 ui-monospace,monospace;color:#b91c1c}</style><div></div>";
    this.box = this.root.querySelector("div");
    this.source = null;
    this.width = 0;
    this.pending = 0;
  }

  connectedCallback() {
    this.observer = new ResizeObserver((entries) => {
      const w = Math.floor(entries[0].contentRect.width);
      if (w > 0 && w !== this.width) {
        this.width = w;
        this.schedule();
      }
    });
    this.observer.observe(this);
    this.fetchSource();
  }

  disconnectedCallback() {
    this.observer?.disconnect();
  }

  attributeChangedCallback(name) {
    if (!this.isConnected) return;
    if (name === "src") this.fetchSource();
    else this.schedule();
  }

  async fetchSource() {
    const url = this.getAttribute("src");
    try {
      const script = this.querySelector('script[type="text/x-lilypond"]');
      this.source = url ? await (await fetch(url)).text() : script ? script.textContent : this.textContent;
    } catch (e) {
      this.source = null;
      this.box.innerHTML = "";
      const p = document.createElement("p");
      p.textContent = `score: cannot fetch ${url}: ${e}`;
      this.box.append(p);
      return;
    }
    this.schedule();
  }

  // One drawing a frame, however many resizes and attributes arrive in it.
  schedule() {
    if (this.pending) return;
    this.pending = requestAnimationFrame(() => {
      this.pending = 0;
      this.draw();
    });
  }

  async draw() {
    if (this.source == null || this.width <= 0) return;
    const m = await load();
    const a4 = this.getAttribute("paper") === "a4";
    const num = (name, fallback) => {
      const v = Number(this.getAttribute(name));
      return Number.isFinite(v) && v > 0 ? v : fallback;
    };
    const drawn = m.engrave(this.source, this.width, num("staff-space", 8), a4, num("page-height", 0), Math.floor(num("page", 0)));
    const refusal = drawn.refusal;
    if (refusal) {
      this.box.innerHTML = "";
      const p = document.createElement("p");
      p.textContent = refusal;
      this.box.append(p);
    } else {
      this.box.innerHTML = drawn.svg;
    }
    const detail = { pages: drawn.pages, page: drawn.page, refusal };
    drawn.free();
    this.dataset.pages = String(detail.pages);
    this.dispatchEvent(new CustomEvent("eui-score:engraved", { detail }));
  }
}

if (!customElements.get("eui-score")) customElements.define("eui-score", EuiScore);

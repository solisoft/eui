# 06 — Events

Status: **normative** for the kinds and payloads; implemented by `eui-proto`
(kinds) and `eui-client` (emission).

An event frame is a node, a kind, the atom naming the server-side handler,
and a payload. Nothing in it is trusted until the server has validated it
against the node's schema.

```
Event := node:varint  event:u8  name:varint  payload:Value
```

## 1. Kinds and payloads

| id | Kind | Payload | Coalesced |
|---:|---|---|:-:|
| `0x01` | `click` | `List[Float x, Float y]`, local to the node the event names — the one holding the handler, not the leaf under the pointer | |
| `0x02` | `double_click` | as `click` | |
| `0x03` | `pointer_down` | `List[Float x, Float y, Int button]` | |
| `0x04` | `pointer_up` | as `pointer_down` | |
| `0x05` | `pointer_move` | `List[Float x, Float y]` | per frame |
| `0x06` | `pointer_enter` | `Null` | |
| `0x07` | `pointer_leave` | `Null` | |
| `0x08` | `key_down` | `List[Str key, Int modifiers]` | |
| `0x09` | `key_up` | as `key_down` | |
| `0x0A` | `text_input` | `Str`, the committed text | |
| `0x0B` | `focus` | `Null` | |
| `0x0C` | `blur` | `Null` | |
| `0x0D` | `change` | `Str`, the editable node's whole value | |
| `0x0E` | `submit` | `Null` | |
| `0x0F` | `scroll` | `List[Int x, Int y]`, the new offsets | per frame |
| `0x10` | `resize` | `List[Float w, Float h]` | per frame |
| `0x11` | `context_menu` | as `click` | |
| `0x12` | `drag_start` | as `pointer_down` | |
| `0x13` | `drag_over` | as `pointer_move` | per frame |
| `0x14` | `drop` | as `pointer_up` | |
| `0x15` | `long_press` | as `click` | |
| `0x16` | `window` | `List[Int first, Int last]`, the rows a windowed `list` needs (spec 04 §7.1), inclusive | when the range changes, once a scroll has landed |

Coordinates are logical pixels relative to the node's border box. `button` is
`0` primary, `1` secondary, `2` middle. `modifiers` is a bit set: `1` shift,
`2` control, `4` alt, `8` super. `key` is the key's name as in the W3C UI
Events `KeyboardEvent.key` value (`"Enter"`, `"a"`, `"ArrowLeft"`).

## 2. Emission rules

- A client emits an event only for a node that has a handler for that kind.
  There is no bubbling: the server composed the tree and attached handlers
  where it wanted them. A `click` on a `text` inside a button reaches the
  button because the button's handler is the nearest one on the path from the
  hit node to the root. That walk is the whole dispatch algorithm.
- `pointer_move`, `scroll`, `resize` and `drag_over` are coalesced: at most one
  per frame per node, carrying the latest value.
- `change` fires when an editable node's value settles — on blur, on `Enter`
  in a single-line field, or after 300 ms of no input. `text_input` fires per
  committed insertion and exists for local handlers; a server that subscribes
  to it across a wide-area link has misread the design.
- A `Handler::Local` runs the chunk and emits nothing. A
  `Handler::LocalThenServer` runs the chunk, then emits.

## 3. What the client will not report

- Keystrokes outside a focused editable node, other than to a node that
  explicitly holds a `key_down` handler and has focus. There is no global key
  capture.
- An input method's composition in progress. The client shows the preedit
  in the focused field and reports nothing; the committed text arrives as
  one `text_input`, and a composition abandoned by a blur leaves no trace.
- `Tab`, `Shift+Tab` and `Escape`: they move or drop focus (spec 03 §3) and
  are consumed by the client. So are the scrolling keys — the arrows, page
  keys, `Home` and `End` — outside an editable node: they scroll, and only
  the resulting `scroll` is reported. `Enter` and `Space` on a focused activatable
  node arrive as the `click` they stand for, at the node's centre.
- Pointer position while the window is unfocused or the pointer is outside it.
- Clipboard contents without the `clipboard.read` capability.
- Anything about the machine beyond the `Viewport` frame.

## 4. Server-side validation

The server validates every event against the node it names: the node exists
in the tree it last sent, it has a handler of that kind naming that atom, and
the payload has the shape in §1. A failure is a protocol error and ends the
session. A local handler's effect is advisory; the server re-derives state
from its own model before anything is trusted.

# Events

An event frame is three fields and a payload: the node it happened on, which
event it was, and the atom naming the server-side handler.

```
Event := node:varint  event:u8  name:varint  payload:Value
```

## Kinds

| | | | |
|---|---|---|---|
| `0x01` click | `0x02` double click | `0x03` pointer down | `0x04` pointer up |
| `0x05` pointer move | `0x06` pointer enter | `0x07` pointer leave | `0x08` key down |
| `0x09` key up | `0x0A` text input | `0x0B` focus | `0x0C` blur |
| `0x0D` change | `0x0E` submit | `0x0F` scroll | `0x10` resize |
| `0x11` context menu | `0x12` drag start | `0x13` drag over | `0x14` drop |
| `0x15` long press | `0x16` window | `0x17` ended | `0x18` time update |
| `0x19` wake | `0x1A` file pick | `0x1B` file save | |

`pointer move` and `scroll` are coalesced to one event per frame. A client that
sends sixty pointer moves a second to a server across the Atlantic has misread
the design; those belong in a [local handler](/docs/views).

`file pick` carries the upload id, the file's **name** and its size — one per
file the person chose, with the bytes following as `Upload` frames. `file
save` carries the name they chose for what a node offers, and *is* the
request for the bytes. Neither ever carries a path: which directory a file
came from or went to is the person's business, and the client keeps it.

## What the server may believe

Nothing, until it has checked.

An event is validated against the schema of the node it names. A client cannot
fabricate an event on a node it was never sent, cannot claim an event kind the
node has no handler for, and cannot submit a value outside the domain the node
declared. This is ordinary server-side validation, and it applies exactly as it
would to a form post — the difference is that the node schema makes the expected
domain explicit, so the check has something to check against.

The rule that matters: **a local handler's effect is advisory**. If a local
handler decremented a counter, incremented a quantity, or marked a row selected,
the server re-derives that from its own state on the next round trip. Nothing a
client computed is trusted, and no authorisation decision is ever made locally.

## Keyboard and focus

The client owns focus. `Tab` and `Shift+Tab` walk the focusable nodes in
document order — editable fields, and any node carrying a `click` handler —
and wrap at the ends. A node focused from the keyboard, or by a server `Focus`
op, wears a 2 px ring in `focus.ring` outside its border box; a node focused
by a pointer click does not. `Enter` or `Space` on a focused button emits the
`click` it stands for, at the node's centre, so a server never distinguishes a
keyboard press from a pointer one. `Enter` in a field is `submit`; `Escape`
drops focus. None of `Tab`, `Shift+Tab` or `Escape` is ever reported. An
input method's composition is shown in the field as it is built and reported
only once committed, as one `text_input`; the candidate window is anchored
to the field. The caret, the selection and the clipboard are the client's
too: click and drag, arrows (by word with Ctrl, extending with Shift), Home
and End, Ctrl+A/C/X/V — a paste is the person's own act and arrives as
typing. The application only ever sees `text_input` and `change`.

## Touch

A server cannot tell a finger from a mouse, and should not try. There is no
touch event kind and none is reserved: the client resolves a contact into
the pointer before anything is emitted, so the same tree works on a desktop
and on a phone.

One contact is followed at a time; a second is ignored while the first is
live. What it becomes depends on what it landed on:

- A node that asked to hear `pointer_move` — a slider, a split bar, a
  scrollbar thumb — **takes** the stroke. Every move is a `pointer_move` at
  the finger, the lift is a `pointer_up`, and the view does not scroll.
- Anything else leaves it **undecided**, and the pointer stays where it
  landed. Lift it and that is a tap: `pointer_up` and `click`, so a tap
  that wobbles a few pixels still reaches what it was aimed at. Move it
  past about eight logical pixels and that is a scroll.
- On becoming a scroll, the press is **given back**: the node that took it
  gets `pointer_up` and no `click`, so a button under a scrolling thumb
  does not fire. The view then follows the finger, including the eight
  pixels of slop, and a finger still moving when it leaves the glass
  carries the view on.

Every gesture ends with hover cleared, so a tile lit on `pointer_enter`
hears `pointer_leave` — a finger leaves nothing behind it. Which is the
other half of the point: `cursor` means nothing on a touch screen, and a
control whose only affordance is hover has no touch behaviour. Give it one.

The rules are normative in [spec 06 §5](https://github.com/solisoft/eui/blob/main/spec/06-events.md).

## What the client will not report

By design, and stated so an application author does not go looking:

- Keystrokes outside a focused editable node. There is no global key capture, so
  the keylogger shape is not available.
- Clipboard contents, without the `clipboard.read` capability.
- Pointer position outside the window, or while the window is unfocused.
- Anything about the machine: no font list, no screen enumeration, no device id,
  no timezone beyond what the viewport frame carries.

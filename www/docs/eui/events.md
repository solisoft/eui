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
| `0x15` long press | | | |

`pointer move` and `scroll` are coalesced to one event per frame. A client that
sends sixty pointer moves a second to a server across the Atlantic has misread
the design; those belong in a [local handler](/docs/views).

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

## What the client will not report

By design, and stated so an application author does not go looking:

- Keystrokes outside a focused editable node. There is no global key capture, so
  the keylogger shape is not available.
- Clipboard contents, without the `clipboard.read` capability.
- Pointer position outside the window, or while the window is unfocused.
- Anything about the machine: no font list, no screen enumeration, no device id,
  no timezone beyond what the viewport frame carries.

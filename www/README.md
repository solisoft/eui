# eui/www — the public site

One page, server-rendered by Soli, describing what EUI is. It is deliberately
plain machinery: **no framework, no build step, no npm, and no JavaScript
beyond fifteen lines that light up a paragraph when you hover a byte.**

- `app/views/home/index.html.slv` — the page.
- `app/views/layouts/application.html.slv` — head, fonts, stylesheet.
- `public/css/site.css` — hand-written, with the design tokens at the top.

The documentation site is the sibling app in `../doc`.

## The rule this page follows

Every number on it comes from a measurement in this repository, and the page
says which. The twenty-two bytes in the hero are the real encoding of

```rust
Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 141, text: TextRef::Inline("1 984, 42 €".into()) }] })
```

taken from `crates/eui-proto/tests/size_budget.rs`; the field labels are the
wire format of `spec/02-wire-format.md`, byte for byte. If a number changes,
change it here too — a marketing page that drifts from its own test suite is
worse than no page.

## Running it

```sh
soli serve . --dev          # http://localhost:5011
```

The nav points at GitHub while the documentation host is not deployed; repoint
those four links when it is.

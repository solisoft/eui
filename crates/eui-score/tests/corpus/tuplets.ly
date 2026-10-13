\version "2.24.4"
\header { title = "Triolets et sextolets" }
\relative c'' {
  \time 4/4
  \tuplet 3/2 { c8 d e } \tuplet 3/2 { f g a } b4 c |
  \tuplet 3/2 { c4 b a } g2 |
  \tuplet 6/4 { c,16 d e f g a } \tuplet 6/4 { b a g f e d } c2 |
  \tuplet 3/2 4 { e8 f g a b c d c b } a4 |
  \tuplet 3/2 { g4 r8 } \tuplet 3/2 { e'8 r e } \times 2/3 { d4 c b } |
  \tuplet 3/2 { a,8 c e } \tuplet 3/2 { a c e } \tuplet 3/2 { f d b } c4 \bar "|."
}

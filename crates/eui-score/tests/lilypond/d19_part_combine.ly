\version "2.24.0"
instrumentOne = \relative c'' {
  c4 d e f | R1 | d'4 c b a | g2 g |
  e4 e f f | g8 f e d c2 | a'2 g | c1 \bar "|."
}
instrumentTwo = \relative c'' {
  c4 d e f | c2 c | R1 | g2 g |
  c,4 c d d | e2 c | f2 e | c1 \bar "|."
}
\new Staff \with { instrumentName = "Horns" } \partCombine \instrumentOne \instrumentTwo

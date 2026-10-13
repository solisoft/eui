\version "2.24.4"
\header { title = "Reprises et silences" composer = "Exemple" }
\relative c'' {
  \time 4/4 \compressEmptyMeasures
  \repeat volta 2 { c4 d e f | g2 e | f4 e d c | b1 }
  \alternative { { a4 b c d | e1 } { a,4 g f e | c'1 } }
  R1*4 | R1*12 |
  \repeat volta 2 { e4 f g a | g2 c, | d4 e f g | e1 }
  c4 e g c | R1 | c,1 \bar "|."
}

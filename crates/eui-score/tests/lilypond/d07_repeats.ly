\relative c' {
  \repeat unfold 2 { c8 d e f } |
  \repeat volta 2 { g2 a4 b } \alternative { { c2 c } { c1 } } |
  \repeat percent 2 { g4 a b c } |
  \cadenzaOn c8 d e f g a b c \cadenzaOff \bar "|."
}

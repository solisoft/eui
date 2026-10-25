\version "2.24.0"
\header { title = "autoChange" }
% One voice split between the hands at middle C.
\new PianoStaff \autoChange \relative c' {
  \time 4/4
  g4 a b c | d c b a | g8 f' e d c b a b | c4 d e2 |
}

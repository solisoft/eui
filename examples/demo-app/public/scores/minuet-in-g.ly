\header { title = "Minuet in G" composer = "Christian Petzold" }
\new PianoStaff <<
  \new Staff \with { instrumentName = "Piano" } \relative c'' { \clef treble \key g \major \time 3/4 \tempo "Moderato" 4 = 112
    d4 g,8 a b c | d4 g, g | e' c8 d e fis | g4 g, g |
    c4 d8 c b a | b4 c8 b a g | fis4 g8 a b g | a2. \bar "|." }
  \new Staff \relative c { \clef bass \key g \major \time 3/4
    g'2 a4 | b2. | c2. | b2. |
    a2. | g2. | d'4 b g | d'2 d,4 }
>>

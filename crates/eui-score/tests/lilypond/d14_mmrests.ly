\header { title = "A part's rests" }
\score {
  \compressMMRests <<
    \new Staff \with { instrumentName = "Flute" } { \time 3/4 c''2. | R2.*9 | R2.*24^"Solo" | \tempo "Allegro" R2.*3 | d''2. | R2.*2 | e''2. \bar "|." }
    \new Staff \with { instrumentName = "Oboe" } { \time 3/4 a'2. | R2.*9 | R2.*24 | R2.*3 | b'2. | c''2. d''2. | e''2. \bar "|." }
  >>
}

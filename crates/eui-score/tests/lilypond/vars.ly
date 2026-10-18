\version "2.24.0"
\header { title = "Variables" tagline = ##f }
global = { \key g \major \time 3/4 }
sopranoMusic = \relative c'' { \voiceOne d4 b g | a2 d,4 | g2. \bar "|." }
altoMusic = \relative c' { \voiceTwo b4 d d | e2 c4 | b2. }
verse = \lyricmode { Glo -- ry to God in the high -- est }
\score {
  <<
    \new Staff << \global \new Voice = "soprano" \sopranoMusic \new Voice = "alto" \altoMusic >>
    \new Lyrics \lyricsto "soprano" \verse
  >>
  \layout { }
  \midi { }
}

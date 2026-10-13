\version "2.24.4"
\header { title = "Petite pièce pour piano" composer = "Exemple" }
\score {
  \new PianoStaff \with { instrumentName = "Piano" } <<
    \new Staff = "up" \relative c'' {
      \clef treble \key g \major \time 3/4
      d4\p( b8 c d e | d4 g, g) | e'4( c8 d e fis | g4 g, g) |
      c4\< d8 c b a | b4 c8 b a g\! | a4 b8 a g fis | g2. \bar "|."
    }
    \new Staff = "down" \relative c {
      \clef bass \key g \major \time 3/4
      g2 a4 | b2. | c2 c4 | b2. |
      a2 a4 | g2 g4 | d'2 d,4 | g2. \bar "|."
    }
  >>
  \layout { }
  \midi { }
}

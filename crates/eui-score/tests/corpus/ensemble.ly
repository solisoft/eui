\version "2.24.4"
\header { title = "Choral pour trois instruments" }
\score {
  \new StaffGroup <<
    \new Staff \with { instrumentName = "Flûte" shortInstrumentName = "Fl." midiInstrument = "flute" } \relative c'' {
      \time 4/4 \key f \major
      a4 g f c' | bes a g2 | a4 bes c d | c1 |
      f4 e d c | bes a g2 | a4 g f e | f1 \bar "|."
    }
    \new Staff \with { instrumentName = "Clarinette" shortInstrumentName = "Cl." midiInstrument = "clarinet" } \relative c' {
      \time 4/4 \key f \major
      f4 e d e | f f e2 | f4 f g f | e1 |
      a4 bes f e | d c c2 | c4 e c c | c1 \bar "|."
    }
    \new Staff \with { instrumentName = "Violoncelle" shortInstrumentName = "Vc." midiInstrument = "cello" } \relative c {
      \clef bass \time 4/4 \key f \major
      f4 c d a | bes f c'2 | f,4 d' e bes | c1 |
      d4 g, bes a | g f c'2 | f,4 c' a c | f,1 \bar "|."
    }
  >>
  \layout { }
  \midi { }
}

\version "2.24.0"
\layout { \context { \Staff \RemoveEmptyStaves } }
\score {
  <<
    \new StaffGroup <<
      \new Staff \with { instrumentName = "Flute" shortInstrumentName = "Fl." } \relative c'' { c1 d e f \break R1*4 \break g1 a b c }
      \new Staff \with { instrumentName = "Oboe" shortInstrumentName = "Ob." } \relative c'' { R1*4 \break e1 f g a \break R1*4 }
    >>
    \new PianoStaff \with { instrumentName = "Piano" shortInstrumentName = "Pno." } <<
      \new Staff \relative c' { c1 d e f \break g a b c \break R1*4 }
      \new Staff { \clef bass c1 d e f \break R1*4 \break R1*4 }
    >>
  >>
}

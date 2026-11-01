\header { title = "Four-Part Chorale" subtitle = "Soprano, alto, tenor, bass" composer = "EUI Score example" }
\new ChoirStaff <<
  \new Staff \with { instrumentName = "Soprano" shortInstrumentName = "S." midiInstrument = "choir aahs" } {
    \key f \major \time 4/4 \tempo "Lento" 4 = 72
    a'4\mp a' g' f' | bes'2 a' | c''4 bes' a' g' | f'1\fermata \bar "|."
  }
  \addlyrics { Al -- le -- lu -- ia, al -- le -- lu -- ia, a -- men. }
  \new Staff \with { instrumentName = "Alto" shortInstrumentName = "A." midiInstrument = "choir aahs" } {
    \key f \major \time 4/4
    f'4\mp f' e' d' | d'2 f' | f'4 f' f' e' | c'1\fermata \bar "|."
  }
  \addlyrics { Al -- le -- lu -- ia, al -- le -- lu -- ia, a -- men. }
  \new Staff \with { instrumentName = "Tenor" shortInstrumentName = "T." midiInstrument = "choir aahs" } {
    \clef bass \key f \major \time 4/4
    c'4\mp c' c' a | bes2 c' | a4 d' c' bes | a1\fermata \bar "|."
  }
  \addlyrics { Al -- le -- lu -- ia, al -- le -- lu -- ia, a -- men. }
  \new Staff \with { instrumentName = "Bass" shortInstrumentName = "B." midiInstrument = "choir aahs" } {
    \clef bass \key f \major \time 4/4
    f4\mp f c d | g,2 f, | f4 bes, c c | f,1\fermata \bar "|."
  }
  \addlyrics { Al -- le -- lu -- ia, al -- le -- lu -- ia, a -- men. }
>>

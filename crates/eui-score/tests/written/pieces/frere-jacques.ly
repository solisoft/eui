\header { title = "Frère Jacques" subtitle = "a round for two voices" composer = "Traditional" }
\new ChoirStaff <<
  \new Staff \with { instrumentName = "Voice 1" midiInstrument = "choir aahs" } \relative c'' {
    \time 4/4 \key g \major \tempo "Allegretto" 4 = 104
    g4 a b g | g a b g | b c d2 | b4 c d2 |
    d8 e d c b4 g | d'8 e d c b4 g | g d g2 | g4 d g2 \bar "|."
  }
  \addlyrics {
    Frè -- re Jac -- ques, frè -- re Jac -- ques, dor -- mez -- vous? Dor -- mez -- vous?
    Son -- nez les ma -- ti -- nes, son -- nez les ma -- ti -- nes, ding, daing, dong! Ding, daing, dong!
  }
  \new Staff \with { instrumentName = "Voice 2" midiInstrument = "choir aahs" } \relative c'' {
    \time 4/4 \key g \major
    R1*2 | g4 a b g | g a b g |
    b4 c d2 | b4 c d2 | d8 e d c b4 g | d'8 e d c b4 g \bar "|."
  }
  \addlyrics {
    Frè -- re Jac -- ques, frè -- re Jac -- ques, dor -- mez -- vous? Dor -- mez -- vous?
    Son -- nez les ma -- ti -- nes, son -- nez les ma -- ti -- nes,
  }
>>

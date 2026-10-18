global = { \key d \major \time 4/4 }
\new Staff <<
  \global
  \new Voice { \voiceOne \relative c'' { fis4 e d2 | e4 fis8 g a2 \bar "|." } }
  \new Voice { \voiceTwo \relative c' { d4 cis b2 | cis4 d8 e fis2 } }
>>

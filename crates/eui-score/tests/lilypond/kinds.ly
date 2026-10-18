\header { tagline = ##f }
harm = \chordmode { c2 a:m7 | f:maj7 g:7 | bes:7 fis:m7.5- | d:sus4 c/e | r1 | c1:dim7 | c1 | c1 }
mel = \relative c'' { e2 c | a4 b c d | g,1 | f'4 e d c | d2 fis | e1 | r1 b1 }
<<
  \new ChordNames \harm
  \new Staff \mel
  \new DrumStaff \drummode { hh8 hh sn hh bd4 sn | <hh bd>8 hh hho hhc cymc4 tomh8 toml | tomfh4 tomfl r2 | cymr4 rb ss hhp | bd1 | bd1 | bd1 | bd1 }
  \new TabStaff \relative c { c4 e g c | e c' g e, | <c e g>1 | a4 c e a | d,2 fis | e,1 | r1 b'1 }
>>

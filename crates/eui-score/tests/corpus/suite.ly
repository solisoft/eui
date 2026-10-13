\version "2.24.4"
\header {
  title = "Petite suite en cinq humeurs"
  subtitle = "en cinq mouvements, pour piano, main droite"
  composer = "Mélodie de démonstration"
  arranger = "gravé par EUI score()"
}
\relative c'' {
  \clef treble \key g \major \time 3/4 \tempo "Allegretto" 4 = 112
  d4( b8 c d e | d4 g, g) | e'4( c8 d e fis | g4 g, g) |
  c4( d8 c b a | b4 c8 b a g | fis4 g8 a b g | a2.) |
  d4( b8 c d e | d4 g, g) | e'4( c8 d e fis | g4 g, g) |
  c4 d8 c b a | b4 c8 b a g | a4 b8 a g fis | g2. \bar "||"
  \key bes \major \time 4/4 \tempo "Andante" 4 = 72
  f4( d8 es f4 bes) | a8 g f es d4 r | <d f>2 <es g>4. f8 | f1 |
  bes4. a8 g4 f | es16 d es f g f es d c4 r | d4 es f g | <d f bes>1 \bar "||"
  \key d \major \time 6/8 \tempo "Vivace" 8 = 160
  a8 d fis a fis d | e cis a e' cis a | d fis a d a fis | e4. r4. |
  g8 fis e d cis b | a b cis d e fis | g4. ~ g8 fis e | d4. r4 r8 \bar "||"
  \key f \major \time 5/4 \tempo "Maestoso" 4 = 84
  f,2 a4 c d | c2. bes4 a | g4( a bes2.) | a1 r4 |
  <f a c>2 <g bes d>2 <a c f>4 | bes4 a g f e | f2. ~ f2 |
  c'4( d e f g) | a2 g4 f e | d4 e f g a | bes2. a4 g | f1 r4 \bar "||"
  \key a \major \time 2/2 \tempo "Presto" 2 = 96
  a8 cis e a e cis a cis | b d fis b fis d b d | cis16 d e fis gis a gis fis e8 d cis b | a2 r |
  e'4 cis8 a e'4 cis8 a | fis'4 d8 b fis'4 d8 b | e8 d cis b a gis fis e | a2 r |
  a8( b cis d e fis gis a) | gis fis e d cis b a gis | a4 cis e a | <a, cis e a>1 \bar "|."
}

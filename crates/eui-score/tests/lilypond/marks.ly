\header { tagline = ##f }
\relative c'' {
  \time 4/4
  \mark \default
  c4-1 d-2\( e-3( f-4) | g2\trill e4\prall c\mordent\) |
  <c, e g c>2\arpeggio <d f-3 a-5>2 |
  \mark \default
  c2\glissando g'2 | c,4:16 d8:32 e8 f2:32 |
  \ottava #1 c''4 d e f | g1 \ottava #0 |
  c,,4^\markup { \italic dolce } d_\markup { \bold "f" sub. } e^"cresc." f |
  c1\turn \mark \markup { \musicglyph "scripts.segno" } |
  c1:32 \bar "|."
}

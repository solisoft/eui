\header { title = "Trills and glissandi" }
\relative c'' { \time 4/4 \key g \major
  g2\trill a4 b | c2\startTrillSpan d4 e\stopTrillSpan |
  c,4\glissando c'4 g2\glissando | d1\startTrillSpan | e2 fis2\stopTrillSpan |
  g,4\glissando g'4 \acciaccatura fis8 g2 \bar "|." }

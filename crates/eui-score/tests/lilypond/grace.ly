\header { tagline = ##f }
\relative c'' {
  \key d \major
  \acciaccatura d8 c4 \appoggiatura e8 d4 \grace { c16 d } e4 \slashedGrace f8 e4 |
  \grace { b16 cis d } e2 \acciaccatura { fis16 } g2 |
  << { \grace a16 b4 c d e } \\ { g,4 a b c } >> |
  \grace c,8 r4 \grace { a'16[ b] } c2. \bar "|."
}

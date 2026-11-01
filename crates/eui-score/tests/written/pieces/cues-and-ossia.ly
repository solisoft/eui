\addQuote "flute" { c''4 d'' e'' f'' | g''8 a'' b'' c''' g''2 | e''1 }
\new Staff = "main" \with { instrumentName = "Clarinet" } {
  R1 | \cueDuring "flute" #UP { R1 } |
  << { c'4 e' g' c'' } \new Staff \with { alignAboveContext = "main" } { c''4 e'' g'' e''' } >> |
  c''1 \bar "|."
}

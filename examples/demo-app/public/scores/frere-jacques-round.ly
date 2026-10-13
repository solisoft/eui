\header { title = "Frère Jacques" subtitle = "a round in three voices" composer = "Traditionnel" }
\new ChoirStaff <<
  \new Staff \with { instrumentName = "1" } { \clef treble \key c \major \time 4/4 \tempo "Moderato" 4 = 100
    c'4 d' e' c' | c' d' e' c' | e' f' g'2 | e'4 f' g'2 |
    g'8 a' g' f' e'4 c' | g'8 a' g' f' e'4 c' | c' g c'2 | c'4 g c'2 |
    c'4 d' e' c' | c' d' e' c' | e' f' g'2 | e'4 f' g'2 \bar "|." }
  \addlyrics {
    Frè -- re Jac -- ques, Frè -- re Jac -- ques, dor -- mez -- vous? Dor -- mez -- vous?
    Son -- nez les ma -- ti -- nes, son -- nez les ma -- ti -- nes, ding, dang, dong! Ding, dang, dong!
    Frè -- re Jac -- ques, Frè -- re Jac -- ques, dor -- mez -- vous? Dor -- mez -- vous?
  }
  \new Staff \with { instrumentName = "2" } { \clef treble \key c \major \time 4/4
    R1 | R1 | c'4 d' e' c' | c' d' e' c' | e' f' g'2 | e'4 f' g'2 |
    g'8 a' g' f' e'4 c' | g'8 a' g' f' e'4 c' | c' g c'2 | c'4 g c'2 |
    c'4 d' e' c' | c' d' e' c' }
  \addlyrics {
    Frè -- re Jac -- ques, Frè -- re Jac -- ques, dor -- mez -- vous? Dor -- mez -- vous?
    Son -- nez les ma -- ti -- nes, son -- nez les ma -- ti -- nes, ding, dang, dong! Ding, dang, dong!
    Frè -- re Jac -- ques, Frè -- re Jac -- ques,
  }
  \new Staff \with { instrumentName = "3" } { \clef treble \key c \major \time 4/4
    R1 | R1 | R1 | R1 | c'4 d' e' c' | c' d' e' c' | e' f' g'2 | e'4 f' g'2 |
    g'8 a' g' f' e'4 c' | g'8 a' g' f' e'4 c' | c' g c'2 | c'4 g c'2 }
  \addlyrics {
    Frè -- re Jac -- ques, Frè -- re Jac -- ques, dor -- mez -- vous? Dor -- mez -- vous?
    Son -- nez les ma -- ti -- nes, son -- nez les ma -- ti -- nes, ding, dang, dong! Ding, dang, dong!
  }
>>

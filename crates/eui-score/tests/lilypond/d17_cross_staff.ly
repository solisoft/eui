\version "2.24.0"
\header { title = "Cross-staff" }
% A beam over both staves (kneed), a voice switching staff inside a beam,
% slurs, a phrasing slur and a tie from one staff to the other, the
% staff-switch line, and one stem over two staves.
\new PianoStaff <<
  \new Staff = "up" {
    \time 4/4
    c'8 \change Staff = "down" g e \change Staff = "up" c'' e'' g'' c'''4 |
    c'16 \change Staff = "down" g, a, b, \change Staff = "up" c' d' e' f' g'2 |
    a'8( \change Staff = "down" g4) \change Staff = "up" e'8 ~ \change Staff = "down" e'2 |
    \change Staff = "up" \showStaffSwitch e'4\( d' \change Staff = "down" a b\) |
    \hideStaffSwitch \change Staff = "up" <e' c'>4 r4 g'2 |
  }
  \new Staff = "down" {
    \clef bass s1 s1 s1 s1
    \stemUp \crossStaff { <e g>4 } e,4 \crossStaff { g2 }
  }
>>

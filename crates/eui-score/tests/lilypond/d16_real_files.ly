\version "2.24.0"
\header { title = "Real files" encodingsoftware = "musicxml2ly" composer = \markup { \italic "Anon." } }
#(set-global-staff-size 20)
\paper { ragged-right = ##f print-page-number = ##t system-system-spacing.basic-distance = #12 }
\layout { \context { \Score autoBeaming = ##f \override BarNumber.break-visibility = ##(#f #t #t) } }
melody = \relative c'' {
  \clef "treble" \numericTimeSignature \time 4/4 \key g \major
  \override TextSpanner.bound-details.left.text = \markup \italic "rit."
  \once \override NoteHead.color = #red
  \stemDown d4\upbow c\downbow b\flageolet a\espressivo | \stemNeutral
  g4 \breathe a b \caesura c | d2\laissezVibrer d4\repeatTie r |
  \tweak color #blue e4 fis! g? \set Staff.autoBeaming = ##t a8 b |
  c4\cresc b a g\! | fis\startTextSpan e d c\stopTextSpan |
  \slurUp \tieDown \dynamicUp \autoBeamOff g8 a b c \autoBeamOn d e fis g \revert NoteHead.color | g1 \bar "|."
}
\score {
  \new Staff \with { \consists "Ambitus_engraver" instrumentName = "Flute" \override StaffSymbol.color = #grey } { \melody }
  \layout { indent = 0 }
  \midi { \tempo 4 = 100 }
}
\score {
  \new Staff { \relative c'' { \key d \major \time 3/4 \partial 4 a4 | d2 cis4 | d2. \bar "|." } }
  \header { piece = "II. Menuet" opus = "Op. 1" }
}

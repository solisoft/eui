\version "2.24.0"
\header { title = "Overrides" }
\layout {
  \context { \Score
    rehearsalMarkFormatter = #format-mark-box-letters
    \override BarNumber.break-visibility = #end-of-line-invisible
    barNumberVisibility = #(every-nth-bar-number-visible 2)
  }
}
\relative c'' {
  \numericTimeSignature \time 4/4
  \mark \default
  \slurDown g4( a b c) | \tieUp g2~ g4 r |
  \dynamicUp c4\f \once \override TextScript.extra-offset = #'(0 . 1.5) b^"dolce" a\p g |
  \override NoteHead.style = #'diamond a2 \revert NoteHead.style b4 c |
  \xNotesOn c8 b a g \xNotesOff f e d c |
  \mark \default
  \tupletDown \tuplet 3/2 { e8 f g } \omit TupletNumber \tuplet 3/2 { a g f } \undo \omit TupletNumber e2 |
  \set subdivideBeams = ##t \set baseMoment = #(ly:make-moment 1/8)
  c32 d e f g a b c b a g f e d c b c4 r |
  \override Beam.positions = #'(3 . 3) c8 e g c \revert Beam.positions \override Beam.damping = #+inf.0 g, e c g' |
  \override Script.direction = #UP c4-> d-. e-- \revert Script.direction f-^ |
  \once \override Score.RehearsalMark.self-alignment-X = #LEFT \mark \default
  \override TextScript.self-alignment-X = #CENTER g1^"centred" \bar "|."
}

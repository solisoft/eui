\version "2.24.2"

#(set-global-staff-size 16)
%\pointAndClickOff


\header { title = "telemann"
  tagline = ##f
}

\paper {
  #(define fonts
     (set-global-fonts
      ; #:music "beethoven"
      ;  #:factor (/ staff-height pt 20)
      ))
  line-width = 18.5 \cm
  ragged-last-bottom = ##f
  ragged-bottom = ##f
  top-margin = 1.5\cm
  bottom-margin = 1.5\cm
  two-sided = ##t
  inner-margin = 15 \mm
  first-page-number = 2
  %min-systems-per-page = 5

  %page-count = 3

  oddHeaderMarkup = \markup\fill-line {
    \null
    %\unless \on-first-page

    \override #'(font-name . "Garamond Premr Pro")
    \fontsize #3
    \fromproperty #'page:page-number-string
  }
  evenHeaderMarkup =  \markup\fill-line {
    %\unless \on-first-page

    \override #'(font-name . "Garamond Premr Pro")
    \fontsize #3
    \fromproperty #'page:page-number-string
    \null
  }
}

global = {
  \key c \major
  \time 4/4
  \partial 8
}

flute = \relative c''' {
  \global
  g8 a4 a a8 g r c f,4 f f8 e r e' d g r c,  b e r d~d c16 b c8 a r16 g a b c d e fis
  % 5

  g8 fis16 e d8 c b e r g, fis c' r b~b a16 g a8 fis! g4 r r2 r8 e'16\p d e4 r2 r8 c16 b c4 r8 g[ fis a]
  % 10

  r8 b[ a c] b a16 g a8 fis g4 r r16 g a b c d e fis g8 b, a e' r fis,[ g g'] r e,[ c' a] g4 r8 a\f bes4 bes bes r8 d
  % 15

  c4 bes a16( g) f8 r4 r2 r8 bes16\p a bes!4 r2 r8 a16 g a4 r8 a[ b d] r c[ d f] e c16 b c8 e, f16 g a g f e d c
  % 20

  d8 b' c a c r b r c4 r r2 r8 b16 a b8 g r b[ b d] r c[ d f] e c16 b c8 e, f16 g a g f e d c d8 b' c c,
  % 25

  r2 r16 c\f d e f g a b c8 bes r a16 g a f e f g a b c d8 c r b16 a b g fis g a b c d e8 f r e16 d e f g4 f16 e d c b a g8 b c4\fermata r
  % 30

  r8 e[(\p d c)] b( a) g( c) b( e) d( c) b( a) g( fis) g a b( c) d( e) f( e) d( c) b( c) a( gis) a b c( b) c( d) c( b) a( b)
  % 35

  gis4. a8 a2 r8 b cis( d) e( f) g( f) e( d) e( cis) d( cis) d f e( d) cis( d) e( f) g( f) e( d) e( cis) d( cis!) d4
  % 40

  R1*6 r2 r4 r8 \bar "||"
}

sopranoVoice = \relative c'' {
  \global
  \autoBeamOff
  r8 R1*6
  % 7

  r4 r8 g a4 a a8[ g] r c f,4 f f8 e r e' d g r c,(
  % 10

  b8) e r d~d[ c16 b c8] a r16 g[ a b] c[ d e fis] g8[ fis!16 e d8] c b e r g,( fis) c' r b~b[ a16 g a8] fis g4 r R1
  % 15

  r2 r4 r8 a bes4 bes bes r8 d c4 bes a16[ g] f8 r g a c r b( c) e r d~d[ e16 d e8] c r f16[ e d c b a]
  % 20

  g8 g' b, f' r e r d r c16[ b c8] e d4 c b16[ a] g8 r g a c r b( c) e r d~d[ e16 d e8] c r f16[ e d c b a] g8 g' c, f
  % 25

  e4 d c r R1*3 r2 r4\fermata r8 e
  % 30

  d[( c)] b[( a)] g[( c)] b[( e)] d[ c] b[ a] g[( fis)] g a b[( c)] d[( e)] f[( e)] d[ c] b[( a gis)] e' c[( b)] c d e[( d)] e[( f)] e[( d)] c[( d)]
  % 35

  b4. a8 a4 r8 b cis[( d)] e[( f)] g[( f)] e[( d)] cis[( b a)] e' f[( e)] f d cis[( d)] e[( f)] g[( f)] e[( d)] cis[( b a)] e' f[ e] d c
  % 40

  b8[( c)] b4 r b8 e dis4. e8 e[ g16 fis] e8[ d] cis[ e16 d] cis!8[ b] ais4 b8 cis! ais4. b8 a2~a4 b8 a g[ a] b4
  % 45

  r4 e8 e, g4( fis8.) e16 e2 r4 r8
}

% ﬅ ſ ss ﬋ si ﬈ ct ﬇ ę sp 﫥


verse = \lyricmode {
  Zu gu -- ter Nacht, ihr al -- ten Sün -- den!
  mein Herz bricht eu -- er Band __ ent -- zwey, __ _ _
  mein Herz bricht eu -- er Band __ ent -- zwey.
  Zu gu -- ter Nacht, ihr al -- ten Sün -- den!
  mein Herz bricht eu -- er Band __ ent -- zwey, __ _
  bricht eu -- er Band ent -- zwey,
  ihr al -- ten Sün -- den!
  mein Herz bricht eu -- er Band __ ent -- zwey, __ _
  bricht eu -- er Band ent -- zwey.

  Ich will, nach GOt -- tes Wohl -- ge -- fal -- len,
  in ei -- nem neu -- en Le -- ben wal -- len;
  ſein Geiﬅ ﬅeht mei -- nem Gei -- ﬅe bey;
  biß ich in je -- ner neu -- en Er -- de,
  voll -- kom -- men neu und hei -- lig wer -- de;
  denn ﬈e -- he! dort iﬅ al -- les neu, __ _ _ _ _
  dort iﬅ al -- les neu; __
  ﬈e -- he. ﬈e -- he! dort iﬅ al -- les neu.
}

bcMusic = \relative c {
  \global
  c8 f4 f, c' r8 a d4 g, c r8 c' b4 a g fis g c,8 d g,4 a
  % 5

  b4 r8 a g4 cis d g c, r8 d g,4 r8 e' f4 f, c' r8 a d4 g, c r8 c' b4 a
  % 10

  g4 fis g c,8 a g4 a

  <<
    { b4 }
    \context Ossia
    \with
    {
      \omit TimeSignature
      alignBelowContext = #"bc"
      \magnifyStaff #2/3
      firstClef = ##f
      \clef bass
    }
    {
      \once \override TextScript.X-offset = #-7 \once \override TextScript.Y-offset = #-0.5
      a4*1/4^\markup { \fontsize #2 "Orig." } \stopStaff
    }
  >>


  r8 a g4 cis d e c r8 d g,4 r g' f e r8 d
  % 15

  e4 c f r8 d g4 f e r8 d e4 c f r f g a b c a d r
  % 20

  b8 g a f g g, g' f e4 r8 c b g' a, fis' g4 r8 e f4 g a b c a d r b8 a a f
  % 25

  g8 f g g, c4 r e c f r fis d g r gis a8 b c4 e,8 f g4 g, c,\fermata r
  % 30

  g'2~g-!~g-!~g-!~
  \shape #'((0.2 . 1.8) (2 . 1.5) (2 . 2.4) (0 . 1.8)) Slur
  g(-! gis)~gis a~a-!~a-!
  % 35

  e'4 e, a2~a-!~a-!~a-! d a~a-!~a-! d
  % 40

  dis4 r e8 fis g e c' a b b, e e, e' fis g fis g e fis! e d e fis e fis! fis, cis' e dis cis! b cis dis b e4 e,8 fis
  % 45

  g8 fis g c b2 e, r4 r8
  \jump  \markup {\override #'(font-name . "Garamond Premr Pro") \fontsize #2 "Da Capo."} \bar "||"
}

bcFigures = \figuremode {
  \global
  s8 s2 <6 4>8 <5 3> s4 <7> <7> <5 4>8 <3> s4 <6> <6\\> <5>8 <6> <7> <6> s4
}

flutePart = \new Staff \with {
  instrumentName = ""
  \override VerticalAxisGroup.default-staff-staff-spacing =
  #'((basic-distance . 0)(padding . 10 )) % ajuster entre  flute et soprane

} {\clef french \flute}

sopranoVoicePart = \new Staff \with {
  instrumentName = ""
  \override VerticalAxisGroup.default-staff-staff-spacing =
  #'((basic-distance . 10)(padding . 10 )) % ajuster entre soprane et BC

} { \clef treble \sopranoVoice }
\addlyrics { \verse }

bassoContinuoPart = \new Staff = "bc"
\with {
  implicitBassFigures = #'(0)
  \override VerticalAxisGroup.default-staff-staff-spacing =
  #'((basic-distance . 0)(padding . 1 )) % ajuster pour l'ossia

}
{ \clef bass << \bcMusic \bcFigures >> }

\score {
  %\new ChoirStaff

  <<
    \flutePart
    \sopranoVoicePart
    \bassoContinuoPart
  >>

  \layout {
    %indent = 3 \cm

    \context {
      \Score
      \override StaffSymbol.line-thickness = #1.55
      \override Slur.thickness = #2
      \override Tie.minimum-length = #3
      \override Stem.thickness = #1.55
      \override Hairpin.thickness = #1.5
      \override BarLine.hair-thickness = #1.9
      \override BarNumber.break-align-symbols = #'(clef left-edge staff-bar)
      \override BarNumber.X-offset = #1.5
      \override BarNumber.Y-offset = #5
      \override BarNumber.font-name = #"Garamond Premier Pro"
      \override BarNumber.font-size = #0
      \override LyricText.font-name = #"Garamond Premr Pro"
      \override LyricText.font-size = #2
      \override InstrumentName.font-name = #"Garamond Premier Pro"
      \override InstrumentName.font-size = #2
      \override BarNumber.padding = #1.0
      figuredBassPlusStrokedAlist = #'()
      figuredBassPlusDirection = #RIGHT
      %\override BassFigureAlignmentPositioning.direction = #DOWN

      \override BassFigure.font-name = #"Garamond Premier Pro"
      \override BassFigure.font-size = # -1
      \override NoteHead.style = #'baroque
      \override Custos.style = #'mensural
      \defineBarLine "||:" #'("||" "||:" " ||:")
      \defineBarLine ":||" #'(":||" "||" " ||")
      \defineBarLine ":||:" #'(":||" "||:" " ||")
      %startRepeatBarType = ".|:-||"

      %\override SpacingSpanner.common-shortest-duration = #(ly:make-moment 1/12)

    }
    \context {
      \Staff
      \consists Custos_engraver
    }

    \context {
      \Score
      %\consists "Span_bar_engraver"

      % Avoid a vertical line at the beginning of the system:

      %\remove "System_start_delimiter_engraver"

      % Incorporate the new context in the hierarchy

      \accepts "Ossia"
      \override VerticalAxisGroup.default-staff-staff-spacing =
      #'((basic-distance . 0)
         (padding . -0 ) ;ajustez pour l'ossia
         )
    }

    \context {
      \Staff
      \name "Ossia"
      alignBelowContext = #"bc"
      \alias "Staff"
      \omit TimeSignature
      \magnifyStaff #2/3
      firstClef = ##f
      \override VerticalAxisGroup.default-staff-staff-spacing =
      #'((basic-distance . 10)
         (padding . 20 ) ;ajustez pour régler tenor
         )
    }
  }
}

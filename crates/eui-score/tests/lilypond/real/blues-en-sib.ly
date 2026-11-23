\version "2.24.4"
\language "english"

\header {
  title = "Blues en Sib"
  composer = "Bireli Lagrene"
}

\paper {
  % Add space for instrument names

  indent = 35\mm
}

global = {
  \key bf \major
  \time 4/4
}

chordNames = \chordmode {
  \global
  % Ajouter ici des chiffrages.

  bf1:7 ef:7 bf:7 s
  ef:7 e:dim bf:7 g:7
  c:m7 f:7
  bf2:7 g:7 c:m7 f:7
  bf1:7 ef:7 bf:7 s
  ef:7 e:dim bf:7 g:7
  c:m7 f:7
  bf2:7 g:7 c:m7 f:7
  bf1:7 ef:7 bf:7 s
  ef:7 e:dim bf:7 g:7
  c:m7 f:7
  bf2:7 g:7 c:m7 f:7
  bf1:7 ef:7 bf:7 s
  ef:7 e:dim bf:7 g:7
  c:m7 f:7
  bf2:7 g:7 c:m7 f:7

}

electricBass = \relative c, {
  \global
  % En avant la musique.

  \acciaccatura cs''8 d4 r \acciaccatura fs,8 g bf r4
  r2 df8 g, bf ef df4 bf8 g r bf bf4
  r2 r4 bf8 c16 df
  af8 c \tuplet 3/2 { bf f df } c bf~8 a16 c
  bf4 c8 bf d f g4
  bf d8 ef d a4.
  r4 af8 df af4 fs8 g
  r c \tuplet 3/2 { bf af f } ef4 r8 d
  \tuplet 3/2 { ef f gf } r \xNote g, b'4 r8 g,
  \tuplet 3/2 { bf'4 f d8 bf } d af' g f ef c bf c16 bf a8 gf' f ef \bar "||"
  \break
  d f bf4 bf r
  r r8 df \tuplet 3/2 { c bf g } \tuplet 3/2 { e bf' e, }
  cs'16( d) bf g \xNote ef e cs8 r8. g16 df'' bf g e!
  cs g' e cs bf g \tuplet 3/2 { cs fs, bf} f e e,8 r16 \harmonicNote<d''' gf>8.~ \harmonicNote q4
  r8. e,,,16 d' bf d f a f a cs
  bf c e g r4 r8 g,8 g' e
  f d \tuplet 3/2 { bf f \xNote d' } \acciaccatura cs8 d4 r16 g, ef' \acciaccatura cs8 d16~
  16 bf g f d c b d f af g f ef d b! c
  r4 \acciaccatura as8 b d16 b c d ef g bf d f ef
  df a af g b af a d af e g e f ef c8
  r4 r16 b c d e g b, d f af g f
  ef g bf d f ef df bf af gf cf af a df cf a\break\bar "||"
  af gf e d c bf a bf c d f d ef f8.
  r8. g16 f'8 \acciaccatura e f ef16 df bf ef df bf af df
  bf af f bf af f ef af f ef df f ef df bf ef
  df bf8.~4 \tuplet 3/2 { fs8 g b } \acciaccatura b8 cs e
  r2 c16 df f af c af c c
  af f df c bf c8. r4 r8 cs
  \tuplet 3/2 { d f g  } bf c d f g gs
  a2 r16fs g8 r fs
  \tuplet 3/2 { g bf g} ef16 d c f d ef g bf g gs a af
  g gf f e ef g d b df a af gf ef8 e16 f
  \acciaccatura g8 a c16 a bf c d f bf4 r8 g
  bf bf r4 r8 g \acciaccatura b c4\bar "||"\break
  \acciaccatura b c4 r \tuplet 3/2 { <c af d,>8 <bf g cs,> <c af d,> } r4
  r2 \tuplet 3/2 { <c af d,>8 <bf g cs,> <c af d,> } r4
  r <df g, df>2 e,,,4
  r8 <fs'' a>( <gs a>4) r4 r8 g,
  a' \xNote a,, \xNote a g' bf'4 r8 df
  g, bf e, g df f bf,4
  df8 cf a bf e f a c~
  4 a16 f d c' b8 af g f
  ef g d b \tuplet 3/2 { c d ef  } g bf
  \acciaccatura ef,8 e g r4 g f8 bf~
  4 \tuplet 3/2 { g8 f d } \tuplet 3/2 { g f d } \tuplet 3/2 { df c bf }
  g f r bf r4 \tuplet 3/2 {<d af' c>8 <cs g' b> <d af' c>~  }\bar "||" q1

}

chordsPart = \new ChordNames \chordNames

electricBassPart = \new StaffGroup \with {
  systemStartDelimiter = #'SystemStartSquare
  \consists "Instrument_name_engraver"
  instrumentName = "Basse électrique"
} <<
  \new Staff \with {
    midiInstrument = "electric bass (finger)"
  } { \clef bass \transposition c \transpose c c' \electricBass }
  \new TabStaff \with {
    stringTunings = #bass-tuning
    midiInstrument = "electric bass (finger)"
  } \electricBass
>>

\score {
  <<
    \chordsPart
    \electricBassPart
  >>
  \layout { }
  \midi {
    \tempo 4=100
  }
}

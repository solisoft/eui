\header { title = "A clef and a key in the middle of a bar" }
\new PianoStaff <<
  \new Staff \relative c'' { \key d \major d4 fis a d | cis2 \key bes \major bes4 d | f1 \bar "|." }
  \new Staff { \clef bass d4 a, \clef treble fis' a' | \clef bass a,2 g,8 bes, d f | bes,1 }
>>

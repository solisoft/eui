\version "2.19.81"
#(set-global-staff-size 18)

\book {
  \paper {
    system-system-spacing.basic-distance  = #10
    print-all-headers = ##t
    top-margin = 1\cm
    right-margin = 1.5\cm
    left-margin = 1.5\cm
  }
  \header {
    title = \markup {  \override #'(font-name . "Copperplate") \fontsize # 4 \bold "Set 2"}
    subtitle = "Antiquities of Scottish Music"
     page-count = 1
 tagline = ##f
  }
  \markup { \vspace #1 }



\score {
  \new Staff  \transpose c c'    {
    \set Staff.instrumentName    = \markup { \combine \draw-circle #2.2 #.2 ##f \vcenter \halign #CENTER \huge \number  "1" }
    \key d \major \time 4/4 \partial 4
    fis4
    b4 b b cis'8. e'16 |
    fis'4 b b a8 fis |
    a4 a a8. b16 cis'8. d'16 |
    e'8 fis' e' d' cis'4 b8 a |

    d'4. d'8 e'4. e'8 |
    fis'4 a' cis' b8. cis'16 |
    a4 a' fis'8 e' d' cis' |
    b2 b4 \bar "||"

    b8. cis'16 |
    d'4 d' d'8. a16 fis8 a |
    d'8. e'16
    fis'8. g'16 a'4 e'8 d' |
    cis'4 b8 a e' 4 a |
    cis'8. d'16 e'8. fis'16 e'4 d'8 cis' |

    d'8 cis' d' e' fis'8. e'16 fis'8. b'16 |
    a'8 fis' e'8. d'16 cis'4 b8 cis' |
    a4. a'8 fis' e' d' cis' |
    b2 b4 \bar "|."
  }

  \header {
    title = \markup { \vspace #1 \override #'(font-name . "Copperplate") \fontsize # 1 \bold "I. Johnnie Cope"}
    opus = ##f
    subtitle = ##f
  }
}


\score {
  \new Staff  \transpose c c'  {
    \set Staff.instrumentName    = \markup { \combine \draw-circle #2.2 #.2 ##f \vcenter \halign #CENTER \huge \number  "2" }

    \key d \major \time 2/2 \partial 8
    \repeat volta 2 {
      cis'8 |
      d'8 fis' cis' e' d' b b cis' |
      d'8 b e' d' cis' a a cis' |
      d'8 fis' cis' e'd'  b b' fis' |
      g'8 e' cis' fis' d'[ b b] \bar ":|."
    }  \break
    cis'8 |
    d'8 fis' e' d' a' fis' e' d' |
    cis'8 a e' a fis' a e' a |

    d'8 fis' cis' e' d' b b' cis'' |
    d''8 cis'' b' ais' b'16 b' b'8 b'[ cis''] |
    d''8 cis'' b' ais' b' a' g' fis' |
    g'8 e' fis' d' cis' a e' cis' |

    d'8 b e' cis' fis' d' g' e'
    fis'8 b' cis' ais' b'16[ b8.] b8 \bar "|."
  }

  \header {
    title = \markup {  \override #'(font-name . "Copperplate") \fontsize # 1 \bold "II. Miss Hog, Newliston"}
    opus = ##f
    subtitle = ##f
  }
}



\score {
  \new Staff  \transpose c c'  {
    \set Staff.instrumentName    = \markup { \combine \draw-circle #2.2 #.2 ##f \vcenter \halign #CENTER \huge \number  "3" }
    \key d \major \time 2/2 \partial 8
    \repeat volta 2 {
      fis
      b4 b8 cis' d' e' fis' d' |
      cis'4 cis'8 a' cis' a e' cis' |
      b4 b8 cis' d' e' fis' b' |
      a'8 fis' e' cis' b16[ b b8] b
    }\break
    fis'8
    b'8 fis' b' cis'' d''4 cis''8 b' |
    a'8 e' a' b' cis''4 b'8 a'
    b'8 fis' b' cis'' d''4 cis''8 b'|
    a'8 fis' e' cis' b16[b b8] b4 |
    b'8 fis' b' cis'' b' cis'' d'' b' |
    a'8 b' cis'' a' a' fis' e' a |
    fis'8 a' b' cis'' b' a' b' cis'' |
    a'8 fis' e' cis' b16[ b b8] b \bar "|."
  }

\header {
    title = \markup {  \override #'(font-name . "Copperplate") \fontsize # 1 \bold "III. Mrs Morthland"}
    opus = ##f
    subtitle = ##f
  }
}




\score {
  \new Staff  \transpose c c'  {
    \set Staff.instrumentName    = \markup { \combine \draw-circle #2.2 #.2 ##f \vcenter \halign #CENTER \huge \number  "4" }
    \key d \major \time 2/2 \partial 8
    \repeat volta 2 {
      cis'8
      d'8e b e d' e b cis' |
      d'8 e b e fis d d cis'
      d'8 e b e g b e' b |
      cis'8 a d' a fis[ d d]
    }\break
    fis'8
    g'8 e' b' e' g'8 e' e' fis'
    g'8 e' b' e' fis' d' d' fis'
    g'8 e' b' e' g' e' fis' d'
    cis'8 a d' a fis d d fis'
    g'8 e' b' e' g' e' e' fis'
    g'8  e' b' e' fis' d' d' fis'
    e'8 b' g' b' fis' a' e' g'
    fis'8 cis' d' a fis[ d d] \bar "|."

  }

  \header {
    title = \markup {  \override #'(font-name . "Copperplate") \fontsize # 1 \bold "IV. As a Thòiseach"}
    opus = ##f
    subtitle = ##f
  }
}
}

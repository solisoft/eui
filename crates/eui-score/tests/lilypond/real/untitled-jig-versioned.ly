\version "2.27.5"
\header {
        title="Untitled Jig"
        composer="from a recording by Seamus Egan"
        encoder="Jean.Louis.Thiry"
}
PAVA = \fixed c'{
        \tempo 4 = 120
        \time 6/8
        \key g \major
        \repeat volta 2 {
                e8 e e g fis e |
                e8 b e g fis e |
                e8 g b b a g |
                a4 fis8 g fis b |
                e8 e e g fis e |
                e8 b e g fis e |
                e8 g b b a g |
                fis8 e d e4 r8|
        }
        \section
        \key d \major
        fis8 fis fis fis g a |
        a8 g fis g4. |
        b8 b b e' d' c' |
        b8 c' b a g a |
        b4 b8 d' c' b |
        b8 a b d' c' b |
        b8 d' fis' fis' e' d' |
        cis'8 b a b4 r8 |
        \section
        \key a \major
        cis'8 d' e' e' fis' e' |
        e'8 d' cis' b a gis |
        a4 cis'8 d' cis' b |
        cis'8 b a a a g |
        fis8 g a a g fis |
        g8 a b b4 r8 |
        b8 cis' dis' e' b a |
        g8 fis g b g fis \bar "|." |
}
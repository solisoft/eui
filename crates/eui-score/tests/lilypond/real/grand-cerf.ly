\version "2.25.18"
\pointAndClickOff
\include "english.ly"
%------------------------------------------------------------------------
\header {
    title = \markup { \override #'(font-name . "petitamedium")  "Le grand cerf" }
   % subtitle =  \markup {\epsfile #X #20  #"lundimage.eps" }
    composer = ""
    poet = ""
    arranger = ""
    meter =""
    img = \markup {  \epsfile #X #30 #"cerf.eps"}
    tagline =   \markup { \column { \center-column { \img  \fontsize #-3 "Edition du 10 janvier 2011 by LilyPond" }}}
}

%-----------------PAPER -----------------------------------------------
#(set-global-staff-size 18)

\paper {
        #(define dump-extents #t)
        %#(set-paper-size "a5" 'landscape)
        #(set-paper-size "a5" )
        line-width = 110\mm
         indent = 0.0\mm

        system-system-spacing = #'((basic-distance . 12) (minimum-distance . 8) (padding . 1) (stretchability . 60))
        score-system-spacing = #'((basic-distance . 14) (minimum-distance . 8) (padding . 1) (stretchability . 120))
        markup-system-spacing = #'((basic-distance . 13) (padding . 0.5) (stretchability . 30))
        score-markup-spacing = #'((basic-distance . 12) (padding . 0.5) (stretchability . 60))
        markup-markup-spacing = #'((basic-distance . 1) (padding . 0.5))
        top-system-spacing = #'((basic-distance . 10) (minimum-distance . 0) (padding . 1))
        top-markup-spacing = #'((basic-distance . 2) (minimum-distance . 0) (padding . 1))
        last-bottom-spacing = #'((basic-distance . 1) (minimum-distance . 0) (padding . 1) (stretchability . 30))

        ragged-bottom =  ##t
        ragged-last-bottom =  ##t
        ragged-right = ##f

        page-count = #1
}
%---------------- GLOBAL-DEFINITIONS ----------------------------
globaldef={
    \clef treble
    \key f \major
    \time 2/4
   % \set Score.proportionalNotationDuration = \musicLength 4
    \set Score.barNumberVisibility = ##f
}

\layout {
    \context {
        \Score
        \override MetronomeMark.stencil = ##f %% on supprime l'indication de temp
        %dont view the dynamics for the midi output
        \override DynamicText.stencil = ##f


    }

    \context {  %propriété des Lyrics
                \Lyrics
                \override LyricText.font-name = "petitamedium"
                \override LyricText.font-size =  #-2.5 %%taille des caractères
                 \override LyricText.minimum-Y-extent = #'(-1.0 . 1.0)
            }
}

%----------------- VOICE ---------------------------------------------
Voix = \relative c' {
            \set Timing.beamExceptions = #'()
                r4 c8 f8
                f8. g16 f8 e8
                g4 c8 g8
                g8. a16 g8 f8
                a8 a8 c,8  f8
                f8. g16 f8 e8
                \time 3/4
                g4 c,8[ c8] d8[ e8]
                \time 2/4
                f4 c'4
                c4 c8 bf8
                d4 bf8 bf8
                bf8 bf8 bf8 a8
                c4 a8 a8
                a8 a8 a8 g8
                \time 3/4
                d'4 c,8[ c8] d8[ e8]
                f2.
                \bar "|."
                }

%----------------- LIRICS ---------------------------------------------
LI = \lyricmode {
Dans sa mai -- son un grand cerf
re -- gar -- dait par la fe -- nê -- tre
un la -- pin ve -- nir à lui
et frap -- per ain -- si:

Cerf, cerf,
ou -- vre -- moi!
ou le chas -- seur me tue -- ra!
La --  pin la -- pin
entre et viens,
me ser -- rer la main.
}

LII = \lyricmode {

}

LIII = \lyricmode {

}


%----------------- VOICE ---------------------------------------------

Voix = \context ChoirStaff = "V"
        <<
        \new Voice     <<
                \context Voice = V { << \globaldef  \Voix >>  \stopStaff  }
            %    \new Lyrics \lyricsto "V" {   \LI }
                %\new Lyrics \lyricsto "V" {   \LII }
                %\new Lyrics \lyricsto "V" {   \LIII }
                >>
        >>
%----------------- VOICE ---------------------------------------------
\book {
    \score {
        \Voix
    \layout {}
    }

%----------------------------------------------------------------------
\markup {
    \vspace #1
    \hspace #10
    \override #'(font-name . "petitamedium")
    \fontsize #-1
    \column    {
        "Dans sa maison un grand cerf"
        "regardait par la fenêtre    "
        "un lapin venir a lui        "
        "et frapper ainsi.           "
        "                            "
        "Cerf, cerf,                 "
        "ouvre moi !                 "
        "ou le chasseur me tuera !   "
        "Lapin lapin                 "
        "entre et viens,             "
        "me serrer la main.          "
        }

    %\column {\hspace #15 \epsfile #X #20 #"lapin3.eps"  }
    \column { " "
    {\hspace #5 \epsfile #X #10 #"lapin.eps" }
    }
    }
}
%------------------------------------------------------------------------

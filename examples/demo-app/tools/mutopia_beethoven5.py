#!/usr/bin/env python3
"""Mutopia's Beethoven 5 (1st movement, public domain) reduced to the LilyPond
the EUI score editor reads: one StaffGroup, a staff per part with its name
and MIDI instrument, absolute pitches, concert pitch — what the editor's
Examples menu loads (public/scores/beethoven-symphony-5-1.ly).

    curl -fsSLO https://www.mutopiaproject.org/ftp/BeethovenLv/O67/Symphony5_1/Symphony5_1-lys.zip
    unzip Symphony5_1-lys.zip
    tools/mutopia_beethoven5.py Symphony5_1-lys 124 > public/scores/beethoven-symphony-5-1.ly

124 bars is the exposition, its repeat included. Typeset by Stelios Samelis
(adapted by Johannes Heinecke), placed in the public domain by the
typesetter (Mutopia-2017/11/05-941). What the editor does not read is
reduced: a second voice on a staff, the words, the tremolos written out,
the tenor clef as bass, the clarinets and horns at concert pitch.
"""
import re
import sys
from fractions import Fraction

PARTS = [
    # file, variable, name, short, transposition (diatonic, semitones) to concert
    ("flauti.ly", "flauti", "Flauti", "Fl.", None),
    ("oboi.ly", "oboi", "Oboi", "Ob.", None),
    ("clarinetti.ly", "clarinetti", "Clarinetti", "Cl.", (-1, -2)),  # in B flat
    ("fagotti.ly", "fagotti", "Fagotti", "Fg.", None),
    ("cornies.ly", "cornies", "Corni", "Cor.", (-5, -9)),  # in E flat
    ("trombec.ly", "trombec", "Trombe", "Tr.", None),  # in C
    ("timpani.ly", "timpani", "Timpani", "Timp.", None),
    ("violinoone.ly", "violinoone", "Violino I", "Vl. I", None),
    ("violinotwo.ly", "violinotwo", "Violino II", "Vl. II", None),
    ("viola.ly", "viola", "Viola", "Va.", None),
    ("violoncello.ly", "violoncello", "Violoncello", "Vc.", None),
    ("basso.ly", "basso", "Basso", "Cb.", None),
]


# What each part sounds as: Mutopia gave the horns and trumpets an English horn.
MIDI = {"cornies": "french horn", "trombec": "trumpet"}


def strip_comments(text):
    text = re.sub(r"%\{.*?%\}", " ", text, flags=re.S)
    out = []
    for line in text.split("\n"):
        # A % outside a string starts a comment.
        in_str = False
        for i, ch in enumerate(line):
            if ch == '"':
                in_str = not in_str
            elif ch == "%" and not in_str:
                line = line[:i]
                break
        out.append(line)
    return "\n".join(out)


def balanced(text, start, open_ch="{", close_ch="}"):
    """Index just past the brace group opening at `start`."""
    depth = 0
    i = start
    in_str = False
    while i < len(text):
        ch = text[i]
        if ch == '"':
            in_str = not in_str
        elif not in_str:
            if text.startswith(open_ch, i):
                depth += 1
                i += len(open_ch)
                continue
            if text.startswith(close_ch, i):
                depth -= 1
                i += len(close_ch)
                if depth == 0:
                    return i
                continue
        i += 1
    raise ValueError("unbalanced")


def variable(text, name):
    m = re.search(r"\b" + name + r"\s*=\s*\{", text)
    end = balanced(text, m.end() - 1)
    return text[m.end():end - 1]


def drop_groups(text, pattern):
    """Remove `pattern` followed by a brace group (a \\markup { … })."""
    while True:
        m = re.search(pattern, text)
        if not m:
            return text
        j = m.end()
        while text[j].isspace():
            j += 1
        if text[j] == "{":
            end = balanced(text, j)
        else:
            end = re.match(r'"[^"]*"|\S+', text[j:]).end() + j
        text = text[:m.start()] + " " + text[end:]


def first_voice(text):
    """`<< { a } \\\\ { b } >>` becomes `a`: the editor writes one voice."""
    while True:
        m = re.search(r"<<\s*\{", text)
        if not m:
            return text
        end = balanced(text, m.start(), "<<", ">>")
        inner = text[m.start() + 2:end - 2]
        j = inner.index("{")
        first = inner[j + 1:balanced(inner, j) - 1]
        text = text[:m.start()] + " " + first + " " + text[end:]


NATURAL = [0, 2, 4, 5, 7, 9, 11]
STEPS = "cdefgab"


def parse_pitch(name):
    step = STEPS.index(name[0])
    rest = name[1:]
    alter = {"": 0, "is": 1, "isis": 2, "es": -1, "eses": -2, "s": -1, "ses": -2}[rest]
    return step, alter


def spell(step, alter):
    name = STEPS[step]
    if alter > 0:
        return name + "is" * alter
    if alter < 0:
        if name in "ea":
            return name + "s" + "es" * (-alter - 1)
        return name + "es" * -alter
    return name


def transpose_pitch(name, marks, by):
    step, alter = parse_pitch(name)
    octave = marks.count("'") - marks.count(",")
    d, c = by
    total = step + d
    new_step = total % 7
    octave += total // 7
    have = NATURAL[new_step] + 12 * (total // 7) - NATURAL[step]
    alter += c - have
    if octave > 0:
        marks = "'" * octave
    else:
        marks = "," * -octave
    return spell(new_step, alter) + marks


PITCH = re.compile(r"(?<![a-zA-Z\\])([a-g](?:isis|eses|is|es|s)?)([',]*)(?![a-zA-Z])")


def transpose(text, by):
    # Not inside strings or command names.
    parts = re.split(r'("[^"]*"|\\[a-zA-Z]+)', text)
    for i in range(0, len(parts), 2):
        parts[i] = PITCH.sub(lambda m: transpose_pitch(m.group(1), m.group(2), by), parts[i])
    return "".join(parts)


def clean(body):
    body = re.sub(r"\\once\s*", " ", body)
    body = re.sub(r"\\override\s+[\w.]+\s*#'[\w-]+\s*=\s*(##[tf]|#'?[\w.-]+|#?\d+(\.\d+)?)", " ", body)
    body = drop_groups(body, r"[\^_-]?\\markup")
    instrument = None
    m = re.search(r'\\set\s+Staff\.midiInstrument\s*=\s*#?"([^"]*)"', body)
    if m:
        instrument = m.group(1)
    body = re.sub(r'\\set\s+[\w.]+\s*=\s*(#?"[^"]*"|##[tf]|#\'?[\w-]+)?', " ", body)
    for cmd in ["compressFullBarRests", "cadenzaOn", "cadenzaOff", "pageBreak", "break", "stopTrillSpan", "startTrillSpan"]:
        body = re.sub(r"\\" + cmd + r"(?![a-zA-Z])", " ", body)
    body = re.sub(r"\\mark\s*\\default", " ", body)
    body = re.sub(r"\\bar\s*\"\|\"", " ", body)
    body = re.sub(r"\\pizz(?![a-zA-Z])", '^"pizz."', body)
    body = re.sub(r"\\arco(?![a-zA-Z])", '^"arco"', body)
    body = re.sub(r"\\trill(?![a-zA-Z])", " ", body)
    body = re.sub(r"\\cresc(?![a-zA-Z])", r"\\<", body)
    body = re.sub(r"\\(dim|decresc)(?![a-zA-Z])", r"\\>", body)
    body = re.sub(r"\\staccato(?![a-zA-Z])", "-.", body)
    # A rest written at a pitch.
    body = re.sub(r"(?<![a-zA-Z\\])[a-g](?:isis|eses|is|es|s)?[',]*(\d*\.*)\s*\\rest", r"r\1", body)
    # Forced and cautionary accidentals.
    body = re.sub(r"(?<![a-zA-Z\\])([a-g](?:isis|eses|is|es|s)?[',]*)[!?]", r"\1", body)
    body = re.sub(r"\\context\s+Voice\s*(=\s*\"?\w+\"?)?", " ", body)
    body = first_voice(body)
    body = re.sub(r"\\tempo\s+\d+\s*=\s*\d+", " ", body)
    body = re.sub(r"\\clef\s+\"?tenor\"?", r"\\clef bass", body)
    body = same_clef_dropped(body)
    # A script on a rest (a fermata over a bar of silence): the editor
    # puts scripts on notes only.
    body = re.sub(r"(?<![a-zA-Z\\])([rR]\d*\.*(?:\*\d+)?)\s*[\^_-]\\\w+", r"\1", body)
    # A measured tremolo, `c2:8`: the eighths it stands for.
    body = re.sub(r"(?<![a-zA-Z\\])([a-g](?:isis|eses|is|es|s)?[',]*)(\d+):(\d+)((?:\\[a-zA-Z]+)*)", tremolo, body)
    # A tie on one note of a chord: the chord's own tie, after it, is the
    # one the editor reads.
    body = re.sub(r"<[^<>]*>", lambda m: m.group(0).replace("~", " "), body)
    # The same on a chord, `<ees' c''>2:8`.
    body = re.sub(r"(<[^<>]*>)(\d+):(\d+)((?:\\[a-zA-Z]+)*)", tremolo, body)
    return body, instrument


def same_clef_dropped(body):
    """A clef the staff is already in says nothing, and may fall mid-bar."""
    out = []
    current = None
    last = 0
    for m in re.finditer(r'\\clef\s+"?(\w+)"?', body):
        out.append(body[last:m.start()])
        if m.group(1) != current:
            out.append(m.group(0))
            current = m.group(1)
        last = m.end()
    out.append(body[last:])
    return "".join(out)


def tremolo(m):
    pitch, whole, each, marks = m.group(1), int(m.group(2)), int(m.group(3)), m.group(4)
    count = max(1, each // whole)
    return " ".join([f"{pitch}{each}{marks}"] + [pitch] * (count - 1))


# ---- cutting at a bar line ---------------------------------------------------

TOKEN = re.compile(
    r'"[^"]*"|\\key\s+[a-g]\w*[\',]*|\\[a-zA-Z]+|\\[!<>\\]|<<|>>|[{}<>\[\]()~|]|'
    r"(?P<event>(?:[a-g](?:isis|eses|is|es|s)?[',]*|[rRs]))(?![a-zA-Z])(?P<dur>\d+\.*(?:\*\d+(?:/\d+)?)?)?|"
    r"[a-zA-Z]+|\S"
)


def cut(body, bars, measure=Fraction(1, 2)):
    """The body up to the end of bar `bars`, the braces it opened closed."""
    now = Fraction(0)
    dur = Fraction(1, 4)
    in_chord = False
    depth = 0
    for m in TOKEN.finditer(body):
        t = m.group(0)
        if t == "{":
            depth += 1
        elif t == "}":
            depth -= 1
        elif t == "<":
            in_chord = True
            continue
        elif t == ">":
            in_chord = False
            d = re.match(r"\d+\.*", body[m.end():])
            if d:
                dur = length(d.group(0))
            now += dur
        elif m.group("event") and not in_chord:
            if m.group("dur"):
                dur = length(m.group("dur"))
            # A bar rest running past the cut ends on it.
            if m.group("event") == "R" and now + dur > bars * measure and "*" in (m.group("dur") or ""):
                left = int((bars * measure - now) / measure)
                return body[:m.start()] + f"R2*{left}" + " }" * depth
            now += dur
        if now >= bars * measure:
            end = m.end()
            # Take the postfix marks of the last note with it.
            tail = re.match(r"(\s*(?:-\.|\\[a-zA-Z!<>]+|\^\\?[a-zA-Z\"]+|\]|\)))*", body[end:])
            end += tail.end()
            out = body[:end].rstrip()
            out = re.sub(r"~\s*$", "", out)
            return out + " }" * depth
    return body


def length(token):
    m = re.match(r"(\d+)(\.*)(?:\*(\d+)(?:/(\d+))?)?", token)
    base = Fraction(1, int(m.group(1)))
    total = base
    add = base
    for _ in m.group(2):
        add /= 2
        total += add
    if m.group(3):
        total *= int(m.group(3))
        if m.group(4):
            total /= int(m.group(4))
    return total


def main():
    folder, bars = sys.argv[1], int(sys.argv[2])
    staves = []
    for i, (file, var, name, short, by) in enumerate(PARTS):
        text = strip_comments(open(f"{folder}/{file}", encoding="utf-8").read())
        body, instrument = clean(variable(text, var))
        if by:
            body = transpose(body, by)
            body = re.sub(r"(\\key\s+[a-g]\w*)[',]+", r"\1", body)
        body = cut(body, bars)
        if i == 0:
            body = '\\tempo "Allegro con brio" 2 = 96 ' + body
        with_ = f'instrumentName = "{name}" shortInstrumentName = "{short}"'
        instrument = MIDI.get(var, instrument)
        if instrument:
            with_ += f' midiInstrument = "{instrument}"'
        body = re.sub(r"\s+", " ", body).strip()
        staves.append(f"\\new Staff \\with {{ {with_} }} {{ {body} }}")
    print('\\header { title = "Symphony No. 5" subtitle = "1st movement, Allegro con brio" composer = "Ludwig van Beethoven" }')
    print("\\new StaffGroup <<")
    for s in staves:
        print("  " + s)
    print(">>")


main()

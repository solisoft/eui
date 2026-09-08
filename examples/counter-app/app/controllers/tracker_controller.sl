# Cheesetracker — a FastTracker 2 in an EUI window.
#
# The demo the catalogue could not be: a pattern editor with a cursor that
# lives under the keyboard, sixteen instruments, and a Play button that
# makes the thing audible. Everything here is Soli — the pattern, the
# instruments, and the mixer that turns them into eight-bit PCM — and
# everything drawn is EUI: boxes, text in the mono face, and four canvases
# for the scopes. There is no tracker library and no audio API on the
# client: the window is handed a `.wav` and told to play it.
#
# Why it works that way. Spec 03 §7 gives the client one sound primitive:
# an asset, `playing`, a volume, a position, and a `time_update` back. It
# is not a mixer, and it should not be — a mixer in the client is a
# synthesiser every application inherits whether it wants one or not. So
# the *server* mixes: on Play it renders the pattern into
# `public/tracker/song.wav`, the node names the file like any other asset,
# and the window fetches it by hash and plays it. The playhead comes back
# as milliseconds, which is what moves the row cursor: the pattern scrolls
# because the sound says where it is.
#
# The keyboard is FT2's. The lower two rows are the low octave
# (Z S X D C V G B H N J M), the upper two the octave above
# (Q 2 W 3 E R 5 T 6 Y 7 U), the arrows move the cursor, and the digits
# type an instrument, a volume or an effect when the cursor is on that
# column. One `click` handler and one `key_down` handler serve the whole
# grid: the click's payload is a point in the pattern's own box, which is
# a character grid, so a row and a column are two divisions.

TRACKER_ROWS = 64
TRACKER_CHANNELS = 8
# What a window shows around the cursor. FT2 keeps the current row in the
# middle of the pattern and scrolls the rows past it, which is also why a
# tracker never needs a scrollbar.
TRACKER_VISIBLE = 23
TRACKER_RATE = 22050

TRACKER_NOTE_NAMES = [
  "C-",
  "C#",
  "D-",
  "D#",
  "E-",
  "F-",
  "F#",
  "G-",
  "G#",
  "A-",
  "A#",
  "B-"
]

# Octave 0's twelve notes in millihertz, so the frequency of any note is a
# table lookup and a doubling — no logarithms, and no floating point in
# the mixer's inner loop.
TRACKER_BASE_MHZ = [16352, 17324, 18354, 19445, 20602, 21827, 23125, 24500, 25957, 27500, 29135, 30868]

TRACKER_OCTAVE_MUL = [
  1,
  2,
  4,
  8,
  16,
  32,
  64,
  128
]

# The five voices an instrument can have. A tracker's samples would live
# in the instrument; these are generated, so the instrument is a shape and
# a volume.
TRACKER_WAVES = ["Square", "Pulse", "Saw", "Triangle", "Noise"]

# FT2's note keys: two rows for the low octave, two for the one above.
TRACKER_KEYMAP = {
  "z": 0,
  "s": 1,
  "x": 2,
  "d": 3,
  "c": 4,
  "v": 5,
  "g": 6,
  "b": 7,
  "h": 8,
  "n": 9,
  "j": 10,
  "m": 11,
  "q": 12,
  "2": 13,
  "w": 14,
  "3": 15,
  "e": 16,
  "r": 17,
  "5": 18,
  "t": 19,
  "6": 20,
  "y": 21,
  "7": 22,
  "u": 23
}

# The palette, as close to FastTracker 2's own as a screenshot allows.
TRACKER_DESK = "#5c6a86"
TRACKER_PANEL = "#7f8aa8"
TRACKER_FACE = "#9aa4bf"
TRACKER_LIGHT = "#c6cddf"
TRACKER_DARK = "#2b3145"
TRACKER_BLACK = "#0b0d16"
TRACKER_ROW_4 = "#161b2c"
TRACKER_ROW_16 = "#20263c"
TRACKER_NOTE = "#e9edfb"
TRACKER_INST = "#7ce87c"
TRACKER_VOL = "#e8e07c"
TRACKER_FX = "#e8a87c"
TRACKER_DIM = "#4d5773"
TRACKER_CURSOR = "#2f5fb0"
TRACKER_PLAY = "#294b2b"
TRACKER_INK = "#0b0d16"

# ------------------------------------------------------------- the model

def tracker_cells
  range(0, TRACKER_ROWS * TRACKER_CHANNELS).map(fn(i) { {
    "n": -1,
    "i": 0,
    "v": 0,
    "f": -1,
    "p": 0
  } })
end

def tracker_note(cells, at, chan, note, inst)
  cell = cells[at * TRACKER_CHANNELS + chan]
  cell["n"] = note
  cell["i"] = inst
  cells
end

# A demonstration pattern: a bass line, a chord every four rows, and a hat
# on the off-beats. Something has to play when the button is pressed.
def tracker_demo_cells
  cells = tracker_cells()
  bass = [24, 24, 31, 24, 26, 24, 29, 24]
  i = 0
  while i < 16
    step = bass[i % 8]
    cells = tracker_note(cells, i * 4, 0, step, 1)
    cells = tracker_note(cells, i * 4 + 2, 0, step + 12, 1)
    i = i + 1
  end
  j = 0
  while j < 8
    root = j % 2 == 0 ? 48 : 51
    cells = tracker_note(cells, j * 8, 1, root, 2)
    cells = tracker_note(cells, j * 8, 2, root + 4, 2)
    cells = tracker_note(cells, j * 8, 3, root + 7, 2)
    j = j + 1
  end
  k = 0
  while k < 32
    cells = tracker_note(cells, k * 2 + 1, 4, 60, 3)
    k = k + 1
  end
  m = 0
  while m < 8
    cells = tracker_note(cells, m * 8 + 4, 5, 72 + m % 3 * 2, 4)
    m = m + 1
  end
  cells
end

def tracker_instruments
  [
    {
      "name": "bass square",
      "wave": 0,
      "vol": 48
    },
    {
      "name": "lead pulse",
      "wave": 1,
      "vol": 34
    },
    {
      "name": "hat noise",
      "wave": 4,
      "vol": 20
    },
    {
      "name": "bell tri",
      "wave": 3,
      "vol": 30
    },
    {
      "name": "saw stab",
      "wave": 2,
      "vol": 32
    },
    {
      "name": "",
      "wave": 0,
      "vol": 40
    },
    {
      "name": "",
      "wave": 0,
      "vol": 40
    },
    {
      "name": "",
      "wave": 0,
      "vol": 40
    }
  ]
end

def tracker_defaults(state)
  base = {
    "cells": tracker_demo_cells(),
    "instruments": tracker_instruments(),
    "row": 0,
    "chan": 0,
    "col": 0,
    "inst": 1,
    "octave": 4,
    "edit": true,
    "bpm": 125,
    "speed": 6,
    "playing": false,
    "at": 0,
    "seek": 0,
    "rendered": "",
    "took": 0,
    "status": "Ready."
  }
  for key in base.keys()
    base[key] = state[key] unless state[key].nil?
  end
  base
end

def tracker_cell_at(state, at, chan)
  state["cells"][at * TRACKER_CHANNELS + chan]
end

# ------------------------------------------------------- what a cell says

def tracker_hex(n, width)
  digits = "0123456789ABCDEF"
  return digits[n % 16] if width == 1

  digits[int(n / 16) % 16] + digits[n % 16]
end

def tracker_note_text(n)
  return "---" if n < 0
  return "===" if n == -2

  TRACKER_NOTE_NAMES[n % 12] + str(int(n / 12))
end

def tracker_row_ms(state)
  # FT2's clock: a tick is 2500/BPM milliseconds and a row is `speed` of
  # them, which is why 125 BPM at speed 6 is the familiar 120 ms row.
  int(2500 * state["speed"] / state["bpm"])
end

# --------------------------------------------------------------- the mix
# Eight-bit PCM at 22 050 Hz, mixed a row at a time. A voice's phase is an
# integer that wraps at 65 536, so a sample is a table lookup and a
# multiply, and the whole pattern is a few hundred thousand of them.

def tracker_freq_mhz(note)
  octave = int(note / 12)
  octave = 7 if octave > 7
  TRACKER_BASE_MHZ[note % 12] * TRACKER_OCTAVE_MUL[octave]
end

def tracker_step(note)
  # phase units per sample: freq × 65536 / rate, in millihertz throughout.
  int(tracker_freq_mhz(note) * 65536 / (TRACKER_RATE * 1000))
end

def tracker_wave_at(wave, phase, i)
  return phase < 32768 ? 100 : -100 if wave == 0
  return phase < 16384 ? 100 : -100 if wave == 1
  return int(phase / 328) - 100 if wave == 2

  if wave == 3
    t = int(phase / 164)
    return t < 200 ? t - 100 : 300 - t
  end

  # Noise: a hash of the sample number, so a voice needs no state to be
  # rendered out of order.
  int(i * 1103515245 / 65536) % 200 - 100
end

# One row of one voice, as an array of signed samples.
def tracker_voice_row(voice, count, at)
  return range(0, count).map(fn(i) { 0 }) if voice.nil?

  wave = voice["wave"]
  step = voice["step"]
  vol = voice["vol"]
  phase0 = voice["phase"]
  age0 = voice["age"]
  range(0, count).map(fn(i) {
    age = age0 + i
    # A short attack and a long decay, so a note has an edge and a tail
    # instead of a click at each end.
    gain = age < 64 ? age * 100 / 64 : 100
    fade = age > 2000 ? 100 - int((age - 2000) / 60) : 100
    fade = 0 if fade < 0
    tracker_wave_at(wave, (phase0 + i * step) % 65536, age) * vol * gain * fade / 1000000
  })
end

def tracker_mix_row(voices, count, at)
  sum = range(0, count).map(fn(i) { 0 })
  for voice in voices
    next if voice.nil?
    next if voice["vol"] == 0

    part = tracker_voice_row(voice, count, at)
    sum = range(0, count).map(fn(i) { sum[i] + part[i] })
  end
  sum
end

def tracker_le(n, width)
  return [n % 256, int(n / 256) % 256] if width == 2;

  [n % 256, int(n / 256) % 256, int(n / 65536) % 256, int(n / 16777216) % 256]
end

def tracker_wav_header(count)
  riff = [82, 73, 70, 70]
  wave = [87, 65, 86, 69]
  fmt = [102, 109, 116, 32]
  data = [100, 97, 116, 97]
  head = riff.concat(tracker_le(36 + count, 4), wave, fmt, tracker_le(16, 4))
  head = head.concat(tracker_le(1, 2), tracker_le(1, 2), tracker_le(TRACKER_RATE, 4))
  head = head.concat(tracker_le(TRACKER_RATE, 4), tracker_le(1, 2), tracker_le(8, 2))
  head.concat(data, tracker_le(count, 4))
end

# The pattern, rendered. Voices survive across rows — a note rings until
# the channel is given another one — which is the whole difference between
# a tracker and a drum machine.
def tracker_render(state)
  started = DateTime.now()
  cells = state["cells"]
  instruments = state["instruments"]
  count = int(TRACKER_RATE * tracker_row_ms(state) / 1000)
  voices = range(0, TRACKER_CHANNELS).map(fn(c) { nil })
  bytes = []
  index = 0
  while index < TRACKER_ROWS
    chan = 0
    while chan < TRACKER_CHANNELS
      cell = cells[index * TRACKER_CHANNELS + chan]
      note = cell["n"]
      voices[chan] = nil if note == -2
      if note >= 0
        instrument = instruments[cell["i"] - 1] ?? instruments[0]
        voices[chan] = {
          "wave": instrument["wave"],
          "step": tracker_step(note),
          "vol": cell["v"] > 0 ? cell["v"] : instrument["vol"],
          "phase": 0,
          "age": 0
        }
      end
      chan = chan + 1
    end
    mixed = tracker_mix_row(voices, count, index * count)
    bytes = bytes.concat(mixed.map(fn(v) {
      out = 128 + v
      out = 255 if out > 255
      out = 0 if out < 0
      out
    }))
    # Carry each voice forward: the phase it reached and the samples it has
    # lived, so the next row continues the note rather than restarting it.
    chan = 0
    while chan < TRACKER_CHANNELS
      voice = voices[chan]
      unless voice.nil?
        voice["phase"] = (voice["phase"] + count * voice["step"]) % 65536
        voice["age"] = voice["age"] + count
      end
      chan = chan + 1
    end
    index = index + 1
  end
  mkdir_p("public/tracker") rescue nil
  file_write_bytes("public/tracker/song.wav", tracker_wav_header(bytes.length()).concat(bytes))
  ms = int((DateTime.now().to_unix() - started.to_unix()) * 1000)
  {
    "path": "public/tracker/song.wav",
    "ms": ms,
    "samples": bytes.length()
  }
end

# ------------------------------------------------------------- the events

def tracker_move(state, drow, dchan)
  index = state["row"] + drow
  index = index % TRACKER_ROWS
  index = index + TRACKER_ROWS if index < 0
  state["row"] = index
  chan = state["chan"] + dchan
  chan = chan % TRACKER_CHANNELS
  chan = chan + TRACKER_CHANNELS if chan < 0
  state["chan"] = chan
  state
end

def tracker_put(state, note)
  return state unless state["edit"]

  cell = tracker_cell_at(state, state["row"], state["chan"])
  cell["n"] = note
  cell["i"] = note < 0 ? 0 : state["inst"]
  tracker_move(state, 1, 0)
end

def tracker_clear(state)
  return state unless state["edit"]

  cell = tracker_cell_at(state, state["row"], state["chan"])
  cell["n"] = -1
  cell["i"] = 0
  cell["v"] = 0
  cell["f"] = -1
  cell["p"] = 0
  tracker_move(state, 1, 0)
end

# A digit typed on the instrument, volume or effect column shifts the
# value left and drops the digit in, the way a tracker's hex fields work.
def tracker_digit(state, digit)
  return state unless state["edit"]

  cell = tracker_cell_at(state, state["row"], state["chan"])
  col = state["col"]
  cell["i"] = (cell["i"] * 16 + digit) % 256 if col == 1
  cell["v"] = (cell["v"] * 16 + digit) % 128 if col == 2
  cell["p"] = (cell["p"] * 16 + digit) % 256 if col == 3
  state
end

def tracker_letter(state, key)
  offset = TRACKER_KEYMAP[key]
  return nil if offset.nil?

  note = (state["octave"] + int(offset / 12)) * 12 + offset % 12
  note > 95 ? 95 : note
end

def tracker_key(state, key, mods)
  return tracker_move(state, -1, 0) if key == "ArrowUp"
  return tracker_move(state, 1, 0) if key == "ArrowDown"
  return tracker_move(state, -16, 0) if key == "PageUp"
  return tracker_move(state, 16, 0) if key == "PageDown"
  return set_key(state, "row", 0) if key == "Home"
  return set_key(state, "row", TRACKER_ROWS - 1) if key == "End"
  return tracker_clear(state) if key == "Delete" || key == "Backspace"
  return tracker_put(state, -2) if key == "`" || key == "CapsLock"
  return set_key(state, "edit", !state["edit"]) if key == "Escape"

  if key == "ArrowLeft"
    return tracker_move(set_key(state, "col", 3), 0, -1) if state["col"] == 0

    return set_key(state, "col", state["col"] - 1)
  end
  if key == "ArrowRight"
    return tracker_move(set_key(state, "col", 0), 0, 1) if state["col"] == 3

    return set_key(state, "col", state["col"] + 1)
  end
  return tracker_move(set_key(state, "col", 0), 0, mods % 2 == 1 ? -1 : 1) if key == "Tab"

  lower = key.downcase()
  note = state["col"] == 0 ? tracker_letter(state, lower) : nil
  return tracker_put(state, note) unless note.nil?

  # A digit is an instrument, a volume or an effect parameter, unless the
  # cursor is on the note column, where the digits pick the octave.
  digits = "0123456789abcdef"
  at = digits.index_of(lower) ?? -1
  return tracker_digit(state, at) if at >= 0 && state["col"] > 0
  return set_key(state, "octave", at) if at >= 0 && at < 8 && state["col"] == 0

  state
end

# The pattern is a character grid, so where a click landed is two
# divisions: the row from the y, the channel and its column from the x.
def tracker_click(state, params)
  point = params["payload"] ?? [0, 0]
  props = params["props"] ?? {}
  top = props["top"] ?? 0
  index = top + int(point[1] / TRACKER_ROW_H)
  index = TRACKER_ROWS - 1 if index > TRACKER_ROWS - 1
  index = 0 if index < 0
  x = point[0] - TRACKER_GUTTER_W
  cell = int(x / TRACKER_CELL_W)
  cell = 0 if cell < 0
  cell = TRACKER_CHANNELS - 1 if cell > TRACKER_CHANNELS - 1
  within = x - cell * TRACKER_CELL_W
  col = 0
  col = 1 if within > 34
  col = 2 if within > 58
  col = 3 if within > 82
  state["row"] = index
  state["chan"] = cell
  state["col"] = col
  state
end

TRACKER_ROW_H = 15
TRACKER_CELL_W = 116
TRACKER_GUTTER_W = 34

def tracker_play(state)
  answer = tracker_render(state)
  state["rendered"] = answer["path"]
  state["took"] = answer["ms"]
  state["playing"] = true
  state["at"] = 0
  state["seek"] = 0
  state["status"] = "Playing — " + str(answer["samples"]) + " samples mixed in " + str(answer["ms"]) + " ms"
  state
end

def tracker_stop(state)
  state["playing"] = false
  state["at"] = 0
  state["status"] = "Stopped."
  state
end

# The playhead says where the sound is; the cursor follows it. This is the
# only clock in the application — nothing here counts frames.
def tracker_tick(state, params)
  payload = params["payload"] ?? [0, 0]
  at = payload[0]
  state["at"] = at
  return state unless state["playing"]

  index = int(at / tracker_row_ms(state))
  index = TRACKER_ROWS - 1 if index > TRACKER_ROWS - 1
  state["row"] = index
  state
end

def tracker(event_data)
  event = event_data["event"]
  params = event_data["params"]
  props = params["props"] ?? {}
  state = tracker_defaults(event_data["state"] ?? {})
  match event {
    "key" => tracker_key(state, params["payload"][0], params["payload"][1]),
    "click" => tracker_click(state, params),
    "play" => tracker_play(state),
    "stop" => tracker_stop(state),
    "ended" => tracker_stop(state),
    "tick" => tracker_tick(state, params),
    "inst" => set_key(state, "inst", props["id"]),
    "octave" => set_key(state, "octave", props["id"]),
    "edit" => set_key(state, "edit", !state["edit"]),
    "bpm" => set_key(state, "bpm", state["bpm"] + props["by"] < 32 ? 32 : state["bpm"] + props["by"]),
    "speed" => set_key(state, "speed", state["speed"] + props["by"] < 1 ? 1 : state["speed"] + props["by"]),
    "wave" => tracker_set_wave(state, props["id"]),
    _ => state,
  }
end

def tracker_set_wave(state, id)
  instrument = state["instruments"][state["inst"] - 1]
  return state if instrument.nil?

  instrument["wave"] = id
  state
end

# ---------------------------------------------------------------- the look
# Everything below draws FastTracker 2's own furniture: a face colour, a
# light edge above and left, a dark one below and right. Two boxes, since
# a border in EUI has one colour and four widths.

def tracker_bevel(style, children)
  inner = style.merge({
    "border": [0, 1, 1, 0],
    "border_color": TRACKER_DARK,
    "bg": style["bg"] ?? TRACKER_FACE
  })
  {
    "k": "box",
    "s": {
      "border": [1, 0, 0, 1],
      "border_color": TRACKER_LIGHT,
      "display": style["display"] ?? "column",
      "shrink": 0
    },
    "c": [node("box", inner, children)]
  }
end

def tracker_mono(content, colour, size)
  text(
    content,
    {
      "font": "mono",
      "size": size,
      "fg": colour
    }
  )
end

def tracker_button(label, event, props, lit)
  face = lit ? TRACKER_LIGHT : TRACKER_FACE
  face_box = tracker_bevel(
    {
      "display": "row",
      "align": "center",
      "justify": "center",
      "bg": face,
      "pad": [1, 2, 1, 2],
      "cursor": "pointer"
    },
    [tracker_mono(label, TRACKER_INK, 0)]
  )
  face_box["on"] = {"click": event}
  face_box["p"] = props
  face_box
end

def tracker_field(label, value)
  row(
    {"gap": 2, "align": "center"},
    [
      tracker_mono(label, TRACKER_INK, 0),
      tracker_bevel(
        {
          "bg": TRACKER_BLACK,
          "pad": [0, 2, 0, 2],
          "display": "row"
        },
        [tracker_mono(value, TRACKER_NOTE, 0)]
      )
    ]
  )
end

# The transport, the tempo and the state of the editor: FT2's top left.
def tracker_top(state)
  transport = row(
    {"gap": 1, "align": "center"},
    [
      tracker_button("Play", "play", {}, state["playing"]),
      tracker_button("Stop", "stop", {}, false),
      tracker_button(state["edit"] ? "Edit ON" : "Edit off", "edit", {}, state["edit"])
    ]
  )
  numbers = row(
    {
      "gap": 3,
      "align": "center",
      "wrap": "wrap"
    },
    [
      tracker_field("BPM", str(state["bpm"])),
      tracker_button("-", "bpm", {"by": -5}, false),
      tracker_button("+", "bpm", {"by": 5}, false),
      tracker_field("Spd", str(state["speed"])),
      tracker_button("-", "speed", {"by": -1}, false),
      tracker_button("+", "speed", {"by": 1}, false),
      tracker_field("Oct", str(state["octave"])),
      tracker_field("Row", tracker_hex(state["row"], 2))
    ]
  )
  tracker_bevel(
    {
      "display": "column",
      "gap": 2,
      "pad": 2,
      "width": "100%"
    },
    [transport, numbers]
  )
end

# One scope per channel, drawing the shape of whatever that channel is
# sounding. A tracker's scopes move; these say what the voice *is*, which
# is what a still picture of a tracker can honestly show.
def tracker_scope(state, chan)
  cell = tracker_cell_at(state, state["row"], chan)
  playing = cell["n"] >= 0
  instrument = state["instruments"][cell["i"] - 1] ?? state["instruments"][0]
  wave = instrument["wave"]
  points = range(0, 30).map(fn(i) { [i * 2, 13 - tracker_wave_at(wave, i * 4369 % 65536, i) * 11 / 100] })
  line = [0, playing ? TRACKER_INST : TRACKER_DIM, 1].concat(flatten_points(points))
  tracker_bevel(
    {"bg": TRACKER_BLACK, "pad": 0},
    [canvas(60, 26, [line])]
  )
end

def tracker_scopes(state)
  row(
    {
      "gap": 1,
      "wrap": "wrap",
      "shrink": 1
    },
    range(0, TRACKER_CHANNELS).map(fn(c) { tracker_scope(state, c) })
  )
end

# The instrument list, FT2's right-hand column: a number, a name, and the
# shape it plays.
def tracker_instrument_row(state, i)
  instrument = state["instruments"][i]
  chosen = state["inst"] == i + 1
  line = row(
    {
      "gap": 2,
      "align": "center",
      "width": "100%",
      "pad": [0, 1, 0, 1],
      "bg": chosen ? TRACKER_CURSOR : "none"
    },
    [
      tracker_mono(tracker_hex(i + 1, 2), chosen ? TRACKER_NOTE : TRACKER_DIM, 0),
      tracker_mono(instrument["name"] == "" ? "—" : instrument["name"], chosen ? TRACKER_NOTE : TRACKER_INST, 0),
      spacer(),
      tracker_mono(TRACKER_WAVES[instrument["wave"]], TRACKER_DIM, 0)
    ]
  )
  line["on"] = {"click": "inst"}
  line["p"] = {"id": i + 1}
  line
end

def tracker_instruments_panel(state)
  waves = row(
    {"gap": 1, "wrap": "wrap"},
    range(0, TRACKER_WAVES.length()).map(fn(w) { tracker_button(TRACKER_WAVES[w], "wave", {"id": w}, false) })
  )
  tracker_bevel(
    {
      "display": "column",
      "gap": 1,
      "pad": 2,
      "bg": TRACKER_PANEL,
      "width": 260,
      "shrink": 0
    },
    [
      tracker_mono("Instruments", TRACKER_INK, 0),
      tracker_bevel(
        {
          "bg": TRACKER_BLACK,
          "pad": 1,
          "display": "column",
          "width": "100%"
        },
        range(0, state["instruments"].length()).map(fn(i) { tracker_instrument_row(state, i) })
      ),
      waves
    ]
  )
end

# ------------------------------------------------------------ the pattern

def tracker_cell_nodes(state, at, chan, playing_row)
  cell = tracker_cell_at(state, at, chan)
  here = state["row"] == at && state["chan"] == chan
  parts = [
    {
      "w": TRACKER_NOTE_W,
      "t": tracker_note_text(cell["n"]),
      "c": cell["n"] < 0 ? TRACKER_DIM : TRACKER_NOTE
    },
    {
      "w": TRACKER_INST_W,
      "t": cell["i"] > 0 ? tracker_hex(cell["i"], 2) : "..",
      "c": cell["i"] > 0 ? TRACKER_INST : TRACKER_DIM
    },
    {
      "w": TRACKER_VOL_W,
      "t": cell["v"] > 0 ? tracker_hex(cell["v"], 2) : "..",
      "c": cell["v"] > 0 ? TRACKER_VOL : TRACKER_DIM
    },
    {
      "w": TRACKER_FX_W,
      "t": cell["f"] >= 0 ? tracker_hex(cell["f"], 1) + tracker_hex(cell["p"], 2) : "...",
      "c": cell["f"] >= 0 ? TRACKER_FX : TRACKER_DIM
    }
  ]
  range(0, parts.length()).map(fn(i) {
    part = parts[i]
    lit = here && state["col"] == i
    {
      "k": "box",
      "s": {
        "width": part["w"],
        "display": "row",
        "bg": lit ? TRACKER_CURSOR : "none"
      },
      "c": [tracker_mono(part["t"], part["c"], 0)]
    }
  })
end

TRACKER_NOTE_W = 34
TRACKER_INST_W = 24
TRACKER_VOL_W = 24
TRACKER_FX_W = 34

def tracker_row_line(state, at, playing_row)
  background = TRACKER_BLACK
  background = TRACKER_ROW_4 if at % 4 == 0
  background = TRACKER_ROW_16 if at % 16 == 0
  background = TRACKER_PLAY if at == playing_row
  cells = range(0, TRACKER_CHANNELS).map(fn(c) {
    row(
      {
        "gap": 0,
        "width": TRACKER_CELL_W,
        "shrink": 0
      },
      tracker_cell_nodes(state, at, c, playing_row)
    )
  })
  keyed("r" + str(at), row(
    {
      "gap": 0,
      "height": TRACKER_ROW_H,
      "align": "center",
      "bg": background,
      "width": "100%"
    },
    [{
      "k": "box",
      "s": {
        "width": TRACKER_GUTTER_W,
        "display": "row",
        "shrink": 0
      },
      "c": [tracker_mono(tracker_hex(at, 2), at == state["row"] ? TRACKER_NOTE : TRACKER_DIM, 0)]
    }].concat(cells)
  ))
end

# The window of rows around the cursor. A tracker keeps the current row in
# the middle and moves the pattern past it, so there is no scrollbar to
# chase and the server sends twenty-three rows instead of sixty-four.
def tracker_window_top(state)
  top = state["row"] - int(TRACKER_VISIBLE / 2)
  top = 0 if top < 0
  top = TRACKER_ROWS - TRACKER_VISIBLE if top > TRACKER_ROWS - TRACKER_VISIBLE
  top
end

def tracker_pattern(state)
  top = tracker_window_top(state)
  playing_row = state["playing"] ? state["row"] : -1
  lines = range(top, top + TRACKER_VISIBLE).map(fn(r) { tracker_row_line(state, r, playing_row) })
  grid = {
    "k": "box",
    "s": {
      "display": "column",
      "gap": 0,
      "bg": TRACKER_BLACK,
      "pad": 0,
      "cursor": "pointer",
      "width": "100%",
      "grow": 1
    },
    "on": {"click": "click", "key_down": "key"},
    "p": {"top": top},
    "c": lines
  }
  tracker_bevel(
    {
      "display": "column",
      "bg": TRACKER_BLACK,
      "width": "100%",
      "grow": 1
    },
    [{
      "k": "scroll",
      "s": {
        "overflow": "scroll",
        "width": "100%",
        "grow": 1,
        "bg": TRACKER_BLACK
      },
      "c": [grid]
    }]
  )
end

def tracker_status(state)
  tracker_bevel(
    {
      "display": "row",
      "gap": 3,
      "align": "center",
      "pad": [0, 2, 0, 2],
      "width": "100%"
    },
    [
      tracker_mono(state["status"], TRACKER_INK, 0),
      spacer(),
      tracker_mono("Z-M / Q-U notes  ·  arrows move  ·  Esc toggles edit", TRACKER_INK, 0)
    ]
  )
end

def tracker_view(raw_state)
  state = tracker_defaults(raw_state ?? {})
  w = (state["viewport"] ?? {})["width"] ?? 1280
  wide = bp_min(w, "md")
  sound = audio(state["rendered"] == "" ? "public/sounds/chime.wav" : state["rendered"], {
    "playing": state["playing"],
    "volume": 90,
    "position": state["seek"]
  }, {"time_update": "tick", "ended": "ended"})
  left = column(
    {
      "gap": 2,
      "grow": 1,
      "shrink": 1,
      "min_width": 0
    },
    [tracker_top(state), tracker_scopes(state)]
  )
  head = wide ? row(
    {
      "gap": 2,
      "align": "start",
      "width": "100%"
    },
    [left, tracker_instruments_panel(state)]
  ) : column(
    {"gap": 2, "width": "100%"},
    [left, tracker_instruments_panel(state)]
  )
  column(
    {
      "gap": 2,
      "pad": 2,
      "bg": TRACKER_DESK,
      "width": "100%",
      "height": "100%"
    },
    [sound, head, tracker_pattern(state), tracker_status(state)]
  )
end

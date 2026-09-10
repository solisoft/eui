# A tracker in an EUI window: FastTracker 2's shape, Soli's mixer.
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
# What the mixer renders at. The rate is the whole cost of a render — one
# closure call per sample per voice, so mix time is linear in it — and the
# depth is nearly free, so the modes trade one against the other and say
# so on the status line rather than picking for you.
TRACKER_MODES = [
  {
    "name": "11k · 8-bit",
    "rate": 11025,
    "bits": 8
  },
  {
    "name": "22k · 16-bit",
    "rate": 22050,
    "bits": 16
  },
  {
    "name": "44k · 16-bit",
    "rate": 44100,
    "bits": 16
  }
]

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
TRACKER_CELL = "#1d3a70"
TRACKER_LINE = "#262d47"
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
    "play_row": -1,
    "follow": true,
    "at": 0,
    "seek": 0,
    "rendered": "",
    "print": "",
    "took": 0,
    "status": "Ready.",
    "skin": "modern",
    "mode": 1,
    "viewport": {
      "width": 1280,
      "height": 800,
      "scale": 1.0,
      "mode": "dark",
      "density": "cozy",
      "font_scale": 1.0
    }
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

def tracker_mode(state)
  TRACKER_MODES[state["mode"] ?? 1] ?? TRACKER_MODES[0]
end

def tracker_step(note, rate)
  # phase units per sample: freq × 65536 / rate, in millihertz throughout.
  int(tracker_freq_mhz(note) * 65536 / (rate * 1000))
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

# A voice's shape, sampled once into 256 steps. The mixer's inner loop is
# then an index and a multiply — no branch on the waveform, and none of
# the arithmetic that decides what a saw looks like.
# A voice's shape, sampled into 256 steps and already scaled by the volume
# and the envelope this row wants. Folding both into the table is what
# takes the mixer's inner loop down to a multiply, a lookup and an add:
# the four multiplies and two divisions that used to be *per sample* are
# now 256 of each, per voice, per row.
# Kept between rows: a voice's level only changes when its envelope moves,
# so most rows of most notes ask for a table that has already been built.
# Five shapes and a hundred-odd levels is a few hundred entries at worst.
TRACKER_TABLES = {}

def tracker_table(wave, level)
  key = str(wave) + ":" + str(level)
  hit = TRACKER_TABLES[key]
  return hit unless hit.nil?

  built = range(0, 256).map(fn(i) { tracker_wave_at(wave, i * 256, i) * level / 10000 })
  TRACKER_TABLES[key] = built
  built
end

# The envelope, read once for the row rather than per sample: a note is
# flat for two thousand samples and then falls away. The attack is the
# exception — sixty-four samples, patched in below, because a square wave
# that starts at full height clicks.
def tracker_level(vol, age)
  fade = age > 2000 ? 100 - int((age - 2000) / 60) : 100
  fade = 0 if fade < 0
  vol * fade
end

def tracker_add_voice(sum, voice, count)
  level = tracker_level(voice["vol"], voice["age"])
  return sum if level <= 0

  wave_table = tracker_table(voice["wave"], level)
  step = voice["step"]
  phase0 = voice["phase"]
  mixed = range(0, count).map(fn(i) { sum[i] + wave_table[int((phase0 + i * step) % 65536 / 256)] })
  return mixed if voice["age"] > 0

  # The note starts here: ramp its first sixty-four samples in place.
  i = 0
  while i < 64 && i < count
    mixed[i] = sum[i] + int((mixed[i] - sum[i]) * i / 64)
    i = i + 1
  end
  mixed
end

def tracker_mix_row(voices, count, at)
  sum = range(0, count).map(fn(i) { 0 })
  for voice in voices
    next if voice.nil?
    next if voice["vol"] == 0

    sum = tracker_add_voice(sum, voice, count)
  end
  sum
end

def tracker_le(n, width)
  return [n % 256, int(n / 256) % 256] if width == 2;

  [n % 256, int(n / 256) % 256, int(n / 65536) % 256, int(n / 16777216) % 256]
end

def tracker_wav_header(count, rate, bits)
  riff = [82, 73, 70, 70]
  wave = [87, 65, 86, 69]
  fmt = [102, 109, 116, 32]
  data = [100, 97, 116, 97]
  block = int(bits / 8)
  head = riff.concat(tracker_le(36 + count, 4), wave, fmt, tracker_le(16, 4))
  head = head.concat(tracker_le(1, 2), tracker_le(1, 2), tracker_le(rate, 4))
  head = head.concat(tracker_le(rate * block, 4), tracker_le(block, 2), tracker_le(bits, 2))
  head.concat(data, tracker_le(count, 4))
end

# Eight bits go out unsigned, sixteen signed and little-endian: what a
# `.wav` means by each, and the only place the depth shows up at all.
def tracker_pack(mixed, peak, bits)
  if bits == 8
    return mixed.map(fn(v) {
      out = 128 + int(v * 120 / peak)
      out = 255 if out > 255
      out = 0 if out < 0
      out
    })
  end

  pairs = mixed.map(fn(v) {
    out = int(v * 30000 / peak)
    out = 32767 if out > 32767
    out = 0 - 32767 if out < 0 - 32767
    out = out + 65536 if out < 0;
    [out % 256, int(out / 256) % 256]
  })
  pairs.flatten()
end

# The pattern, rendered. Voices survive across rows — a note rings until
# the channel is given another one — which is the whole difference between
# a tracker and a drum machine.
def tracker_render(state)
  started = DateTime.microtime()
  cells = state["cells"]
  instruments = state["instruments"]
  mode = tracker_mode(state)
  rate = mode["rate"]
  count = int(rate * tracker_row_ms(state) / 1000)
  voices = range(0, TRACKER_CHANNELS).map(fn(c) { nil })
  mixed = []
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
          "step": tracker_step(note, rate),
          "vol": cell["v"] > 0 ? cell["v"] : instrument["vol"],
          "phase": 0,
          "age": 0
        }
      end
      chan = chan + 1
    end
    mixed = mixed.concat(tracker_mix_row(voices, count, index * count))
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
  # One pass to find the loudest sample, one to scale by it: eight bits
  # have no headroom to waste, and four voices at full volume would clip
  # every time the chord lands.
  peak = 1
  for v in mixed
    peak = v if v > peak
    peak = 0 - v if 0 - v > peak
  end
  bytes = tracker_pack(mixed, peak, mode["bits"])
  mkdir_p("public/tracker") rescue nil
  file_write_bytes("public/tracker/song.wav", tracker_wav_header(bytes.length(), rate, mode["bits"]).concat(bytes))
  ms = int((DateTime.microtime() - started) / 1000)
  {
    "path": "public/tracker/song.wav",
    "ms": ms,
    "samples": mixed.length(),
    "mode": mode["name"]
  }
end

# ------------------------------------------------------------- the events

# Moving the cursor takes the window off the playhead: while it plays, the
# pattern scrolls with the music until the typist says otherwise, and from
# then on it stays where they are working. Play or Stop hands it back.
def tracker_move(state, drow, dchan)
  state["follow"] = false
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

# The wav that is playing was mixed before the key was pressed, so an edit
# cannot be heard until the next Play. Saying so is the whole difference
# between a tracker that ignores you and one you understand.
def tracker_edited(state)
  state["status"] = "Edited — Play again to hear it" if state["playing"]
  state
end

def tracker_put(state, note)
  return state unless state["edit"]

  cell = tracker_cell_at(state, state["row"], state["chan"])
  cell["n"] = note
  cell["i"] = note < 0 ? 0 : state["inst"]
  tracker_move(tracker_edited(state), 1, 0)
end

def tracker_clear(state)
  return state unless state["edit"]

  cell = tracker_cell_at(state, state["row"], state["chan"])
  cell["n"] = -1
  cell["i"] = 0
  cell["v"] = 0
  cell["f"] = -1
  cell["p"] = 0
  tracker_move(tracker_edited(state), 1, 0)
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
  tracker_edited(state)
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
  return set_key(state, "edit", !state["edit"]) if key == "Insert"
  return state["playing"] ? tracker_stop(state) : tracker_play(state) if key == " "

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

  # On the other columns a digit is a value, typed the way a tracker types
  # hex: shifted in from the right.
  digits = "0123456789abcdef"
  at = digits.index_of(lower) ?? -1
  return tracker_digit(state, at) if at >= 0 && state["col"] > 0

  # FT2 moves the octave with the two keys beside the note rows.
  return tracker_octave(state, 1) if key == "*" || key == "+"
  return tracker_octave(state, -1) if key == "/" || key == "-"

  state
end

def tracker_octave(state, by)
  octave = state["octave"] + by
  octave = 0 if octave < 0
  octave = 7 if octave > 7
  set_key(state, "octave", octave)
end

# The pattern is a character grid, so where a click landed is two
# divisions: the row from the y, the channel and its column from the x.
# A click carries its row in its props; the channel and the column come
# from the x, because a pattern is a character grid and a point in it is
# two divisions.
def tracker_click(state, params)
  point = params["payload"] ?? [0, 0]
  props = params["props"] ?? {}
  top = props["top"] ?? 0
  index = top + int((point[1] - TRACKER_ROW_H) / TRACKER_ROW_H)
  index = TRACKER_ROWS - 1 if index > TRACKER_ROWS - 1
  index = 0 if index < 0
  x = point[0] - TRACKER_GUTTER_W
  cell = props["first"] ?? 0
  cell = cell + int(x / TRACKER_CELL_W)
  cell = 0 if cell < 0
  cell = TRACKER_CHANNELS - 1 if cell > TRACKER_CHANNELS - 1
  within = x % TRACKER_CELL_W
  col = 0
  col = 1 if within > 34
  col = 2 if within > 58
  col = 3 if within > 82
  state["row"] = index
  state["chan"] = cell
  state["col"] = col
  state["follow"] = false
  state
end

TRACKER_ROW_H = 15
TRACKER_CELL_W = 116
TRACKER_GUTTER_W = 34

# What the song is, in one string: the same pattern, tempo and instruments
# render to the same wav, so the second Play is free.
def tracker_fingerprint(state)
  notes = state["cells"].map(fn(c) { str(c["n"]) + ":" + str(c["i"]) + ":" + str(c["v"]) })
  shapes = state["instruments"].map(fn(i) { str(i["wave"]) + ":" + str(i["vol"]) })
  str(state["bpm"]) + "/" + str(state["speed"]) + "/" + str(state["mode"]) + "/" + notes.join(",") + "/"
  + shapes.join(",")
end

# A new window starts stopped, whatever the last one was doing: the state
# survives the session that made it, and a transport that comes back
# playing is a Play button that does nothing.
def tracker_open(state, params)
  state["viewport"] = params["viewport"] ?? state["viewport"]
  state["playing"] = false
  state["play_row"] = -1
  state["follow"] = true
  state["at"] = 0
  state["status"] = "Ready."
  state
end

def tracker_play(state)
  print_of = tracker_fingerprint(state)
  if state["rendered"] != "" && state["print"] == print_of
    state["playing"] = true
    state["play_row"] = 0
    state["follow"] = true
    state["at"] = 0
    state["seek"] = state["seek"] == 0 ? 1 : 0
    state["status"] = "Playing — the same song, already mixed"
    return state
  end

  answer = tracker_render(state)
  state["print"] = print_of
  state["rendered"] = answer["path"]
  state["took"] = answer["ms"]
  state["playing"] = true
  state["play_row"] = 0
  state["follow"] = true
  state["at"] = 0
  state["seek"] = state["seek"] == 0 ? 1 : 0
  state["status"] = "Playing " + answer["mode"] + " — " + str(answer["samples"]) + " samples mixed in "
  + str(answer["ms"])
  + " ms"
  state
end

def tracker_stop(state)
  state["playing"] = false
  state["play_row"] = -1
  state["follow"] = true
  state["at"] = 0
  state["status"] = "Stopped."
  state
end

# The playhead says where the sound is. It is its own row, not the cursor:
# the cursor is where typing lands, and a tracker one can only watch is
# half a tracker. This is the only clock in the application — nothing here
# counts frames; the window reports where its own mixer has got to.
def tracker_tick(state, params)
  payload = params["payload"] ?? [0, 0]
  at = payload[0]
  state["at"] = at
  return state unless state["playing"]

  index = int(at / tracker_row_ms(state))
  index = TRACKER_ROWS - 1 if index > TRACKER_ROWS - 1
  state["play_row"] = index
  state
end

# Which row is sounding: the playhead while it plays, the cursor otherwise.
# The scopes read it, so they show the chord that is in the air rather than
# the one under the cursor.
def tracker_sounding_row(state)
  return state["play_row"] if state["playing"] && state["play_row"] >= 0

  state["row"]
end

def tracker(event_data)
  event = event_data["event"]
  params = event_data["params"]
  props = params["props"] ?? {}
  state = tracker_defaults(event_data["state"] ?? {})
  match event {
    "connect" => tracker_open(state, params),
    "viewport" => set_key(state, "viewport", params["viewport"] ?? state["viewport"]),
    "key" => tracker_key(state, params["payload"][0], params["payload"][1]),
    "click" => tracker_click(state, params),
    "play" => tracker_play(state),
    "stop" => tracker_stop(state),
    "ended" => tracker_stop(state),
    "tick" => tracker_tick(state, params),
    "inst" => set_key(state, "inst", props["id"]),
    "octave" => tracker_octave(state, props["by"] ?? 0),
    "edit" => set_key(state, "edit", !state["edit"]),
    "skin" => set_key(state, "skin", state["skin"] == "retro" ? "modern" : "retro"),
    "mode" => set_key(state, "mode", (state["mode"] + 1) % TRACKER_MODES.length()),
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
#
# Two skins over one tracker. The **modern** one draws with the theme's
# roles — `surface.raised`, `accent.base`, `text.muted` — so it follows the
# viewer into dark mode and into whatever palette their desktop is wearing
# (05 §1), and it borrows the catalogue's own buttons and cards. The
# **retro** one paints FastTracker 2's furniture literally: a face colour,
# a light edge above and left, a dark one below and right, and the exact
# greens and yellows a DOS tracker put on black.
#
# Nothing below decides which: the view resolves a skin once and hands it
# down, so a cell knows what colour a note is and nothing else.

def tracker_skin(state)
  if !(state["skin"] == "retro")
    return {
      "modern": true,
      "desk": "surface.base",
      "panel": "surface.raised",
      "face": "surface.sunken",
      "edge": "border.subtle",
      "grid": "surface.base",
      "row4": "surface.raised",
      "row16": "surface.sunken",
      "head": "surface.raised",
      "note": "text.default",
      "inst": "accent.base",
      "vol": "success.base",
      "fx": "warning.base",
      "dim": "text.disabled",
      "cursor": "accent.base",
      "on_cursor": "accent.on",
      "cell": "info.subtle",
      "line": "surface.overlay",
      "play": "success.subtle",
      "ink": "text.default",
      "label": "text.muted",
      "radius": 2,
      "pad": 3
    }
  end

  {
    "modern": false,
    "desk": TRACKER_DESK,
    "panel": TRACKER_PANEL,
    "face": TRACKER_FACE,
    "edge": TRACKER_DARK,
    "grid": TRACKER_BLACK,
    "row4": TRACKER_ROW_4,
    "row16": TRACKER_ROW_16,
    "head": TRACKER_ROW_16,
    "note": TRACKER_NOTE,
    "inst": TRACKER_INST,
    "vol": TRACKER_VOL,
    "fx": TRACKER_FX,
    "dim": TRACKER_DIM,
    "cursor": TRACKER_CURSOR,
    "on_cursor": TRACKER_NOTE,
    "cell": TRACKER_CELL,
    "line": TRACKER_LINE,
    "play": TRACKER_PLAY,
    "ink": TRACKER_INK,
    "label": TRACKER_INK,
    "radius": 0,
    "pad": 2
  }
end

# A panel. Bevelled in the retro skin — two boxes, since a border in EUI
# has one colour and four widths — and a flat bordered surface with a
# radius in the modern one.
def tracker_frame(skin, style, children)
  if skin["modern"]
    flat = style.merge({
      "bg": style["bg"] ?? skin["panel"],
      "border": 1,
      "border_color": skin["edge"],
      "radius": skin["radius"],
      "display": style["display"] ?? "column"
    })
    return node("box", flat, children)
  end

  # The bevel is two boxes, so anything the caller wants the frame to do in
  # its parent's flow — grow into the window, take its width — belongs on
  # the outer one, and the inner one has to fill it.
  inner = style.merge({
    "border": [0, 1, 1, 0],
    "border_color": TRACKER_DARK,
    "bg": style["bg"] ?? skin["face"],
    "grow": style["grow"] ?? 0,
    "height": style["grow"] ?? 0 > 0 ? "100%" : "auto"
  })
  {
    "k": "box",
    "s": {
      "border": [1, 0, 0, 1],
      "border_color": TRACKER_LIGHT,
      "display": style["display"] ?? "column",
      "shrink": 0,
      "grow": style["grow"] ?? 0,
      "width": style["width"] ?? "auto"
    },
    "c": [node("box", inner, children)]
  }
end

# The grid is monospaced in both skins: a tracker's columns only line up
# because every glyph is the same width.
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

# A label reads in the skin's own voice: the theme's face when modern, the
# same mono as everything else when retro.
def tracker_label(skin, content)
  if skin["modern"]
    return text(
      content,
      {"fg": skin["label"], "size": 1}
    )
  end

  tracker_mono(content, skin["label"], 0)
end

# The transport's Play spins while the mixer works. A pattern takes about
# a second to render, and a button that looks inert for a second is a
# button people press twice and then call broken.
def tracker_play_button(skin, state)
  return tracker_button(skin, "Play", "play", {}, true) unless skin["modern"]

  play_button = loading_button(state["playing"] ? "Playing" : "Play", "play", "tracker_play")
  play_button["s"]["min_width"] = 0
  play_button["s"]["pad"] = [
    1,
    3,
    1,
    3
  ]
  play_button
end

def tracker_button(skin, label, event, props, lit)
  if skin["modern"]
    pressed = lit ? button(label, event) : secondary_button(label, event)
    pressed["p"] = props
    # A tracker's toolbar is tighter than the catalogue's default, and the
    # narrowing has to reach the hover and pressed styles too — see `restyle`.
    return restyle(
      pressed,
      {"min_width": 0, "pad": [
        1,
        3,
        1,
        3
      ]}
    )
  end

  face_box = tracker_frame(skin, {
    "display": "row",
    "align": "center",
    "justify": "center",
    "bg": lit ? TRACKER_LIGHT : TRACKER_FACE,
    "pad": [1, 2, 1, 2],
    "cursor": "pointer"
  }, [tracker_mono(label, TRACKER_INK, 0)])
  face_box["on"] = {"click": event}
  face_box["p"] = props
  face_box
end

# A number in a sunken well, which is how a tracker shows a value you can
# change without a text field.
def tracker_field(skin, label, value)
  row(
    {"gap": 2, "align": "center"},
    [
      tracker_label(skin, label),
      tracker_frame(skin, {
        "bg": skin["grid"],
        "pad": [0, 2, 0, 2],
        "display": "row",
        "shrink": 0
      }, [tracker_mono(value, skin["note"], 0)])
    ]
  )
end

# The nameplate, which is the first thing anyone recognises about FT2.
def tracker_plate(skin)
  tracker_frame(skin, {
    "display": "column",
    "pad": [1, 3, 1, 3],
    "bg": skin["grid"],
    "shrink": 0
  }, [
    text(
      "TRACKER II",
      {
        "font": "mono",
        "size": 2,
        "weight": "bold",
        "fg": skin["inst"]
      }
    ),
    tracker_mono("eui · soli", skin["dim"], 0)
  ])
end

# Where the song is: which pattern, how long it is, and what the cursor
# has in hand. A tracker tells you all of it at once.
def tracker_song_panel(state, skin)
  tracker_frame(skin, {
    "display": "column",
    "gap": 1,
    "pad": 2,
    "bg": skin["panel"],
    "shrink": 0
  }, [
    row(
      {"gap": 2, "align": "center"},
      [tracker_field(skin, "Pos", "00"), tracker_field(skin, "Ptn", "00"), tracker_field(skin, "Len", "01")]
    ),
    row(
      {"gap": 2, "align": "center"},
      [
        tracker_field(skin, "Rows", str(TRACKER_ROWS)),
        tracker_field(skin, "Chn", str(TRACKER_CHANNELS)),
        tracker_field(skin, "Ins", tracker_hex(state["inst"], 2))
      ]
    )
  ])
end

# The transport, the tempo, the octave, and which skin is on.
def tracker_top(state, skin)
  transport = row(
    {"gap": 1, "align": "center"},
    [
      tracker_play_button(skin, state),
      tracker_button(skin, "Stop", "stop", {}, false),
      tracker_button(skin, state["edit"] ? "Edit ON" : "Edit off", "edit", {}, state["edit"]),
      tracker_button(skin, skin["modern"] ? "Retro skin" : "Modern skin", "skin", {}, false),
      tracker_button(skin, "Sound " + tracker_mode(state)["name"], "mode", {}, false)
    ]
  )
  numbers = row(
    {
      "gap": 3,
      "align": "center",
      "wrap": "wrap"
    },
    [
      tracker_field(skin, "BPM", str(state["bpm"])),
      tracker_button(skin, "-", "bpm", {"by": -5}, false),
      tracker_button(skin, "+", "bpm", {"by": 5}, false),
      tracker_field(skin, "Spd", str(state["speed"])),
      tracker_button(skin, "-", "speed", {"by": -1}, false),
      tracker_button(skin, "+", "speed", {"by": 1}, false),
      tracker_field(skin, "Oct", str(state["octave"])),
      tracker_button(skin, "-", "octave", {"by": -1}, false),
      tracker_button(skin, "+", "octave", {"by": 1}, false),
      tracker_field(skin, "Row", tracker_hex(state["row"], 2)),
      tracker_field(skin, "Head", state["play_row"] < 0 ? "--" : tracker_hex(state["play_row"], 2))
    ]
  )
  tracker_frame(skin, {
    "display": "column",
    "gap": 2,
    "pad": 2,
    "grow": 1,
    "shrink": 1,
    "min_width": 0
  }, [transport, numbers])
end

# One scope per channel, drawing the shape of whatever that channel is
# sounding. A tracker's scopes move; these say what the voice *is*, which
# is what a still picture of a tracker can honestly show.
def tracker_scope(state, skin, chan)
  cell = tracker_cell_at(state, tracker_sounding_row(state), chan)
  sounding = cell["n"] >= 0
  instrument = state["instruments"][cell["i"] - 1] ?? state["instruments"][0]
  wave = instrument["wave"]
  points = range(0, 30).map(fn(i) { [i * 2, 13 - tracker_wave_at(wave, i * 4369 % 65536, i) * 11 / 100] })
  line = [0, sounding ? skin["inst"] : skin["dim"], 1].concat(flatten_points(points))
  tracker_frame(skin, {"bg": skin["grid"], "pad": 0}, [canvas(60, 26, [line])])
end

def tracker_scopes(state, skin)
  row(
    {
      "gap": 1,
      "wrap": "wrap",
      "shrink": 1
    },
    range(0, TRACKER_CHANNELS).map(fn(c) { tracker_scope(state, skin, c) })
  )
end

# The instrument list: a number, a name, and the shape it plays.
def tracker_instrument_row(state, skin, i)
  instrument = state["instruments"][i]
  chosen = state["inst"] == i + 1
  line = row(
    {
      "gap": 2,
      "align": "center",
      "width": "100%",
      "pad": [0, 1, 0, 1],
      "radius": skin["radius"],
      "bg": chosen ? skin["cursor"] : "none",
      "cursor": "pointer"
    },
    [
      tracker_mono(tracker_hex(i + 1, 2), chosen ? skin["on_cursor"] : skin["dim"], 0),
      tracker_mono(instrument["name"] == "" ? "—" : instrument["name"], chosen ? skin["on_cursor"] : skin["inst"], 0),
      spacer(),
      tracker_mono(TRACKER_WAVES[instrument["wave"]], chosen ? skin["on_cursor"] : skin["dim"], 0)
    ]
  )
  line["on"] = {"click": "inst"}
  line["p"] = {"id": i + 1}
  line
end

def tracker_instruments_panel(state, skin)
  waves = row(
    {"gap": 1, "wrap": "wrap"},
    range(0, TRACKER_WAVES.length()).map(fn(w) {
      tracker_button(skin, TRACKER_WAVES[w], "wave", {"id": w}, false)
    })
  )
  tracker_frame(skin, {
    "display": "column",
    "gap": 1,
    "pad": 2,
    "bg": skin["panel"],
    "width": 280,
    "shrink": 0
  }, [
    tracker_label(skin, "Instruments"),
    tracker_frame(skin, {
      "bg": skin["grid"],
      "pad": 1,
      "display": "column",
      "width": "100%"
    }, range(0, state["instruments"].length()).map(fn(i) { tracker_instrument_row(state, skin, i) })),
    waves
  ])
end

# ------------------------------------------------------------ the pattern

TRACKER_NOTE_W = 34
TRACKER_INST_W = 24
TRACKER_VOL_W = 24
TRACKER_FX_W = 34

def tracker_cell_nodes(state, skin, at, chan)
  cell = tracker_cell_at(state, at, chan)
  here = state["row"] == at && state["chan"] == chan
  parts = [
    {
      "w": TRACKER_NOTE_W,
      "t": tracker_note_text(cell["n"]),
      "c": cell["n"] < 0 ? skin["dim"] : skin["note"]
    },
    {
      "w": TRACKER_INST_W,
      "t": cell["i"] > 0 ? tracker_hex(cell["i"], 2) : "..",
      "c": cell["i"] > 0 ? skin["inst"] : skin["dim"]
    },
    {
      "w": TRACKER_VOL_W,
      "t": cell["v"] > 0 ? tracker_hex(cell["v"], 2) : "..",
      "c": cell["v"] > 0 ? skin["vol"] : skin["dim"]
    },
    {
      "w": TRACKER_FX_W,
      "t": cell["f"] >= 0 ? tracker_hex(cell["f"], 1) + tracker_hex(cell["p"], 2) : "...",
      "c": cell["f"] >= 0 ? skin["fx"] : skin["dim"]
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
        "radius": lit ? skin["radius"] : 0,
        "bg": lit ? skin["cursor"] : "none"
      },
      "c": [tracker_mono(part["t"], lit ? skin["on_cursor"] : part["c"], 0)]
    }
  })
end

# Four things want to be visible at once on the same line: the row the
# cursor is on, the cell in it, the character group being typed, and the
# row the music has reached — which may be the same row. They are drawn as
# four nested washes, strongest innermost, and the row number keeps the
# cursor's colour whatever the line under it is doing, so the two are never
# confused for one another.
def tracker_row_line(state, skin, at, playing_row, first, across)
  here = at == state["row"]
  background = "none"
  background = skin["row4"] if at % 4 == 0
  background = skin["row16"] if at % 16 == 0
  background = skin["line"] if here
  background = skin["play"] if at == playing_row
  cells = range(first, first + across).map(fn(c) {
    row(
      {
        "gap": 0,
        "width": TRACKER_CELL_W,
        "shrink": 0,
        "radius": skin["radius"],
        "bg": here && state["chan"] == c ? skin["cell"] : "none"
      },
      tracker_cell_nodes(state, skin, at, c)
    )
  })
  line = row(
    {
      "gap": 0,
      "height": TRACKER_ROW_H,
      "align": "center",
      "bg": background,
      "width": "100%",
      "cursor": "pointer"
    },
    [{
      "k": "box",
      "s": {
        "width": TRACKER_GUTTER_W,
        "display": "row",
        "shrink": 0,
        "bg": here ? skin["cursor"] : "none"
      },
      "c": [tracker_mono(tracker_hex(at, 2), here ? skin["on_cursor"] : skin["dim"], 0)]
    }].concat(cells)
  )
  keyed("r" + str(at), line)
end

# How many channels fit across, and which one they start at. A tracker
# does not scroll its pattern: the cursor moves and the visible channels
# follow it, which is also what lets the arrow keys belong to the
# application — a scroller in the tree would take them first (03 §3).
def tracker_across(state)
  width = (state["viewport"] ?? {})["width"] ?? 1280
  fits = int((width - TRACKER_GUTTER_W - 24) / TRACKER_CELL_W)
  fits = 1 if fits < 1
  fits = TRACKER_CHANNELS if fits > TRACKER_CHANNELS
  fits
end

def tracker_first_channel(state)
  across = tracker_across(state)
  first = state["chan"] - int(across / 2)
  first = 0 if first < 0
  first = TRACKER_CHANNELS - across if first > TRACKER_CHANNELS - across
  first
end

def tracker_visible(state)
  height = (state["viewport"] ?? {})["height"] ?? 800
  # What the furniture above and below takes: the panels, the scopes, the
  # heading strip and the status line.
  fits = int((height - 300) / TRACKER_ROW_H)
  fits = 8 if fits < 8
  fits = TRACKER_ROWS if fits > TRACKER_ROWS
  fits
end

def tracker_window_top(state)
  visible = tracker_visible(state)
  centre = state["row"]
  centre = state["play_row"] if state["playing"] && state["follow"] && state["play_row"] >= 0
  top = centre - int(visible / 2)
  top = 0 if top < 0
  top = TRACKER_ROWS - visible if top > TRACKER_ROWS - visible
  top
end

# The heading strip over the channels, so the eye can find the one it is
# typing in. It scrolls with the pattern, so it stays over its own column.
# The heading strip over the channels that are on screen, so the eye can
# find the column it is typing in — and see that there are others.
def tracker_channel_heads(state, skin, first, across)
  heads = range(first, first + across).map(fn(c) {
    {
      "k": "box",
      "s": {
        "width": TRACKER_CELL_W,
        "display": "row",
        "justify": "center",
        "shrink": 0,
        "radius": skin["radius"],
        "bg": state["chan"] == c ? skin["cursor"] : "none"
      },
      "c": [tracker_mono("Channel " + str(c + 1), state["chan"] == c ? skin["on_cursor"] : skin["dim"], 0)]
    }
  })
  row(
    {
      "gap": 0,
      "width": "100%",
      "bg": skin["head"]
    },
    [{
      "k": "box",
      "s": {
        "width": TRACKER_GUTTER_W,
        "display": "row",
        "shrink": 0
      },
      "c": [tracker_mono("", skin["dim"], 0)]
    }].concat(heads)
  )
end

# The pattern fills what the window has left. It never scrolls: the rows
# it shows are the rows that fit, centred on the cursor, and the channels
# it shows follow the cursor across. A scroller here would take the arrow
# keys before the application saw them (03 §3).
def tracker_pattern(state, skin)
  visible = tracker_visible(state)
  top = tracker_window_top(state)
  first = tracker_first_channel(state)
  across = tracker_across(state)
  playing_row = state["playing"] ? state["play_row"] : -1
  lines = range(top, top + visible).map(fn(r) { tracker_row_line(state, skin, r, playing_row, first, across) })
  grid = {
    "k": "box",
    "s": {
      "display": "column",
      "gap": 0,
      "bg": skin["grid"],
      "pad": 0,
      "width": "100%",
      "grow": 1,
      "overflow": "clip"
    },
    "on": {"click": "click", "key_down": "key"},
    "p": {"top": top, "first": first},
    "c": [tracker_channel_heads(state, skin, first, across)].concat(lines)
  }
  tracker_frame(skin, {
    "display": "column",
    "bg": skin["grid"],
    "width": "100%",
    "grow": 1
  }, [grid])
end

def tracker_status(state, skin)
  tracker_frame(skin, {
    "display": "row",
    "gap": 3,
    "align": "center",
    "pad": [0, 2, 0, 2],
    "width": "100%"
  }, [
    tracker_label(skin, state["status"]),
    spacer(),
    tracker_label(skin, "Z-M / Q-U notes · arrows move · Space plays · type while it plays")
  ])
end

def tracker_view(raw_state)
  state = tracker_defaults(raw_state ?? {})
  skin = tracker_skin(state)
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
    [
      row(
        {
          "gap": 2,
          "align": "start",
          "wrap": "wrap"
        },
        [tracker_plate(skin), tracker_top(state, skin), tracker_song_panel(state, skin)]
      ),
      tracker_scopes(state, skin)
    ]
  )
  head = wide ? row(
    {
      "gap": 2,
      "align": "start",
      "width": "100%"
    },
    [left, tracker_instruments_panel(state, skin)]
  ) : column(
    {"gap": 2, "width": "100%"},
    [left, tracker_instruments_panel(state, skin)]
  )
  page = column(
    {
      "gap": 2,
      "pad": skin["pad"],
      "bg": skin["desk"],
      "width": "100%",
      "height": "100%",
      "min_height": 420
    },
    [sound, head, tracker_pattern(state, skin), tracker_status(state, skin)]
  )
  # A window too short for the panels can be scrolled — with the wheel or
  # the bar, never with the arrows, which belong to the pattern (03 §3).
  {
    "k": "scroll",
    "s": {
      "width": "100%",
      "height": "100%",
      "bg": skin["desk"]
    },
    "c": [page]
  }
end

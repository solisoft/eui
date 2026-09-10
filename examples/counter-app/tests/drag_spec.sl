# The drag itself: which events move a divider and which are ignored.
#
# The one that matters is the first — a pointer_move with no press must do
# nothing, or dragging a scrollbar or selecting text inside a panel would
# move the divider instead.

# ---- copied from app/controllers/eui_builders.sl, do not edit ----

def split_span(extent, bar)
  span = extent - bar
  span < 0 ? 0 : span
end

def split_sizes(extent, fraction, min_a, min_b, bar)
  span = split_span(extent, bar)
  return [0, 0] if span <= 0

  a = int((span * fraction / 1000.0).round())
  room = span - min_b
  a = room if a > room
  a = min_a if a < min_a
  a = 0 if a < 0
  a = span if a > span
  [a, span - a]
end

def split_at(extent, at, min_a, min_b, bar)
  span = split_span(extent, bar)
  return 500 if span <= 0

  a = int(at) - int(bar / 2)
  room = span - min_b
  a = room if a > room
  a = min_a if a < min_a
  a = 0 if a < 0
  a = span if a > span
  int((a * 1000.0 / span).round())
end

def split_event(state, params, name, dir, extent, min_a, min_b, bar)
  kind = params["kind"]
  drag = name + "_drag"
  step = 25

  if kind == "pointer_down"
    state[drag] = true
  elsif kind == "pointer_up"
    state[drag] = false
  elsif kind == "pointer_move"
    if state[drag] ?? false
      payload = params["payload"] ?? [0, 0]
      at = dir == "row" ? payload[0] : payload[1]
      state[name] = split_at(extent, at, min_a, min_b, bar)
    end
  elsif kind == "key_down"
    payload = params["payload"] ?? [""]
    pressed_key = payload[0]
    back = dir == "row" ? "ArrowLeft" : "ArrowUp"
    fwd = dir == "row" ? "ArrowRight" : "ArrowDown"
    current = state[name] ?? 500
    state[name] = current - step if pressed_key == back
    state[name] = current + step if pressed_key == fwd
    state[name] = 500 if pressed_key == "Home"
    state[name] = 0 if state[name] < 0
    state[name] = 1000 if state[name] > 1000
  end
  state
end

def check(label, got, want)
  if got == want
    print("ok   " + label)
  else
    print("FAIL " + label + " got " + got.to_s + " want " + want.to_s)
  end
end

def ev(kind, payload)
  {"kind": kind, "payload": payload}
end

s = {"sx": 500, "sx_drag": false}

# A move with no press does nothing: the panels only follow a real drag.
s = split_event(s, ev("pointer_move", [700, 40]), "sx", "row", 806, 80, 80, 6)
check("move without a press is ignored", s["sx"], 500)

# Press, move, release.
s = split_event(s, ev("pointer_down", [3, 40]), "sx", "row", 806, 80, 80, 6)
check("press starts the drag", s["sx_drag"], true)
s = split_event(s, ev("pointer_move", [603, 40]), "sx", "row", 806, 80, 80, 6)
check("move sets the fraction", s["sx"], 750)
s = split_event(s, ev("pointer_up", [603, 40]), "sx", "row", 806, 80, 80, 6)
check("release ends the drag", s["sx_drag"], false)
s = split_event(s, ev("pointer_move", [200, 40]), "sx", "row", 806, 80, 80, 6)
check("move after release is ignored", s["sx"], 750)

# A drag that runs off the end stops at the floor rather than inverting.
s = split_event(s, ev("pointer_down", [3, 40]), "sx", "row", 806, 80, 80, 6)
s = split_event(s, ev("pointer_move", [5000, 40]), "sx", "row", 806, 80, 80, 6)
check("a drag past the end clamps", s["sx"], 900)
s = split_event(s, ev("pointer_move", [0 - 500, 40]), "sx", "row", 806, 80, 80, 6)
check("a drag before the start clamps", s["sx"], 100)

# The keyboard moves it too, on the axis that matches the direction.
k = {"sy": 500, "sy_drag": false}
k = split_event(k, ev("key_down", ["ArrowDown"]), "sy", "column", 406, 60, 60, 6)
check("ArrowDown grows the first panel", k["sy"], 525)
k = split_event(k, ev("key_down", ["ArrowUp"]), "sy", "column", 406, 60, 60, 6)
check("ArrowUp shrinks it back", k["sy"], 500)
k = split_event(k, ev("key_down", ["ArrowRight"]), "sy", "column", 406, 60, 60, 6)
check("the other axis is ignored", k["sy"], 500)
k = split_event(k, ev("key_down", ["ArrowUp"]), "sy", "column", 406, 60, 60, 6)
k = split_event(k, ev("key_down", ["Home"]), "sy", "column", 406, 60, 60, 6)
check("Home recentres", k["sy"], 500)

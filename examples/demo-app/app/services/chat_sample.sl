# Atrium's cast, its rooms, and the conversation the seed writes into them.
#
# This is a *service* and not part of the controller, because two different
# things need it and only one of them is a running server: `soli db:seed`
# loads `app/models` and `app/services` and nothing else, so a generator that
# lived beside the views could not be reached from a seed — and a seed that
# carried its own copy of the room list would drift from the app's the first
# time anyone renamed a channel.
#
# Nothing here touches the database. It answers "what would message 812 of
# #general say", and `db/seeds.sl` writes the answers down.

# --------------------------------------------------------------- the cast
# Names carry their own colour and initial, so an avatar never needs a file
# and two people are never the same blue.

CHAT_PEOPLE = [
  "Camille Roy", "Ines Okafor", "Théo Lindqvist", "Mara Bellini",
  "Yusuf Demir", "Anneke de Vries", "Rafael Costa", "Nour Haddad",
  "Sven Halvorsen", "Priya Raman", "Owen Gallagher", "Lucia Ferreira"
]

CHAT_TONES = ["accent.base", "info.base", "success.base", "warning.base", "danger.base"]

# Who this window is. One per session, so the second window on the same
# server is someone else and the room has two people in it.
CHAT_ME_DEFAULT = 0

def chat_person(i)
  CHAT_PEOPLE[i % CHAT_PEOPLE.length()]
end

def chat_tone(i)
  CHAT_TONES[(i * 7) % CHAT_TONES.length()]
end

def chat_initial(i)
  chat_person(i)[0]
end

# --------------------------------------------------------------- the rooms

CHAT_SPACES = [
  {"id": "atrium", "name": "Atrium", "tone": "accent.base"},
  {"id": "field", "name": "Field Ops", "tone": "success.base"},
  {"id": "labs", "name": "Labs", "tone": "warning.base"}
]

CHAT_ROOMS = [
  {"id": "general", "name": "general", "kind": "channel", "topic": "Anything that does not have a room yet", "members": 34},
  {"id": "protocol", "name": "protocol", "kind": "channel", "topic": "Wire format, ops, budgets", "members": 19},
  {"id": "design", "name": "design", "kind": "channel", "topic": "Roles, spacing, the things people see", "members": 22},
  {"id": "release", "name": "release", "kind": "channel", "topic": "What ships, and when", "members": 12},
  {"id": "incidents", "name": "incidents", "kind": "channel", "topic": "Only while something is on fire", "members": 41},
  {"id": "dm-ines", "name": "Ines Okafor", "kind": "dm", "who": 1, "topic": "", "members": 2},
  {"id": "dm-theo", "name": "Théo Lindqvist", "kind": "dm", "who": 2, "topic": "", "members": 2},
  {"id": "dm-mara", "name": "Mara Bellini", "kind": "dm", "who": 3, "topic": "", "members": 2}
]

def chat_room(id)
  found = CHAT_ROOMS.filter(fn(r) { r["id"] == id })
  found.length() > 0 ? found[0] : CHAT_ROOMS[0]
end

def chat_rooms_of(kind)
  CHAT_ROOMS.filter(fn(r) { r["kind"] == kind })
end

# ------------------------------------------------------- the derived past
# Message `i` of a room is a function of `i`. Nothing is stored, so a room
# with ten thousand messages behind it costs the server one arithmetic per
# row the window actually asks for — the same bargain `feed_post` makes.

CHAT_HISTORY = 4000

CHAT_LINES = [
  "Pushed the windowing fix — the fling lands on cards now instead of the sunken placeholder.",
  "Anyone else seeing the atlas evict on the second theme toggle?",
  "The 50-row table is 4 619 B against 14 362 B of HTML. I keep re-measuring it and it keeps being true.",
  "Reminder that the byte budget *is* the design review. If the frame got bigger, the design got worse.",
  "I moved the composer to surface.raised. It reads as a thing you act in rather than a thing you read.",
  "Shaping cache hit rate is 94% on a scroll now. It was 61% before the intern pass.",
  "Do we want the day separator sticky? Spec 04 has no sticky, so: no, and I think that is the right no.",
  "Landed the keyboard focus walk. Document order, wraps at the end, Escape drops it.",
  "Two windows on one server and they see each other. It polls, but it polls honestly.",
  "That crash was the waker, except it was not the waker. Reverted the fix that said otherwise.",
  "Every widget in the catalogue is a plain function returning a hash. Still no native code anywhere.",
  "Can we get `#` into the icon set? I am drawing channels with a text glyph and it shows.",
  "The manifest is pinned on first use now, and rotation needs the old key's blessing.",
  "Dark mode costs zero bytes on the wire. The server never learns which one you are in.",
  "Re-ran the budgets on the little ARM box. Everything inside except the first paint, which is 12 ms over.",
  "Split drag is a round trip a frame. Fine on loopback, visibly not fine over a real link.",
  "I think the thread panel should keep its width per channel rather than globally. Opinions?",
  "Uploads land on disk as they arrive — two chunks held, whatever the file weighs.",
  "Fonts are embedded and we enumerate nothing, so the fingerprint story is actually testable.",
  "Finally: a message that arrives scrolls the river, because the server can say ScrollTo now.",
  "The 21 icons are starting to hurt. Everything else I draw as a glyph or a canvas path.",
  "Renamed it Atrium. Every other demo here has a name and this one was just 'chat'.",
  "Grouping consecutive messages by author is most of what makes this look designed.",
  "Reordered the rail so the workspace you were in is the one you come back to.",
  "Six hundred rows a second through the diff on this laptop. The encoder is not the bottleneck.",
  "A wheel notch is 100 px eased over 180 ms, and a Magic Mouse sends zeros in between.",
  "Presence is a timestamp and nothing else. Anything older than six seconds is simply not there.",
  "Put the unread rule in danger.base. It is the one thing on the page you are meant to lose your place at."
]

CHAT_SHORT = [
  "agreed", "on it", "nice", "that reads much better", "yes please",
  "will look after standup", "hm", "good catch", "shipping it", "same here",
  "one more pass and it is done", "I owe you a coffee for that one"
]

# Four shapes of message, decided by the number alone so a row's height is
# known without the row being built.
def chat_shape(room_id, i)
  n = i + room_id.length()
  return "file" if n % 53 == 0
  return "link" if n % 31 == 0
  return "short" if n % 3 == 0

  "line"
end

def chat_text_of(room_id, i)
  shape = chat_shape(room_id, i)
  return CHAT_SHORT[(i * 5) % CHAT_SHORT.length()] if shape == "short"
  return "Worth a read before Thursday: https://eui.solisoft.net/spec/04-layout" if shape == "link"
  return "Here is the capture from the run that went wrong." if shape == "file"

  CHAT_LINES[(i * 11 + room_id.length()) % CHAT_LINES.length()]
end

# The past runs backwards from now at an uneven pace, so a day separator
# falls somewhere sensible and a burst of three messages in one minute looks
# like a burst of three messages in one minute.
CHAT_NOW = 0

def chat_now
  CHAT_NOW = DateTime.now().to_unix() if CHAT_NOW == 0
  CHAT_NOW
end

# How much time has passed by row `i`. Every term is non-decreasing in `i`
# and the first is strictly increasing, so the past runs one way — which is
# not true of the obvious `(i % 7) * 900`, where a modulus that wraps sends
# a message backwards past the one before it and every day break with it.
def chat_elapsed(i)
  i * 47 + (i / 7) * 900 + (i / 23) * 60
end

def chat_at(room_id, i)
  chat_now() - chat_elapsed(CHAT_HISTORY - 1) + chat_elapsed(i)
end

# Who is speaking. It has to be the *same* person for two or three rows at a
# time or nothing ever groups: an author derived straight from `i` changes on
# every line, and a messenger where every message carries its own avatar is
# the list of cards this is trying not to be.
def chat_run(i)
  i / 3
end

def chat_who(room_id, i)
  room = chat_room(room_id)
  return (chat_run(i) % 2 == 0) ? (room["who"] ?? 1) : 0 if room["kind"] == "dm"

  (chat_run(i) * 13 + (i / 11) * 5 + room_id.length()) % CHAT_PEOPLE.length()
end

# One message of the derived past. `id` is what a reaction and a thread hang
# off, and it is stable because `i` is.
def chat_past(room_id, i)
  {
    "id": room_id + ":" + str(i),
    "n": i,
    "who": chat_who(room_id, i),
    "at": chat_at(room_id, i),
    "text": chat_text_of(room_id, i),
    "shape": chat_shape(room_id, i),
    "live": false
  }
end

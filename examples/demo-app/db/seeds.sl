# Atrium's rooms, written down.
#
#   soli db:migrate up      # once, to make the collections
#   soli db:seed            # this file
#
# Seeds re-run every time and are not tracked, so this one is idempotent: a
# room that already has its messages is left alone. Re-seeding after adding a
# channel fills only the new one.
#
# The conversation itself comes from `app/services/chat_sample.sl`, which
# `db:seed` loads along with the models — so the room list here is the room
# list the application draws, and cannot drift from it.

# How much history each room gets. Four thousand is not for show: it is what
# makes the windowed list mean something. The client holds one height per
# row and the server builds only the rows in view, so a room this long costs
# a window and not a room.
SEED_PER_ROOM = 4000

# Documents per insert. One round trip per message would be forty thousand of
# them; one insert of forty thousand is a request nothing should have to
# parse. A thousand is neither.
SEED_BATCH = 1000

# A count against a collection nobody has made yet comes back as the error
# **string**, not as a number and not as a raise — so the comparison below
# used to die with "Cannot compare string and int", and a virgin database
# could not be seeded at all. A room whose count cannot be read is a room with
# nothing in it, which is exactly what needs filling.
def seed_count(room_id)
  seed_answer = ChatMessage.where({ "room": room_id }).count() rescue 0
  return 0 unless seed_answer.class == "int"

  seed_answer
end

def seed_room(room_id)
  seed_have = seed_count(room_id)
  if seed_have >= SEED_PER_ROOM
    print("  " + room_id + ": " + str(seed_have) + " already, left alone")
    return 0
  end

  seed_from = seed_have
  seed_at = seed_from
  while seed_at < SEED_PER_ROOM
    seed_upto = seed_at + SEED_BATCH
    seed_upto = SEED_PER_ROOM if seed_upto > SEED_PER_ROOM
    seed_rows = range(seed_at, seed_upto).map(fn(i) {
      chat_past(room_id, i).merge({ "room": room_id })
    })
    ChatMessage.create_many(seed_rows)
    seed_at = seed_upto
  end
  print("  " + room_id + ": wrote " + str(SEED_PER_ROOM - seed_from))
  SEED_PER_ROOM - seed_from
end

print("Seeding Atrium")
seed_total = 0
for seed_room_def in CHAT_ROOMS
  seed_total = seed_total + seed_room(seed_room_def["id"])
end

# The counter every session watches. One document, one number: a tick asks
# whether it has moved instead of reading a room to find out.
if ChatMeta.find_by("key", "seq").nil?
  ChatMeta.create({ "key": "seq", "value": 0 })
end

print("Seeded " + str(seed_total) + " messages across " + str(CHAT_ROOMS.length()) + " rooms")

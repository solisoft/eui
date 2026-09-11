# Atrium's arithmetic.
#
#   soli test tests/chat_spec.sl --no-coverage
#
# Everything checked here lives in `app/services/chat_measure.sl`, which
# `soli test` preloads along with the models — so there is nothing to import
# and, unlike `split_spec.sl`, nothing copied in that can drift.
#
# What is worth testing is what the screen cannot show you is wrong. A row
# height that disagrees with the row it measures does not look broken; it
# slides every row under it, a few pixels at a time, and you find out from a
# scroll bar that lies. A grouping rule off by one turns a block of six
# messages into six cards, which looks like a design choice.

def a_message(who, at, text, shape)
  { "id": "general:1", "n": 1, "who": who, "at": at, "text": text, "shape": shape }
end

describe("Atrium's arithmetic") do
  test("a short line wraps to one, a long one to more") do
  assert_eq(chat_wrap_lines("ok", 900), 1)
  assert_eq(chat_wrap_lines("x" * 400, 420) > 1, true)
  end

  test("a narrower column never wraps less far") do
  wide = chat_wrap_lines("x" * 300, 900)
  narrow = chat_wrap_lines("x" * 300, 360)
  assert_eq(narrow >= wide, true)
  end

  test("a reader who made the text bigger gets more lines, not clipped ones") do
    # The bug this pins: the estimate ignored the viewer's font scale, so at
    # 1.5 it thought a line held forty-six characters when it held thirty —
    # the body was clamped short and the last words of a message were simply
    # not drawn, for exactly the people who had asked for larger text.
    normal = chat_wrap_lines_at("x" * 300, 800, 1.0)
    bigger = chat_wrap_lines_at("x" * 300, 800, 1.5)
    assert_eq(bigger > normal, true)
  end

  test("a taller line makes a taller row") do
    one = a_message(3, 1000000, "hello", "line")
    small = chat_height_at(one, nil, 800, false, 0, CHAT_LINK_PX, 1.0)
    large = chat_height_at(one, nil, 800, false, 0, CHAT_LINK_PX, 1.5)
    assert_eq(large > small, true)
  end

  test("a body is never clamped shorter than the words need") do
    # Text wraps at words, so a line never fills completely — and an estimate
    # that assumed it did clamped a ninety-three character message to two
    # lines when it takes three, and the last word was not drawn. Erring long
    # costs a few pixels; erring short costs the message.
    said = "Presence is a timestamp and nothing else. Anything older than six seconds is simply not there."
    assert_eq(chat_wrap_lines_at(said, 352, 1.0) >= 3, true)
  end

  test("nothing wraps past the ceiling, whatever the width") do
  assert_eq(chat_wrap_lines("x" * 100000, 400), CHAT_MAX_LINES)
  assert_eq(chat_wrap_lines("x" * 100000, 10) <= CHAT_MAX_LINES, true)
  end

# ---- grouping ------------------------------------------------------------

  test("the same person, moments apart, is one block") do
  first = a_message(3, 1000000, "one", "line")
  second = a_message(3, 1000060, "two", "line")
  assert_eq(chat_pair_grouped?(second, first), true)
  end

  test("a different person is never a continuation") do
  first = a_message(3, 1000000, "one", "line")
  second = a_message(4, 1000060, "two", "line")
  assert_eq(chat_pair_grouped?(second, first), false)
  end

  test("the same person, far enough apart, is not") do
  first = a_message(3, 1000000, "one", "line")
  second = a_message(3, 1000000 + CHAT_GROUP_WITHIN + 1, "two", "line")
  assert_eq(chat_pair_grouped?(second, first), false)
  end

  test("the first row of a room is never a continuation") do
  assert_eq(chat_pair_grouped?(a_message(3, 1000000, "one", "line"), nil), false)
  end

  test("a new day opens a block and is never a continuation") do
  # 86 400 seconds apart is a day by construction, which is the whole of what
  # `chat_day` claims — it is a division, not a calendar, because the height
  # pass asks four times per row and a room has four thousand of them.
  first = a_message(3, 86400 * 10 - 30, "late", "line")
  second = a_message(3, 86400 * 10 + 30, "early", "line")
  assert_eq(chat_pair_day_break?(second, first), true)
  assert_eq(chat_pair_grouped?(second, first), false)
  end

  test("the first row always opens a day") do
  assert_eq(chat_pair_day_break?(a_message(3, 1000000, "one", "line"), nil), true)
  end

# ---- heights -------------------------------------------------------------

  test("a row is at least a line tall and always positive") do
  one = chat_height_of(a_message(3, 1000000, "hello", "line"), nil, 800, false, 0, CHAT_LINK_PX)
  assert_eq(one > CHAT_LINE_PX, true)
  end

  test("a continuation is shorter than an opening row by its header") do
  first = a_message(3, 1000000, "hello", "line")
  second = a_message(3, 1000060, "hello", "line")
  opening = chat_height_of(second, nil, 800, false, 0, CHAT_LINK_PX)
  running = chat_height_of(second, first, 800, false, 0, CHAT_LINK_PX)
  # `nil` means "no row above", which is a day break as well as a new block,
  # so the difference is both.
  assert_eq(opening - running, CHAT_HEADER_PX + CHAT_DAY_PX)
  end

  test("what is stuck to a row adds exactly its own height") do
  first = a_message(3, 1000000, "hello", "line")
  plain = chat_height_of(a_message(3, 1000060, "hello", "line"), first, 800, false, 0, CHAT_LINK_PX)
  marked = chat_height_of(a_message(3, 1000060, "hello", "line"), first, 800, true, 0, CHAT_LINK_PX)
  answered = chat_height_of(a_message(3, 1000060, "hello", "line"), first, 800, false, 4, CHAT_LINK_PX)
  filed = chat_height_of(a_message(3, 1000060, "hello", "file"), first, 800, false, 0, CHAT_LINK_PX)
  linked = chat_height_of(a_message(3, 1000060, "hello", "link"), first, 800, false, 0, CHAT_LINK_PX)
  assert_eq(marked - plain, CHAT_REACTION_PX)
  assert_eq(answered - plain, CHAT_REPLIES_PX)
  assert_eq(filed - plain, CHAT_FILE_PX)
  assert_eq(linked - plain, CHAT_LINK_PX)
  end

  test("the same row measures the same twice") do
  one = a_message(3, 1000060, "hello", "line")
  before = a_message(3, 1000000, "hi", "line")
  assert_eq(
    chat_height_of(one, before, 800, false, 0, CHAT_LINK_PX),
    chat_height_of(one, before, 800, false, 0, CHAT_LINK_PX)
  )
  end

# ---- the foot ------------------------------------------------------------
# What `scroll_to` is handed. It is the whole content height and not the
# overflow, because the server does not know the height the client gave the
# list — the client clamps it to the last row.

  test("the foot is the whole content") do
  assert_eq(chat_foot([100, 100, 100]), 300)
  end

  test("an empty room has nowhere to go") do
  assert_eq(chat_foot([]), 0)
  end

# Holding a place instead of following the foot: what is above a row is
# where the list stands for that row to be at the top. Unrolling a page adds
# a hundred rows above the one being read, and this is what puts it back.

  test("the offset of a row is what is above it") do
  assert_eq(chat_upto([10, 20, 30, 40], 0), 0)
  assert_eq(chat_upto([10, 20, 30, 40], 2), 30)
  assert_eq(chat_upto([10, 20, 30, 40], 4), 100)
  assert_eq(chat_upto([10, 20, 30, 40], 9), 100)
  assert_eq(chat_upto([], 3), 0)
  end

# ---- links ---------------------------------------------------------------

  test("a message with no link has none") do
  assert_eq(chat_link_in("just words"), nil)
  end

  test("a link is found wherever it sits") do
  assert_eq(chat_link_in("see https://eui.solisoft.net/x now"), "https://eui.solisoft.net/x")
  assert_eq(chat_link_in("see https://eui.solisoft.net/x"), "https://eui.solisoft.net/x")
  end

  test("an unfurl names its host") do
  assert_eq(chat_unfurl("https://eui.solisoft.net/spec/04")["host"], "eui.solisoft.net")
  end

  test("what a message is, is decided by what is in it") do
  assert_eq(chat_shape_of("plain words", nil), "line")
  assert_eq(chat_shape_of("see https://eui.solisoft.net/x", nil), "link")
  assert_eq(chat_shape_of("plain words", { "name": "a.png" }), "file")
  end

# ---- files ---------------------------------------------------------------

  test("a picture is told from a log by its extension") do
  assert_eq(chat_picture?("shot.png"), true)
  assert_eq(chat_picture?("SHOT.PNG"), true)
  assert_eq(chat_picture?("run.log"), false)
  assert_eq(chat_picture?("Makefile"), false)
  end

  test("a weight reads in the unit it deserves") do
  assert_eq(chat_weight(900), "900 B")
  assert_eq(chat_weight(2048), "2 KB")
  assert_eq(chat_weight(3145728), "3 MB")
  end
end

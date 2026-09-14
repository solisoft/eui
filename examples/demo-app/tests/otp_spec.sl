# OTP algebra, tested without a client.
#
#   soli tests/otp_spec.sl
#
# Copied in by `tools/sync_split_spec.py`.

# ---- copied from app/controllers/eui_builders.sl, do not edit ----

def otp_clean(text, o = {})
  n = o["digits"] ?? 6
  numeric = o["numeric"] != false
  said = (text ?? "").to_s
  out = ""
  i = 0
  while i < said.length() && out.length() < n
    ch = said.substring(i, i + 1)
    ok = numeric ? "0123456789".includes?(ch) : (ch != " " && ch != "\n")
    out = out + ch if ok
    i = i + 1
  end
  out
end

def otp_take(value, text, o = {})
  incoming = otp_clean(text, o)
  return value if incoming == ""
  return incoming if incoming.length() != 1

  otp_clean((value ?? "").to_s + incoming, o)
end

def otp_pop(value)
  said = (value ?? "").to_s
  return "" if said.length() <= 1

  said.substring(0, said.length() - 1)
end

def otp_jump(value, i)
  said = (value ?? "").to_s
  return said if i.nil?
  return "" if i <= 0
  return said if i >= said.length()

  said.substring(0, i)
end

def otp_apply(value, kind, payload, props, o = {})
  if kind == "text_input"
    return otp_take(value, payload.to_s, o)
  end
  if kind == "change"
    cell = otp_clean(payload.to_s, {"digits": 1, "numeric": o["numeric"]})
    return otp_pop(value) if cell == ""
    return value
  end
  if kind == "key_down"
    key = payload.class == "array" ? payload[0] : payload.to_s
    return otp_pop(value) if key == "Backspace"
    return value
  end
  return otp_jump(value, props["i"] ?? 0) if kind == "click"

  value
end

def check(label, got, want)
  if got == want
    print("ok   " + label)
  else
    a = got
    b = want
    a = "[" + got.join(", ") + "]" if got.class == "array"
    b = "[" + want.join(", ") + "]" if want.class == "array"
    print("FAIL " + label + " got " + a.to_s + " want " + b.to_s)
  end
end

# ---- clean ----

check("digits only", otp_clean("12a34"), "1234")
check("caps at six", otp_clean("123456789"), "123456")
check("caps at the asked length", otp_clean("123456", {"digits": 4}), "1234")
check("letters when asked", otp_clean("ab12", {"numeric": false}), "ab12")
check("spaces are not a code", otp_clean("1 2 3", {"numeric": false}), "123")
check("nothing typed is nothing", otp_clean(""), "")
check("a missing draft is nothing", otp_clean(null), "")

# ---- take ----

check("one digit appends", otp_take("12", "3"), "123")
check("a letter is ignored", otp_take("12", "a"), "12")
check("a paste of several replaces", otp_take("12", "847291"), "847291")
check("a paste is cleaned", otp_take("", "84-72-91"), "847291")
check("a paste longer than the field is cut", otp_take("", "123456789"), "123456")
check("appending onto a full code stays full", otp_take("123456", "7"), "123456")

# ---- pop / jump ----

check("pop the last", otp_pop("123"), "12")
check("pop one leaves nothing", otp_pop("1"), "")
check("pop nothing is nothing", otp_pop(""), "")
check("jump to the start", otp_jump("123456", 0), "")
check("jump keeps the prefix", otp_jump("123456", 2), "12")
check("jump past the end leaves it", otp_jump("12", 9), "12")
check("jump on nothing is nothing", otp_jump("", 2), "")

# ---- apply ----

check("text_input appends", otp_apply("12", "text_input", "3", {}, {}), "123")
check("paste through text_input", otp_apply("12", "text_input", "999111", {}, {}), "999111")
check("Backspace pops", otp_apply("123", "key_down", ["Backspace", 0], {}, {}), "12")
check("an empty change pops the last cell", otp_apply("123456", "change", "", {}, {}), "12345")
check("a click jumps", otp_apply("123456", "click", null, {"i": 2}, {}), "12")

# The HTML part of a message, as EUI nodes.
#
# Why this exists at all: a multipart mail carries a text part *and* an
# HTML part, and for anything sent by a machine the text part is a
# machine's idea of what the HTML said. It reads like this --
#
#   Bien'ici [https://mail-sender.bienici.com/static/emails/logo.png]
#   [https://www.bienici.com/?at_canal=CRM&at_medium=alertes&...]
#   X (ex-Twitter) [https://.../x_round_50.png]
#
# -- every image reduced to its URL, every link to a second URL beside it,
# and the sentence somewhere underneath. Flattening the HTML ourselves
# produced the same thing for the same reason. The fix is not a better
# flattener; it is to stop flattening and render the structure.
#
# This is a small HTML reader, not a browser. It knows paragraphs, line
# breaks, links, images and headings, and it throws away everything else
# -- which for mail is almost all of it, because the rest is table layout
# and inline CSS. `<style>` and `<script>` are skipped whole: a mail's
# stylesheet is longer than its prose and none of it means anything here.


# ------------------------------------------------- indices, in characters
#
# The trap this module is built around, and it costs a whole class of bugs
# in any mail that is not English.
#
# `length()` and `index_of()` count **bytes**; `substring()` counts
# **characters**. On ASCII they agree and everything works. On a Greek
# line they do not: `"αγγελία: X".length()` is 17 and `index_of("X")` is
# 16, but the string is ten characters, so cutting at 16 runs off the end
# and cutting at anything derived from a byte offset lands mid-word. The
# symptom was a Greek mail rendering as `α&#ελία` -- text eaten out of the
# middle of a word by a decoder that thought it was slicing somewhere else.
#
# So nothing here calls `index_of` or uses `length` as a bound. `mail_at`
# gives the character index of a needle by counting the characters of the
# part before it, and `mail_len` is the character length. Byte counts are
# still used where bytes are what is meant -- the 4096-byte text ceiling.
def mail_at(hay, needle)
  return -1 if !hay.contains(needle)

  bits = hay.split(needle)
  head = bits[0] ?? ""
  head.chars().length()
end

def mail_len(said)
  said.chars().length()
end

# ------------------------------------------------------------- entities
#
# `html_unescape` decodes `&amp;`, `&lt;`, `&gt;` and `&nbsp;` and stops
# there -- it leaves `&agrave;`, `&rsquo;`, `&euro;` and, more awkwardly,
# every numeric reference untouched. A French mail is then full of
# `&agrave;` and a Greek one prices a flat at `&#8364; 600`, which is how
# this was noticed. So the named ones people actually send are a table,
# and the numeric ones -- which cover everything else, in any language --
# are decoded properly.
MAIL_ENTITIES = [
  ["&apos;", "'"],
  ["&euro;", "€"],
  ["&pound;", "£"],
  ["&copy;", "©"],
  ["&reg;", "®"],
  ["&trade;", "™"],
  ["&hellip;", "…"],
  ["&mdash;", "—"],
  ["&ndash;", "–"],
  ["&bull;", "•"],
  ["&lsquo;", "‘"],
  ["&rsquo;", "’"],
  ["&ldquo;", "“"],
  ["&rdquo;", "”"],
  ["&laquo;", "«"],
  ["&raquo;", "»"],
  ["&middot;", "·"],
  ["&times;", "×"],
  ["&deg;", "°"],
  ["&agrave;", "à"],
  ["&aacute;", "á"],
  ["&acirc;", "â"],
  ["&auml;", "ä"],
  ["&aring;", "å"],
  ["&aelig;", "æ"],
  ["&egrave;", "è"],
  ["&eacute;", "é"],
  ["&ecirc;", "ê"],
  ["&euml;", "ë"],
  ["&igrave;", "ì"],
  ["&iacute;", "í"],
  ["&icirc;", "î"],
  ["&iuml;", "ï"],
  ["&ograve;", "ò"],
  ["&oacute;", "ó"],
  ["&ocirc;", "ô"],
  ["&ouml;", "ö"],
  ["&ugrave;", "ù"],
  ["&uacute;", "ú"],
  ["&ucirc;", "û"],
  ["&uuml;", "ü"],
  ["&ccedil;", "ç"],
  ["&ntilde;", "ñ"],
  ["&szlig;", "ß"],
  ["&Agrave;", "À"],
  ["&Eacute;", "É"],
  ["&Egrave;", "È"],
  ["&Ccedil;", "Ç"],
  # The invisible ones, and they matter more than the accented letters:
  # a marketing mail pads its preheader with hundreds of soft hyphens and
  # zero-width joiners so the preview line in a webmail stops after the
  # first sentence. Undecoded, they are not invisible at all -- they are
  # nine hundred literal `&shy;&zwnj;` filling the top of the letter,
  # which is what the reader showed. Decoded to nothing, they do what
  # they were sent to do.
  ["&shy;", ""],
  ["&zwnj;", ""],
  ["&zwj;", ""],
  ["&lrm;", ""],
  ["&rlm;", ""],
  ["&#8203;", ""],
  ["&ensp;", " "],
  ["&emsp;", " "],
  ["&thinsp;", " "]
]

def mail_entities(said)
  # Nothing to decode without an ampersand, and most blocks have none.
  #
  # This runs per block, and the table is sixty entries: a letter of eight
  # hundred blocks was forty-eight thousand searches of strings that could
  # not contain what was being looked for, plus a numeric pass and an
  # unescape each. One `contains` skips all of it.
  return said unless said.contains("&")

  out = said
  for pair in MAIL_ENTITIES
    out = out.replace(pair[0], pair[1]) if out.contains(pair[0])
  end
  out = mail_numeric(out)
  html_unescape(out) rescue out
end

# `&#8364;` and `&#x20AC;`, decoded to the character they name.
def mail_numeric(said)
  return said if !said.contains("&#")

  parts = said.split("&#")
  out = parts[0] ?? ""
  i = 1
  while i < parts.length()
    part = parts[i] ?? ""
    bits = part.split(";")
    code = bits[0] ?? ""
    num = 0
    num = mail_codepoint(code) if bits.length() > 1 && mail_len(code) <= 8
    if num > 0
      out = out + num.chr() + bits.slice(1, bits.length()).join(";")
    else
      out = out + "&#" + part
    end
    i = i + 1
  end
  out
end

MAIL_HEX = "0123456789abcdef"

def mail_codepoint(code)
  low = code.downcase()
  if low.starts_with("x")
    num = 0
    for ch in low.chars().slice(1, low.chars().length())
      at = mail_at(MAIL_HEX, ch)
      return 0 if at < 0

      num = num * 16 + at
    end
    return num
  end
  got = int(code) rescue 0
  got
end

# --------------------------------------------------------------- tokens

# `<` splits HTML into "the text before the first tag" and then, for each
# piece, "a tag, then the text after it". That is the whole tokenizer, and
# it is enough because everything this reads about a tag is in its name
# and its attributes.
# Comments out, before anything is split on `<`.
#
# A mail's comments are not small: a template's build notes, a tracking
# block, and -- the one that showed -- a preheader written as a comment
# around the text it hides. The tokenizer splits on `<`, so a comment
# whose body holds one of those breaks in two: the opener leaves `!--`
# standing on its own line and everything after it is read as prose.
# Nothing inside a comment was ever meant to be read, so none of it
# reaches the tokenizer.
def mail_uncomment(html)
  said = html
  out = ""
  while true
    at = mail_at(said, "<!--")
    return out + said if at < 0

    out = out + said.substring(0, at)
    rest = said.substring(at + 4, mail_len(said))
    stop = mail_at(rest, "-->")
    # An unterminated comment runs to the end of the mail, which is what
    # a browser does with one too.
    return out if stop < 0

    said = rest.substring(stop + 3, mail_len(rest))
  end
  out
end

def mail_tokens(html)
  out = []
  parts = mail_uncomment(html).split("<")
  lead = parts[0] ?? ""
  out.push({"t": "text", "v": lead}) if lead != ""
  i = 1
  while i < parts.length()
    part = parts[i] ?? ""
    at = mail_at(part, ">")
    if at < 0
      out.push({"t": "text", "v": part})
    else
      out.push({"t": "tag", "v": part.substring(0, at)})
      rest = part.substring(at + 1, mail_len(part))
      out.push({"t": "text", "v": rest}) if rest != ""
    end
    i = i + 1
  end
  out
end

# The tag's name, lower case, with any leading slash kept as `/`.
def mail_tag_name(tag)
  said = tag.trim().downcase()
  return "" if said == ""

  cut = mail_at(said, " ")
  name = cut < 0 ? said : said.substring(0, cut)
  name.replace("/", "/").replace("\t", "").replace("\n", "")
end

# One attribute, quoted or bare. Attribute names are ASCII, so searching
# the lowercased tag and cutting the original at the same index is safe.
def mail_attr(tag, name)
  low = tag.downcase()
  at = mail_at(low, name + "=")
  return "" if at < 0

  rest = tag.substring(at + mail_len(name) + 1, mail_len(tag))
  return "" if rest == ""

  q = rest.substring(0, 1)
  if q == "\"" || q == "'"
    body = rest.substring(1, mail_len(rest))
    stop = mail_at(body, q)
    return stop < 0 ? body : body.substring(0, stop)
  end
  stop = mail_at(rest, " ")
  stop < 0 ? rest : rest.substring(0, stop)
end

# ---------------------------------------------------------------- blocks
#
# A flat list, because that is what a mail is once the table scaffolding
# is gone: a run of paragraphs, links and pictures in reading order.
# Nesting is thrown away deliberately -- a three-deep table wrapping one
# sentence is not structure, it is 2003.

MAIL_BREAKERS = ["/p", "/div", "/tr", "/li", "/h1", "/h2", "/h3", "/h4", "/table", "br", "hr"]

# The tags whose *kind* matters, not only their break.
#
# A block used to be text, a link, a picture or a run of mono, and a
# heading was a paragraph in disguise. That was tolerable while the only
# source was a stranger's HTML; it stopped being so when the reader began
# rendering markdown through this parser (`mail_blocks_of`), because a
# document written with headings and lists arrived as a wall of
# paragraphs -- the structure was in the source and thrown away here.
MAIL_HEADS = ["h1", "h2", "h3", "h4"]
MAIL_SKIPPED = ["style", "script", "head", "title"]

def mail_html_blocks(html)
  out = []
  said = ""
  href = ""
  skip = ""
  # What the open block is, if it is not a paragraph: a heading level, a
  # list item or a quote. It is set by the opening tag and spent by the
  # flush that closes the block.
  mark = ""
  for tok in mail_tokens(html)
    if tok["t"] == "text"
      if skip == ""
        said = said + tok["v"]
      end
    else
      name = mail_tag_name(tok["v"])
      if skip != ""
        skip = "" if name == "/" + skip
      elsif MAIL_SKIPPED.contains(name)
        skip = name
      elsif name == "a"
        pair = mail_flush(out, said, "", mark)
        out = pair
        said = ""
        href = mail_attr(tok["v"], "href")
      elsif name == "/a"
        out = mail_flush(out, said, href, mark)
        said = ""
        href = ""
      elsif name == "img"
        out = mail_flush(out, said, href, mark)
        said = ""
        out.push(mail_img_block(tok["v"]))
      elsif MAIL_HEADS.contains(name)
        out = mail_flush(out, said, href, mark)
        said = ""
        mark = name
      elsif name == "li"
        out = mail_flush(out, said, href, mark)
        said = ""
        mark = "li"
      elsif name == "blockquote"
        out = mail_flush(out, said, href, mark)
        said = ""
        mark = "quote"
      elsif name == "/blockquote"
        out = mail_flush(out, said, href, mark)
        said = ""
        mark = ""
      elsif MAIL_BREAKERS.contains(name)
        out = mail_flush(out, said, href, mark)
        said = ""
        # A heading and an item end with their block; a quote holds until
        # its own closing tag, because everything inside it is quoted.
        mark = "" if mark != "quote"
      end
    end
  end
  mail_flush(out, said, href, mark)
end

# Text becomes a block only if it says something. A mail is mostly
# whitespace between tags, and a paragraph of it is a blank line on the
# page for no reason.
# Appends in place and hands the same list back.
#
# It used to `concat`, which copies: one copy per block, so a letter of
# eight hundred blocks copied three hundred thousand entries on its way
# through here. The callers all say `out = mail_flush(out, ...)`, which
# reads the same either way.
def mail_flush(out, said, href, mark = "")
  clean = mail_unwrap(said)
  return out if clean == ""

  if mail_openable?(href)
    out.push({"kind": "link", "text": clean, "href": href, "mark": mark})
    return out
  end
  # A link the client will not open -- mailto:, a tracking redirect on
  # http: -- is still words someone wrote. Keep the words, drop the link.
  out.push({"kind": "text", "text": clean, "mark": mark})
  out
end

# Entities, and the whitespace collapse a browser would do. Without the
# collapse every newline in the source becomes a space in the paragraph
# and the measure goes ragged.
def mail_unwrap(said)
  out = mail_entities(said)
  out = out.replace("\r", " ").replace("\n", " ").replace("\t", " ")
  # Until there are no double spaces, rather than until the string stops
  # changing: the same answer, without comparing the whole string once per
  # pass to find out.
  while out.contains("  ")
    out = out.replace("  ", " ")
  end
  out.trim()
end

# 03 §3.5: exactly one scheme, lower case, literally -- and a platform
# opener is a URI dispatcher, so anything else is refused here rather than
# handed over and hoped about.
def mail_openable?(href)
  return false if href == ""
  return false if !href.starts_with("https://")
  return false if href.contains(" ") || href.contains("\n") || href.contains("\t")
  return false if href.contains("@")

  true
end

def mail_img_block(tag)
  {
    "kind": "image",
    "src": mail_attr(tag, "src"),
    "alt": mail_unwrap(mail_attr(tag, "alt")),
    "w": mail_num(mail_attr(tag, "width")),
    "h": mail_num(mail_attr(tag, "height"))
  }
end

def mail_num(said)
  digits = said.replace("px", "").replace("%", "").trim()
  return 0 if digits == ""

  int(digits) rescue 0
end

# ------------------------------------------------------------ plain text
#
# A text part has no tags, but it does have addresses -- often on their
# own line, because the sender's own flattener put them there. Those are
# links as much as an `<a href>` is, so they are found and made openable.
#
# It also has entities. `&#8364;` appears in a *text* part, which should
# not happen and does, because the part was generated from the HTML one
# by something that unescaped nothing. Running it through `html_unescape`
# costs nothing on a mail that has none.

# The tallest a picture is drawn, before it is scaled down to fit.
MAIL_IMG_TALL = 520

MAIL_URL_STOPS = [" ", "\n", "\t", "\r", "\"", "<", ">", "]"]
MAIL_URL_TAIL = [".", ",", ")", ":", ";", "!", "?"]

# Runs of blank lines, collapsed to one.
#
# A plain-text part often arrives with six or ten newlines where a mail
# merge put a section it did not fill, and every one of them is a line of
# nothing on screen -- a letter you have to scroll past its own gaps to
# read. One blank line between paragraphs is what a blank line means; the
# rest is padding from a template.
#
# Done at display rather than on the way in, so it applies to the mail
# already stored as well as to what arrives next.
def mail_tighten(said)
  out = said.replace("\r", "")
  while out.contains("\n\n\n")
    out = out.replace("\n\n\n", "\n\n")
  end
  out.trim()
end

def mail_text_blocks(body)
  said = mail_tighten(mail_entities(body))
  out = []
  held = ""
  n = 0
  for line in said.split("\n")
    if line.contains("https://")
      out = mail_hold(out, held)
      held = ""
      out = out.concat(mail_autolink(line))
    else
      room = held.bytesize() + line.bytesize() + 1
      if n == 0 || room > MAIL_CHUNK
        out = mail_hold(out, held)
        held = line
      else
        held = held + "\n" + line
      end
    end
    n = n + 1
  end
  mail_hold(out, held)
end

def mail_hold(out, held)
  return out if held.trim() == ""

  out.concat([{"kind": "mono", "text": held}])
end

# One line, cut into the runs before, at and after each address. They
# cannot share a line: a text node is one string with one style (02 §3),
# so a link inside a sentence is a node of its own and lands on its own
# line. That is the protocol, not a shortcut -- and for mail it reads
# well, because a sender's flattener had already put them on their own
# lines.
def mail_autolink(line)
  parts = line.split("https://")
  out = []
  head = parts[0] ?? ""
  out = out.concat([{"kind": "mono", "text": head}]) if head.trim() != ""
  i = 1
  while i < parts.length()
    part = parts[i] ?? ""
    tail = mail_url_tail(part)
    url = "https://" + tail
    out = out.concat([{"kind": "link", "text": url, "href": url}]) if mail_openable?(url)
    out = out.concat([{"kind": "mono", "text": url}]) if !mail_openable?(url)
    rest = part.substring(mail_len(tail), mail_len(part))
    out = out.concat([{"kind": "mono", "text": rest}]) if rest.trim() != ""
    i = i + 1
  end
  out
end

# How much of what follows `https://` is still the address: everything up
# to the first character that cannot be in one, less a trailing mark that
# belongs to the sentence. "Δες την αγγελία: https://x/y." ends at `y`.
def mail_url_tail(part)
  head = part
  for sep in MAIL_URL_STOPS
    bits = part.split(sep)
    first = bits[0] ?? ""
    head = first if first.length() < head.length()
  end
  n = mail_len(head)
  return head if n < 1

  last = head.substring(n - 1, n)
  return head.substring(0, n - 1) if MAIL_URL_TAIL.contains(last)

  head
end

# ---------------------------------------------------------- the nodes

# A link. The prop is the whole mechanism (03 §3.5): the client opens the
# address when the person activates the node, and only then -- there is no
# op that opens one, so a tree that merely arrives opens nothing, and this
# application is never told whether anything happened. A node carrying
# `open` is a focus stop, so Tab reaches every link in a letter.
def mail_link_node(b, m)
  said = b["text"]
  said = b["href"] if said.trim() == ""
  {
    "k": "box",
    "s": {
      "display": "row", "width": m, "pad": [1, 0, 1, 0],
      "cursor": "pointer", "radius": 1
    },
    "p": {"open": b["href"], "role": "link", "label": mail_cap(said, 200)},
    "c": [text(mail_cap(said, 300), {
      "size": 2, "fg": "accent.base", "underline": true, "width": m
    })]
  }
end

# A picture, and the reason it is not loaded until you ask.
#
# Every one of these is a request to a server the sender chose, made from
# your address, the moment you open the mail. A good number are one pixel
# wide and exist only to report that you read it -- the sample that
# started this carried `transparent.png` five times. So the bytes are not
# fetched on open: the block draws what it knows, the alt text and the
# host, and `i` fetches them for the message you are reading and no other.
def mail_image_node(b, m, shots, waiting = [])
  src = b["src"] ?? ""
  alt = b["alt"] ?? ""
  got = shots[src]
  # A picture that turned out to be a pixel or two is a receipt, not a
  # picture -- and `width="100%"` hid that until the bytes arrived.
  return mail_picture(b, m, got) if !got.nil? && (got["w"] ?? 0) > 2 && (got["h"] ?? 0) > 2

  # Bytes that came down and could not be measured are not a tracking
  # pixel, and they used to share its fate: the same branch dropped both,
  # so a picture in a format this application cannot read -- an SVG, a
  # WebP the decoder does not know -- vanished from the letter without a
  # word. A picture that is there and cannot be drawn says so.
  if !got.nil?
    return text("", {"size": 0}) if (got["w"] ?? 0) > 0

    return mail_img_chip(m, alt, src, "format illisible", false)
  end

  # In the queue: the bytes are on their way, and a spinner is the only
  # honest thing to put where the picture will be.
  return mail_img_chip(m, alt, src, "", true) if waiting.filter(fn(q) { q == src }).length() > 0

  says = alt == "" ? mail_host(src) : alt + "  ·  " + mail_host(src)
  mail_img_chip(m, alt, src, "", false)
end

# Where a picture is not, and why: its host, and either a spinner while
# the bytes are coming or a word about what stopped them.
def mail_img_chip(m, alt, src, said, spinning)
  says = alt == "" ? mail_host(src) : alt + "  ·  " + mail_host(src)
  says = says + "  ·  " + said if said != ""
  mark = spinning == true ? mail_spinner(12) : text("IMG", {
    "font": "mono", "size": 0, "weight": "bold",
    "fg": "text.muted", "shrink": 0
  })
  {
    "k": "box",
    "s": {
      "display": "row", "align": "center", "gap": 3, "width": m,
      "pad": [2, 3, 2, 3], "radius": 2, "bg": "surface.sunken"
    },
    "p": {"role": "image", "label": alt == "" ? "Image" : alt},
    "c": [
      mark,
      text(mail_cap(says, 200), {"size": 1, "fg": "text.muted", "grow": 1, "shrink": 1})
    ]
  }
end

# The picture itself, at the size the HTML declared, scaled down to the
# measure if it is wider.
#
# The size has to come from somewhere: a node with no width is measured
# against a loosened constraint, and this process cannot decode a JPEG to
# ask. Mail almost always says -- `width` and `height` on the `img` are
# how a mail keeps its layout in clients that have no CSS -- and where it
# does not, a modest box is used and the picture sits inside it.
def mail_picture(b, m, shot)
  w = shot["w"] ?? 0
  h = shot["h"] ?? 0
  # A picture whose size could not be read gets a modest box rather than
  # an unbounded one: a node with a width and no height is laid out
  # against a loosened constraint and stretches.
  if w < 1 || h < 1
    w = 320
    h = 200
  end
  # Never wider than the measure, and never taller than a screenful --
  # both scaled, so the shape the photograph actually has is kept.
  if w > m
    h = h * m / w
    w = m
  end
  if h > MAIL_IMG_TALL
    w = w * MAIL_IMG_TALL / h
    h = MAIL_IMG_TALL
  end
  {
    "k": "image",
    "s": {"width": w, "height": h, "radius": 2},
    "p": {"src": shot["path"] ?? "", "role": "image", "label": b["alt"] ?? "Image"}
  }
end

# The host on its own, which is what tells you whether a picture is worth
# fetching: a logo on the sender's own domain, or a pixel on a tracker's.
def mail_host(src)
  said = src.replace("https://", "").replace("http://", "")
  cut = mail_at(said, "/")
  cut < 0 ? said : said.substring(0, cut)
end

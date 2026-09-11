# What a link turns out to be, fetched once.
#
# An unfurl is a request to someone else's server, so it happens **when a
# message is written** and never when one is drawn. A fil that resolved links
# at render time would fetch a page per card on every scroll — thousands of
# requests, against sites that never asked to be read by a chat window — and
# it would put a network round trip on the path of a frame.
#
# The answer is kept in `chat_links`, keyed by the URL: the same link posted
# in two rooms is one fetch, and it survives a restart.
#
# The picture is **downloaded**, not linked. A card that pointed at the
# remote image would have every window that ever shows that message fetch it
# from the site being linked — which tells that site who is reading what, and
# is exactly the thing EUI serves assets by content hash to avoid (01 §2.2).

CHAT_LINK_TIMEOUT = 6

# How wide a card draws its picture, in device pixels at 2×. Bigger than the
# card needs is wasted bytes on the wire for ever; smaller is a blurred card.
CHAT_LINK_IMAGE_PX = 600

# Early returns, not a tail ternary: a ternary in the last position whose
# condition reads a local assigned just above it does not resolve here, and
# fails at the `end` with "Undefined variable" naming that local — which
# sends you looking at the assignment, where nothing is wrong.
def chat_link_host(url)
  link_without = url.replace("https://", "").replace("http://", "")
  link_cut = link_without.index_of("/")
  return link_without if link_cut.nil? || link_cut < 0

  link_without.substring(0, link_cut)
end

# One meta tag's content, by property or name. Written by hand rather than
# with a regular expression because the shape varies — the attributes come in
# either order, the quotes are either kind — and a parser that is wrong is
# worse than one that is simple.
def chat_meta_of(html, key)
  link_at = html.index_of(key)
  return "" if link_at.nil? || link_at < 0

  link_rest = html.substring(link_at, html.length())
  link_mark = link_rest.index_of("content=")
  return "" if link_mark.nil? || link_mark < 0
  return "" if link_mark > 200

  link_tail = link_rest.substring(link_mark + 8, link_rest.length())
  link_quote = link_tail.substring(0, 1)
  return "" unless link_quote == "\"" || link_quote == "'"

  link_body = link_tail.substring(1, link_tail.length())
  link_end = link_body.index_of(link_quote)
  return "" if link_end.nil? || link_end < 0

  chat_unescape(link_body.substring(0, link_end))
end

def chat_unescape(said)
  said.replace("&amp;", "&").replace("&quot;", "\"").replace("&#39;", "'")
      .replace("&lt;", "<").replace("&gt;", ">").trim()
end

# The `<title>`, for a page that offers no `og:title`.
def chat_title_of(html)
  link_at = html.index_of("<title")
  return "" if link_at.nil? || link_at < 0

  link_rest = html.substring(link_at, html.length())
  link_open = link_rest.index_of(">")
  return "" if link_open.nil? || link_open < 0

  link_body = link_rest.substring(link_open + 1, link_rest.length())
  link_end = link_body.index_of("</title")
  return "" if link_end.nil? || link_end < 0

  chat_unescape(link_body.substring(0, link_end))
end

# A URL that may be relative, made absolute against the page it came from.
def chat_absolute(url, src)
  return "" if src.blank?
  return src if src.starts_with?("http://") || src.starts_with?("https://")
  return "https:" + src if src.starts_with?("//")

  link_root = "https://" + chat_link_host(url)
  return link_root + src if src.starts_with?("/")

  link_root + "/" + src
end

# The picture, fetched and cut down to what a card draws. Named by a digest
# of its source so the same image is downloaded once, and served from the
# application like any other asset.
def chat_link_image(src)
  return "" if src.blank?

  link_name = "public/chat/og-" + Crypto.sha256(src).substring(0, 16) + ".png"
  return link_name if File.exists(link_name)

  mkdir_p("public/chat")
  link_raw = link_name + ".src"
  link_got = HTTP.download(src, link_raw) rescue 0
  return "" if link_got == 0

  chat_fit_image(link_raw, link_name) rescue nil
  File.delete(link_raw) rescue nil
  return "" unless File.exists(link_name)

  link_name
end

def chat_fit_image(source, target)
  link_pic = Image.new(source)
  link_w = link_pic.width()
  link_h = link_pic.height()
  return link_pic.format("png").to_file(target) if link_w <= CHAT_LINK_IMAGE_PX

  link_scale = CHAT_LINK_IMAGE_PX * 1.0 / link_w
  link_pic.resize(CHAT_LINK_IMAGE_PX, int(link_h * link_scale)).format("png").to_file(target)
end

# Resolve a link, or say why not. Always answers a hash the card can draw:
# a page that refuses to be read still gets its host and its path, which is
# what the card showed before any of this existed.
def chat_link_fetch(url)
  link_fallback = {
    "url": url,
    "host": chat_link_host(url),
    "title": "",
    "description": "",
    "image": ""
  }
  link_opts = {
    "timeout": CHAT_LINK_TIMEOUT,
    "headers": { "User-Agent": "Atrium/1.0 (+EUI link preview)" }
  }
  link_res = HTTP.request("GET", url, link_opts) rescue nil
  return link_fallback if link_res.nil? || link_res["status"] != 200

  link_html = link_res["body"].to_s
  return link_fallback if link_html.blank?

  link_title = chat_meta_of(link_html, "og:title")
  link_title = chat_meta_of(link_html, "twitter:title") if link_title.blank?
  link_title = chat_title_of(link_html) if link_title.blank?

  link_desc = chat_meta_of(link_html, "og:description")
  link_desc = chat_meta_of(link_html, "twitter:description") if link_desc.blank?
  link_desc = chat_meta_of(link_html, "\"description\"") if link_desc.blank?

  link_src = chat_meta_of(link_html, "og:image")
  link_src = chat_meta_of(link_html, "twitter:image") if link_src.blank?

  {
    "url": url,
    "host": chat_link_host(url),
    "title": link_title,
    "description": link_desc,
    "image": chat_link_image(chat_absolute(url, link_src))
  }
end

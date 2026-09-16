# Google Fonts, fetched by the server.
#
# The client never talks to a font service. It shapes with the faces it
# carries and with the assets its own origin served, and that is a privacy
# claim EUI makes in the open (spec 08 §8): no font enumeration, no
# third-party connection, a fingerprint surface of one viewport. A face from
# Google would break all three if the window went and got it.
#
# So the server goes and gets it, once. `HTTP.request` asks the CSS endpoint
# what the faces are, `HTTP.download` writes each one under `public/fonts/`,
# and `eui_font` hands the paths to the asset store, which names them by the
# BLAKE3 of their bytes. From there a face is an asset like a picture: the
# window fetches `/_eui/asset/<hash>` from its own origin and checks the
# bytes against their own name before the shaper sees them. Google learns the
# server's address, on the first boot after a deploy, and nothing else.
#
# The User-Agent decides the format, and only one of the four is any use
# here. Measured against the endpoint on 2026-09-16:
#
#     a modern browser UA  -> format('woff2')     brotli tables
#     an IE 11 UA          -> format('woff')      zlib tables
#     "Mozilla/5.0", bare  -> format('truetype')  <- this one
#     no User-Agent at all -> format('truetype')
#
# The client carries neither brotli nor zlib for fonts: it reads sfnt and
# says so (`looks_like_font`, `eui-client/src/assets.rs`). So the bare UA is
# not a trick to look like an old browser, it is the way to ask for the
# format the shaper reads — and `google_font_url_in` refuses anything that
# is not a `.ttf`, so a change at the other end falls back to sans rather
# than downloading a face nothing can open.
GOOGLE_FONTS_CSS = "https://fonts.googleapis.com/css2"
GOOGLE_FONTS_UA = "Mozilla/5.0"
GOOGLE_FONTS_DIR = "public/fonts"
GOOGLE_FONTS_TIMEOUT = 8

# Declare a Google family and answer the name a style uses it by.
#
#     google_font("Playfair Display", [400, 700])
#     text("A heading", {"font": "Playfair Display", "weight": "bold"})
#
# One face per weight, downloaded once and kept: a file already on disk is
# not fetched again, so only the first boot after a deploy costs a round
# trip. Answers the family name on success and "sans" when the download did
# not happen — an application whose typography depends on a network that is
# down should still draw, and `{"font": "sans"}` is what it drew before.
def google_font(family, weights)
  google_font_declare(family, weights, true)
end

# The same family, without the network: the faces already on disk, declared,
# and `"sans"` when there are none.
#
# This is what a **view** calls. `google_font` may block on four requests to
# a host that is not answering, and a render is the one place that must not:
# the download belongs at boot, in `config/routes.sl`, where a slow font
# service delays a start rather than a frame.
def google_font_here(family, weights)
  google_font_declare(family, weights, false)
end

def google_font_declare(family, weights, fetch)
  font_slug = google_font_slug(family)
  font_paths = []

  for font_weight in weights
    font_file = GOOGLE_FONTS_DIR + "/" + font_slug + "-" + font_weight.to_s + ".ttf"
    if File.exists(font_file)
      font_paths = font_paths.concat([font_file])
      next
    end
    next unless fetch

    font_url = google_font_face_url(family, font_weight)
    next if font_url.blank?

    mkdir_p(GOOGLE_FONTS_DIR) rescue nil
    font_got = HTTP.download(font_url, font_file) rescue 0
    next if font_got == 0

    font_paths = font_paths.concat([font_file])
  end

  return "sans" if font_paths.length() == 0

  # A face on disk the asset store will not take — a path outside
  # `public/`, a file that vanished between the check and the read — must
  # not stop the application booting over a typeface. It draws in sans, the
  # way it did before anybody asked for Playfair.
  font_name = eui_font(family, font_paths) rescue "sans"
  font_name
end

# The URL of one face, from the CSS the family's stylesheet is made of.
#
# The endpoint answers a short stylesheet whose `src: url(...)` is the face
# itself. Asking for one weight at a time keeps the parse to finding the
# first `url(` — there is no ambiguity about which face came back.
def google_font_face_url(family, weight)
  font_query = GOOGLE_FONTS_CSS + "?family=" + google_font_query(family) +
    ":wght@" + weight.to_s
  font_opts = {
    "timeout": GOOGLE_FONTS_TIMEOUT,
    "headers": { "User-Agent": GOOGLE_FONTS_UA }
  }
  font_res = HTTP.request("GET", font_query, font_opts) rescue nil
  return "" if font_res.nil? || font_res["status"] != 200

  google_font_url_in(font_res["body"].to_s)
end

# The first `url(...)` of a stylesheet, unquoted, and only if it is a
# TrueType file.
#
# Blank when there is none — a family the endpoint does not know answers 400
# and never reaches here, but a body that changed shape should fall back
# rather than guess. Blank too for a `.woff` or `.woff2`, which is the whole
# point of the User-Agent above: downloading one would cost a round trip and
# a file the shaper refuses, and the role would draw in sans anyway.
def google_font_url_in(css)
  font_at = css.index_of("url(")
  return "" if font_at.nil? || font_at < 0

  font_rest = css.substring(font_at + 4, css.length())
  font_end = font_rest.index_of(")")
  return "" if font_end.nil? || font_end < 0

  font_url = font_rest.substring(0, font_end).strip()
  font_url = font_url.substring(1, font_url.length() - 1) if font_url.starts_with?("\"")
  font_url = font_url.substring(1, font_url.length() - 1) if font_url.starts_with?("'")
  return "" unless font_url.starts_with?("https://")
  return "" unless font_url.ends_with?(".ttf")

  font_url
end

# "Playfair Display" -> "Playfair+Display", what the endpoint takes.
def google_font_query(family)
  family.strip().replace(" ", "+")
end

# "Playfair Display" -> "playfair-display", a file name.
def google_font_slug(family)
  family.strip().downcase().replace(" ", "-")
end

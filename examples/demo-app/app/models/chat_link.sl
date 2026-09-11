# One link, as its page describes itself.
#
# `title`, `description` and `image` are Open Graph where the page offers it
# and the best fallback where it does not; `image` is a path under `public`,
# because a picture the window fetches from its own origin by hash is one
# the site being linked never hears about (01 §2.2).
class ChatLink < Model
  validates("url", { "presence": true })
end

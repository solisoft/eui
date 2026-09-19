# What the proxy asks before it sends anybody here.
#
# `app.infos` names `/health`, and a slot that does not answer it is never
# promoted — which is the whole point: a deploy that would have served a
# broken application fails instead, on the slot nobody is using yet.
class HomeController < Controller
  # `respond_to` picks by what the caller asked for: `Accept: text/html`
  # gets the page, `application/vnd.eui.frames` gets the render, and
  # `/hello.eui` names the second without varying on a header — which is
  # what a CDN wants, since none of them key on Accept.
  def hello
    respond_to(req, fn(format) {
      format.html(fn() {
        {
          "status": 200,
          "headers": {"Content-Type": "text/html; charset=utf-8"},
          "body": "<!doctype html><title>Hello</title><h1>Hello from HTML</h1>"
        }
      })
      format.eui(fn() {
        eui_render({
          "k": "box",
          "s": {"display": "column", "gap": 12, "pad": 24, "bg": "surface.base"},
          "c": [
            {"k": "text", "t": "Hello from a route", "s": {"size": 24, "fg": "text.default"}},
            {"k": "text", "t": "No session was opened to draw this.", "s": {"fg": "text.muted"}}
          ]
        })
      })
    })
  end

  def health
    {
      "status": 200,
      "headers": {"Content-Type": "application/json"},
      "body": "{\"status\":\"ok\"}"
    }
  end
end

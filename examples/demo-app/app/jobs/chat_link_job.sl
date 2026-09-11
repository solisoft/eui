# Resolve a link, away from anybody waiting.
#
# This is a job and not a step of `chat_say` for a reason that is easy to
# miss until it bites: an unfurl is a request to a server nobody here
# controls, and a server that has gone away does not refuse — it hangs. On
# the send path that would hold the handler for the whole HTTP timeout, and
# because a room wants one realtime worker (a module global belongs to one
# thread), holding that handler holds **every** session on the server. One
# unreachable link would freeze the application for everyone.
#
# So the message is written and drawn immediately with the plain card — host
# and path — and the picture, the title and the description arrive when they
# arrive. The counters move when they land and every window is told, so the
# card fills in under the message. Nobody waited.
class ChatLinkJob
  static def perform(args)
    job_url = str(args["url"] ?? "")
    return if job_url.blank?
    return unless ChatLink.find_by("url", job_url).nil?

    job_got = chat_link_fetch(job_url)
    ChatLink.upsert("url", {
      "url": job_url,
      "host": job_got["host"],
      "title": job_got["title"],
      "description": job_got["description"],
      "image": job_got["image"]
    })

    # The card just got taller, so both counters move: `gen` is what the
    # height table and the row cache are keyed on, `value` is what a tick
    # compares. Moving only the second would redraw a row at a height
    # measured before the picture existed.
    job_meta = ChatMeta.find_by("key", "seq")
    unless job_meta.nil?
      job_meta.gen = (job_meta.gen ?? 0) + 1
      job_meta.value = (job_meta.value ?? 0) + 1
      job_meta.save()
    end

    # And then say so. This runs on a job worker with nobody waiting on it,
    # so there is no session to leave out: every open window renders, and the
    # card fills in under the message the moment the fetch came back rather
    # than at whatever the next tick was going to be.
    eui_wake("chat")
  end
end

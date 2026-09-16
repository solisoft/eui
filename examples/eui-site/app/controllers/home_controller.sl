# What the proxy asks before it sends anybody here.
#
# `app.infos` names `/health`, and a slot that does not answer it is never
# promoted — which is the whole point: a deploy that would have served a
# broken application fails instead, on the slot nobody is using yet.
class HomeController < Controller
  def health
    {
      "status": 200,
      "headers": {"Content-Type": "application/json"},
      "body": "{\"status\":\"ok\"}"
    }
  end
end

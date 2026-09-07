# The two ordinary HTTP routes Needle needs, and the only place a person's
# Spotify account is involved: the authorization code flow, so the player
# can ask Spotify Connect to start a track on a device they already have
# open. The catalogue itself needs none of this — it uses client
# credentials, which has no browser step and no callback at all.
#
# Why it ends in a page telling you to paste a line rather than saving it:
# `Cache` is backed by SoliKV and the desktop build starts no database, and
# `File` writes are jailed to the application folder, which for a desktop
# build is a directory under /dev/shm that dies with the process. There is
# nowhere durable for this process to put a secret, so the refresh token
# goes where the other two already live — the environment.
#
# One limit worth knowing before you look for the bug: these two routes
# answer under `soli serve`, and not in a packaged desktop build. That
# build arms the launcher gate with an embedded-client session (Soli's
# `desktop::token::arm_session`), which mints no launch token at all, so
# every browser request is refused with "This application must be opened
# from its own launcher" — including the one Spotify sends back. Link the
# account once from the repo server, put the refresh token in the
# launcher, and the packaged app never needs a route: it only spends the
# token.

class SpotifyController < Controller
  # GET /spotify/login — send the browser to Spotify's consent screen.
  def login
    if !needle_configured()
      return needle_reply(
        "Not configured",
        "Set SPOTIFY_CLIENT_ID and SPOTIFY_CLIENT_SECRET first — the player needs them for the catalogue too."
      )
    end

    {
      "status": 302,
      "headers": {"Location": needle_authorize_url()},
      "body": ""
    }
  end

  # GET /spotify/callback — Spotify sends the browser back here.
  def callback
    return needle_reply("Not configured", "This app has no Spotify client to exchange a code with.") unless needle_configured()

    refused = params["error"].to_s
    return needle_reply("Spotify said no", "It answered “" + refused + "”. Nothing was saved.") if refused != ""

    if params["state"].to_s != needle_login_state()
      return needle_reply(
        "That did not start here",
        "The state parameter did not match, so the code was ignored. Start again from the player."
      )
    end

    code = params["code"].to_s
    return needle_reply("No code came back", "Spotify redirected without one. Start again from the player.") if code == ""

    tokens = needle_exchange(code)
    blame = "Spotify would not trade that code for a token. Check that the redirect URI on the dashboard reads exactly "
    + needle_redirect()
    return needle_reply("The exchange failed", blame) if tokens.nil?

    refresh = tokens["refresh_token"] ?? ""
    if refresh == ""
      return needle_reply(
        "No refresh token",
        "Spotify returned an access token but no refresh token, so there is nothing worth keeping. Start again from the player."
      )
    end

    needle_reply_token(refresh)
  end
end

# Deploying the site

One environment: a push to `main` that touches `www/` deploys
**eui.solisoft.net** to the shared server, into
`/home/rocky/sites/eui.solisoft.net/`, under `soli-proxy` (already installed
there — this is the same arrangement as the other Soli sites on that box).
The workflow is [`.github/workflows/deploy-site.yml`](../.github/workflows/deploy-site.yml);
it verifies first, deploys second, and **touches only that folder**.

Nothing else in this repository is deployed. The client, the crates and the
specification are not a website; `www/` is.

## The contract

1. `rsync www/` into `/home/rocky/sites/eui.solisoft.net/`, excluding what
   lives on the server (`.env`, `app.infos`, `restart.txt`) and what has no
   business there (`.git/`, `.claude/`, `.env.*`);
2. `soli-proxy deploy -c /home/rocky/sites/config.toml eui`: the proxy starts
   the idle slot, waits for `/up`, moves the traffic across, stops the old
   slot;
3. `soli-proxy restart …`, because after a switch the proxy balances across
   both slot ports while only one is up — every other request would get a 421.

There is **no migration step**: the site has no database. It serves its views
and the Markdown under `www/docs/` from disk.

The CI then checks that a **new** process answers 200 on `/up`, not merely
that the site answers: an old slot still standing would answer 200 without
carrying the code that was just pushed. It finishes by asking for
`/images/samples/tracker.png` through the proxy — the samples page is the one
that can rot silently, since a picture that stopped being deployed still
leaves the page rendering.

> **Why the command and not `touch restart.txt`.** The trigger file is ignored
> in silence when the proxy cannot start the slot — for instance when an
> orphaned process still holds its port. A deployment then goes green while
> the old code keeps serving. `soli-proxy deploy` returns a status and a
> reason.
>
> If that happens (`Port … is already in use`), stop this site's processes —
> and only this site's — then deploy again:
>
> ```sh
> for x in $(pgrep -x soli); do
>   [ "$(readlink /proc/$x/cwd)" = "/home/rocky/sites/eui.solisoft.net" ] && kill $x
> done
> soli-proxy deploy -c /home/rocky/sites/config.toml eui
> ```

## One-time setup, by hand

None of this touches the other sites on the server.

1. Create the site's folder. Its name **is** the domain, and it must contain a
   dot:

   ```sh
   mkdir -p /home/rocky/sites/eui.solisoft.net
   ```

2. Put `app.infos` and `.env` in it, from the examples beside this file, mode
   `0600` for the `.env`:

   ```sh
   install -m 0644 deploy/app.infos.example /home/rocky/sites/eui.solisoft.net/app.infos
   install -m 0600 deploy/env.production.example /home/rocky/sites/eui.solisoft.net/.env
   ```

   `SOLI_APP_HOSTS` and `SOLI_SESSION_SECRET` (32 characters or more) are
   required: under `APP_ENV=production`, Soli refuses to start without them.

3. Let the proxy know about the site. If your `soli-proxy` discovers new
   folders by itself, there is nothing to do; otherwise
   `systemctl restart soli-proxy` is needed, and it briefly restarts **every**
   site — do it in a quiet moment.

4. Point the domain's DNS at the server; the proxy obtains the Let's Encrypt
   certificate itself.

5. First deployment: push to `main`, or run the workflow by hand
   (*Actions → verify & deploy the site → Run workflow*).

`rocky` needs `soli` on its `PATH` (the version check and the probes use it)
and write access to the site's folder.

## Secrets and variables

Repository → Settings → Secrets and variables → Actions.

| Name | Type | Contents |
|---|---|---|
| `DEPLOY_HOST` | secret | the server's address |
| `SSH_DEPLOY_KEY` | secret | a private key dedicated to this CI, no passphrase; its public half goes in `~rocky/.ssh/authorized_keys` |
| `SSH_KNOWN_HOSTS` | secret | `ssh-keyscan <host>` — host verification stays on |
| `DEPLOY_DOMAIN` | variable | `eui.solisoft.net` (the default if absent) |
| `DEPLOY_USER` | variable | `rocky` (the default if absent) |
| `DEPLOY_SITES_DIR` | variable | `/home/rocky/sites` (the default if absent) |
| `DEPLOY_APP` | variable | the `name` in `app.infos`, `eui` by default |

Creating the key:

```sh
ssh-keygen -t ed25519 -N '' -C 'ci-eui' -f ci-eui
cat ci-eui.pub >> ~rocky/.ssh/authorized_keys        # on the server
gh secret set SSH_DEPLOY_KEY < ci-eui
gh secret set SSH_KNOWN_HOSTS --body "$(ssh-keyscan <host> 2>/dev/null)"
gh secret set DEPLOY_HOST --body <host>
gh variable set DEPLOY_DOMAIN --body eui.solisoft.net
shred -u ci-eui ci-eui.pub
```

## The pinned version

`SOLI_VERSION` in the workflow is the version the CI installs *and* the
version the server is checked against before the switch. A CI testing a
different version than production tests nothing, so the two are compared
explicitly rather than assumed; when the server is upgraded, bump the pin in
the same commit.

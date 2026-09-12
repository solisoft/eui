# Deploying

Two sites, one server, one arrangement. A push to `main` deploys whichever
of them it touched, into its own folder under `/home/rocky/sites/`, under
`soli-proxy` (already installed there — the same arrangement as the other
Soli sites on that box). Each workflow verifies first, deploys second, and
**touches only its own folder**.

| What | From | To | Workflow |
|---|---|---|---|
| The documentation site | `www/` | `eui.solisoft.net` | [`deploy-site.yml`](../.github/workflows/deploy-site.yml) |
| The demo application | `examples/demo-app/` | `eui-data.solisoft.net` | [`deploy-data.yml`](../.github/workflows/deploy-data.yml) |

Nothing else in this repository is deployed. The client, the crates and the
specification are not a website.

## The contract, for the documentation site

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

## The demo application

Same three steps, four differences — and each of them is a way this site
could be broken by a deployment that looked fine.

**It has a database.** `soli db:migrate up` runs on the server after the
files land and *before* the slot is switched, so a migration that fails
fails with the old code still serving. Seeding is not in the workflow: it
is a one-time step below, because re-running it on every deploy would
rewrite rooms people have been talking in.

**It writes into its own tree.** Attachments, the thumbnails made from
them, fetched album art. None of it is committed — it is all in
`.gitignore` — so `--delete-delay` would take it off the server on the
first deploy. Those directories are excluded, and rsync does not delete
what it excludes.

**It carries an EUI publisher key**, `config/eui_publisher.pkcs8`,
generated on first boot. A client pins the public half on first connection
and refuses a different one afterwards (spec 01 §2.2), so deleting that
file does not cost people a re-pin — it **locks every existing user out**
until they clear their pin store. It is excluded from the sync for the same
reason `.env` is, and more urgently. Back it up.

**Its verification is a boot.** Not `soli fmt --check`, because the demo is
full of hand-written props the formatter would reflow; not `soli test`,
because the suite wants a SoliDB a runner does not have. `soli serve` warms
every handler in every controller before it answers, so a boot that reaches
200 on `/up` has parsed the whole tree — which is what catches a syntax
error in a controller and a capability the server has never heard of, the
two ways this application has actually failed. No database is needed for
that: `/up` is Soli's own readiness probe.

The workflow finishes by fetching `/.well-known/eui` through the proxy and
printing the first sixteen hex digits of the manifest's checksum. The
manifest is what a client reads before it trusts a byte of the session; if
the publisher key had gone missing and been regenerated, the site would
still answer 200 and every existing user would be locked out. Compare the
checksum with the previous deploy's if someone reports being refused.

### One-time setup

```sh
mkdir -p /home/rocky/sites/eui-data.solisoft.net
install -m 0644 deploy/app.infos.data.example /home/rocky/sites/eui-data.solisoft.net/app.infos
install -m 0600 deploy/env.production.example /home/rocky/sites/eui-data.solisoft.net/.env
```

The `.env` needs the SoliDB connection (`SOLIDB_HOST`, `SOLIDB_DATABASE`,
`SOLIDB_USERNAME`, `SOLIDB_PASSWORD`) on top of `APP_ENV`, `SOLI_APP_HOSTS`
and `SOLI_SESSION_SECRET` — and `SOLI_WS_WORKERS=1`, which belongs there
and nowhere else: `start_script` in `app.infos` is a program and its
arguments, so an environment prefix on that line is read as the name of the
program to run. Then, once, after the first deploy:

```sh
cd /home/rocky/sites/eui-data.solisoft.net
soli db:migrate up
soli db:seed          # once, and never again: it fills the rooms
```

DNS before the first deploy, as for the other site.

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

3. Nothing to restart. `soli-proxy` runs with `--watch` on by default, which
   watches the sites directory and re-runs its discovery — and auto-starts
   what it finds — within a second of the folder appearing. A
   `systemctl restart soli-proxy` is only needed if this instance was started
   with `--watch=false`, and it briefly restarts **every** site, so pick a
   quiet moment.

   Between the folder appearing and the first deploy, the proxy may discover
   an app whose code is not there yet, fail to start it, and **quarantine**
   it — automatic restarts suspended. That is not a problem to fix: any
   explicit deploy releases it, which is what the workflow does.

4. Point the domain's DNS at the server **before** the first deploy; the proxy
   issues the Let's Encrypt certificate for the domains it has discovered, so
   the name has to resolve to the box by then.

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
| `DATA_DOMAIN` | variable | `eui-data.solisoft.net` (the default if absent) |
| `DATA_APP` | variable | the `name` in the demo application's `app.infos`, `eui-data` by default |

The host, the key, the user and the sites directory are shared: both
workflows deploy to the same box over the same SSH key.

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

`SOLI_VERSION` in each workflow is the version the CI installs *and* the
version the server is checked against. A CI testing a different version
than production tests nothing, so the two are compared explicitly rather
than assumed; when the server is upgraded, bump the pins in the same
commit.

The two workflows pin different versions today, and that is a state to get
out of rather than a design: one server runs one `soli`.

`deploy-data.yml` pins **2.2.1** and cannot go lower. The demo application
asks for the `nfc` capability, and the first release whose `eui-proto`
knows that bit is 2.2.1 — it pins eui `c5f19f3`. An older server refuses
the application at boot with `eui_capabilities: unknown capability 'nfc'`,
which is why the version is checked *before* the files are sent: finding
out afterwards would mean the old code is gone too.

# eui/examples/demo-app

Application Soli servie par `proxy/sites`. Ce fichier ne couvre qu'une règle :
comment lancer les tests. Le reste des conventions Soli est documenté dans les
`CLAUDE.md` des applications voisines (`syndic/`, `pdfx/`, `bonfire/`).

## Tests run on the build server, not here

This app has 10 specs under `tests/`. Run them through `rbuild`, which executes
the suite on the dedicated build machine and hands back the console output and
the exit code:

```bash
rbuild soli eui/examples/demo-app test                                 # whole suite
rbuild soli eui/examples/demo-app test tests/<the-relevant-spec>.sl    # one spec, fast feedback
rbuild soli eui/examples/demo-app test --coverage --coverage-min 90.0
```

`rbuild` rsyncs the working tree — uncommitted changes included — starts a
throwaway SoliDB and rewrites `SOLIDB_HOST` in the mirror's `.env.test`, so
nothing the suite does can reach the production database. Only `coverage/` comes
back, and only when you asked for it.

`soli fmt`, `soli lint` and `soli serve . --dev` stay local: they are short and
interactive. `test` is the one that saturates the machine — a forgotten browser
suite once ran there for four hours, starved the shared SoliDB, and turned five
passing specs into false failures.

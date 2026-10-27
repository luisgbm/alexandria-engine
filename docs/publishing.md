# Publishing and rollback

A draft save and a publication are separate operations. Publication validates
metadata, sanitized HTML, local links, media, URL collisions and generated output
before activating a new immutable release. The previous site stays available
while generation runs. Publication operations are serialized and require the
expected active release so stale browser requests cannot race an update.

Only `release/public/` is served. Generated routing configuration and reports,
source snapshots, publisher state and compact history remain private. Routing
changes that cannot safely use the installed Nginx maps are rejected for operator
review. Do not bypass that protection by editing the active tree.

## Browser workflow

Save and preview the selected revision, publish it, then verify its public URL.
Publicações lists retained versions, rollback and eligible older-version deletion.
The active version cannot be deleted; reference and independently verified backup
coverage checks protect retained resources. Keep a usable rollback version.

## Console workflow

The portable installer initializes publisher state. On an existing instance,
these commands require root and switch to its dedicated service identity:

```sh
sudo /INSTANCE/bin/alexandria-engine rebuild \
  --state /INSTANCE/data/state --expected-active ACTIVE_RELEASE
sudo /INSTANCE/bin/alexandria-engine rollback \
  --state /INSTANCE/data/state --release PREVIOUS_RELEASE \
  --expected-active ACTIVE_RELEASE
```

`rebuild` uses the active published snapshot without promoting private drafts or
staged settings. Use it for generator/template updates. Preserve the previous
executable too: publication rollback changes content, not the engine binary.

Earlier `*-staging` command names remain compatibility aliases. Back up executable,
state, content, compact history and shared media together; copying an expanded
public folder alone does not preserve publisher recovery.

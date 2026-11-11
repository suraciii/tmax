# tmax

Use tmax-managed model access inside DeepSeek Harness. The plugin projects the
configured tmax gateway into DSH; it does not copy keys or start the gateway.

## Install or update

The plugin ships with the tmax repository; it is not published to npm.

```sh
git clone https://github.com/suraciii/tmax.git
cd tmax
just dsh-install
```

Restart or refresh DSH Web, then open **tmax** in the sidebar panel list, beside
Plugins and Schedules. To update, pull and re-run `just dsh-install`; uninstall
with `just dsh-remove`.

## Connect

On the **tmax** page, select **Connect**. This writes only the DSH credential
reference through `tmax dsh connect`; the key is never returned to the browser.

The terminal equivalents are:

```sh
tmax dsh connect --home "$DSH_HOME" --dry-run
tmax dsh connect --home "$DSH_HOME"
```

## The page

The plugin claims a first-level page: `sidebar.panellist` adds the **tmax** row
beside Plugins and Schedules, and `main` renders the page itself, so it owns a
full content column instead of competing for room inside a settings section.

- **Access** — the DSH target, its home, the gateway address and readiness, and
  the route protocol, plus any blocker or Host error.
- **Models** — the projected model IDs as chips, or the reason there are none.
- **Usage and limits** — one card per provider that reports a quota, ordered by
  how much attention it needs. Each window gets a meter, its share consumed, and
  its reset date; the provider name opens that provider's own dashboard.

Both tmax window conventions (consumed and remaining) become the same filling
meter, so a row never needs mental arithmetic. Providers that report nothing
readable are omitted rather than shown as blank rows. Every colour resolves
through a `--dsw-alias-*` token, so the page follows the light and dark themes.

It also registers the `tmax_status` tool and the `tmax` skill for agents.

## Limitations

- Responses routes only when the tmax target declares them; mixed-protocol
  entries are refused.
- Models with unknown context capacity are hidden rather than given a guessed
  value; unknown input modalities are advertised as text-only.
- No quota editor, provider manager, or write tool. Those stay in the tmax CLI.
- Install/uninstall stays with DSH; tmax does not manage DSH profiles.

## License

Part of the tmax repository, distributed under GPL-3.0-only; see
[LICENSE](../LICENSE).

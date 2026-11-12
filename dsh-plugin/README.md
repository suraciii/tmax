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

- **Access** — the DSH target, its home, the gateway address and a genuinely
  three-valued readiness (ready / not ready / never reported, with tmax's
  `reason_code` when there is one), the default model, and every route protocol.
- **Models** — the projected model IDs as chips, marked `unverified` where tmax
  has not verified availability, or the reason there are none.
- **Usage and limits** — one card per provider tmax reports, ordered by how much
  attention it needs. Each window gets a proportional meter capped at a fixed
  width so every card shares one 100% end, its share consumed, and its reset
  date in the interface's language. A window at zero keeps a visible mark.
  A suspended provider carries its own line: `额度已用尽 · 月 · 重置 …`, taken from
  tmax's `reason_code`, `window`, and `reset_at`.

No provider is ever dropped. Credit counters (`counters.total`) are read as a
share, so a balance like `剩余 75.6亿 / 80亿` gets a real meter. A row whose shape
yields no share is shown as text with no meter at all — a bar is a claim about
proportion, and drawing one for a value the plugin cannot compute would be a
different kind of lie from omitting it.

Both tmax window conventions (consumed and remaining) become the same filling
meter, so a row never needs mental arithmetic. Every colour resolves through a
`--dsw-alias-*` token, so the page follows the light and dark themes; the two
semantic text tones mix toward the foreground label so they clear 4.5:1 in both
themes, not only in light.

The header states what the page can stand behind rather than what the plumbing
happens to be: `可用`, `受限`, `网关未就绪`, `状态未知`, `读取失败`, `未连接`.
A projection merely existing in DSH is not "ready", and a gateway whose readiness
was never reported is not "not ready".

A quota reading is a timestamped observation, not a live fact. Opening the page
reads the cache and shows it; it re-observes the providers
(`tmax provider refresh <id> --scope quota`) only when that cache has expired —
older than five minutes, or never read. Reaching a provider costs a live request
each, so a fresh open costs none. A deliberate **Refresh** always re-observes.

Freshness is stated once for the section ("readings are current"); a card repeats
it only when its own reading has gone stale. A failed read or refresh keeps the
last good table and names the failure beside it — an outage must not erase a
reading, and a refresh that fails must not look like a no-op.

## HTTP routes

Both are served on the DSH web server and guarded by the composition's
`connection.requestRejection` fence.

| Route | Effect |
|---|---|
| `GET /_tmax/status` | Secret-free projection summary, gateway readiness, models, blockers. |
| `GET /_tmax/usage` | Provider quota rows as last observed, with their observation time. |
| `POST /_tmax/usage/refresh` | Re-observes quotas through tmax, then returns the same payload. |
| `POST /_tmax/connect` | Runs `tmax dsh connect` for the configured target, then refreshes. |

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

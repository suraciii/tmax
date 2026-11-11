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

Restart or refresh DSH Web, then open **Settings → Models** and find the **tmax** card at the bottom. To update, pull and
re-run `just dsh-install`; uninstall with `just dsh-remove`.

## Connect

On **Settings → Models**, use the **tmax** card and select **Connect now**. This writes only
the DSH credential reference through `tmax dsh connect`; the key is never
returned to the browser.

The terminal equivalents are:

```sh
tmax dsh connect --home "$DSH_HOME" --dry-run
tmax dsh connect --home "$DSH_HOME"
```

## What the page shows

- connection and gateway readiness;
- the projected model count and model IDs;
- provider errors and blockers;
- one explicit connect action.

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

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

## What the card shows

The card sits at the bottom of **Settings → Models**, after the provider list, and
uses the same material as the provider rows above it. It follows the light and
dark themes because every colour comes from a `--dsw-alias-*` token.

- a status dot, plus a tag only when the state is not "connected";
- the projected model count and gateway readiness;
- the projected model IDs;
- the one action the card exists for: **Connect**, in primary weight, and only
  while access is not working;
- **usage and limits** per provider: each window as the amount still left, the
  reset date of the tightest window, and the provider dashboard behind its name.

Each window is reported in one direction (`remaining`) even though tmax reports
some windows as consumed and others as remaining, so the reader never converts
in their head. Windows a provider never names collapse into one count when their
values match. Providers that report nothing readable are omitted rather than
shown as blank rows.

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

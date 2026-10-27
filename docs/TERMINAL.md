# Open SHR Desk

[Project home](../README.md) · [Controls](USING.md) · [Engineering](ENGINEERING.md)

The pictured Channel panel runs in a terminal. The optional native window and
the offline simulator are separate views.

## Open the panel

Build from this checkout using the [build instructions](DEVELOPMENT.md).
A connected session needs a running GigPies provider, its show identity and
configured control permissions:

```text
shr-desk --terminal ENDPOINT SHOW_UUID EPOCH WRITER SCOPE
```

Use the provider and role options prepared for your system. Start with a terminal
of at least **160×45 cells**. Connection starts read-only; request control only
for the intended mix or configuration scope.

The shaded Channel panel in the screenshot requires the verified Rio setup and
`SHR_CHANNEL_MATERIALS=rio-kitty-v1`. Other terminals have a readable fallback.
The exact [terminal/font setup](reference/TERMINAL.md) is kept with the engineering
instructions. Physical controller mappings are not yet qualified.

With channel processing enabled (`--processing`), **F2** opens Channel in FOH
scope. [Continue to the controls](USING.md).

## Practice offline

```sh
target/release/shr-desk simulate
```

Try `select 11`, `level -3`, `page channel` and `help`.
This is a text-command simulator, not the pictured provider-backed panel. It
opens no audio hardware and forgets its state on exit.

## Leaving and reconnecting

Ctrl+C closes the terminal console. Resizing or losing focus cancels held input
and invalidates confirmation; review again before applying. Reconnect starts
read-only and does not replay edits. The audio engine runs separately.

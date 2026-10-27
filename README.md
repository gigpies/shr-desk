# SHR Desk

**The audio desk for GigPies.** Channel EQ and compression, faders, monitor sends,
master EQ and effects, with manual control and optional automation.

[![Channel view with four-band EQ, compressor, LED meters and physical-style controls](https://raw.githubusercontent.com/gigpies/gigpies/main/docs/assets/panel-desk-channel.png)](https://raw.githubusercontent.com/gigpies/gigpies/main/docs/assets/panel-desk-channel.png)

Actual terminal capture with deterministic software audio, approved October 20,
2026. Click for full resolution. The image is maintained in GigPies.

[**Using the desk**](docs/TERMINAL.md) · [**GigPies guide**](https://github.com/gigpies/gigpies/blob/main/docs/README.md) · [Engineering](docs/ENGINEERING.md)

## At your fingertips

- Four-band channel EQ, compression, fader, pan, mute and Hold.
- Fullscreen EQ with a separately selectable spectrum source.
- Monitor sends, master EQ and supported stereo FX controls.
- Review changes before applying them; unavailable controls stay unavailable.

The terminal panel and optional native window use the GigPies processing engine.
**Physical controller/display acceptance is still pending.** The screenshot shows
software rendering. Desk recording controls are not connected yet.

## Try the offline simulator

After [building](docs/DEVELOPMENT.md):

```sh
target/release/shr-desk simulate
```

Try `select 11`, `level -3`, `page channel`, then `help`.
This separate text simulator opens no audio device and does not save a show.
For the pictured panel, use the [terminal setup](docs/TERMINAL.md#open-the-panel).

[Software status](docs/STATUS.md) · [MIT license](LICENSE) · [Font and artwork notices](THIRD_PARTY.md)

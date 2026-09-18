# yomiage

An application that converts text files to speech with [VOICEVOX](https://voicevox.hiroshiba.jp/) and serves the generated audio.

## Features

- Converts text files to WAV audio
- Regenerates audio when the input or speaker changes
- Serves generated WAV files with a basic HTML audio player

## Environment variables

- `CONFIG_PATH`: path to the TOML configuration file
- `BIND_ADDR`: address on which the HTTP server listens
- `DATA_DIR`: directory where generated audio and conversion state are stored
- `VOICEVOX_URL`: base URL of the VOICEVOX Engine
- `POLL_INTERVAL_SECONDS`: interval in seconds between checks for input file changes
- `RUST_LOG`: log filtering and verbosity settings

## Configuration

The configuration file defines the VOICEVOX speaker and the text files to convert.

- `voicevox`: VOICEVOX settings
  - `speaker`: speaker ID passed to VOICEVOX
- `target`: text file to convert; this section can be repeated
  - `id`: unique identifier containing only ASCII letters, digits, `-`, or `_`
  - `input_path`: path to a text file; relative paths are resolved from the configuration file

Example configuration:

```toml
[voicevox]
speaker = 3

[[target]]
id = "sample"
input_path = "./sample.txt"
```

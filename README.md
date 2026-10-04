# OCPlayground

`ocplay` — a console (CLI) emulator for [OpenComputers](https://github.com/MightyPirates/OpenComputers).
It boots the real OpenComputers Lua machine (`machine.lua` + `bios.lua` + OpenOS) and renders the
in-game screen to the terminal with full color support.

```
ocplay ./computer.yaml test.lua
```

## Decisions

These choices were made up front and drive the implementation:

| Topic | Decision |
| --- | --- |
| Language / runtime | **Rust + `mlua`** with bundled **Lua 5.4** |
| Fidelity | Run the **actual** OpenComputers system Lua (vendored), not a reimplementation |
| Components | Start with **core** only, but design around a component trait + registry so more can be injected later |
| Script mode | Default is **silent** (non-interactive); `--interactive` opts into the live shell |
| Config | Schema is **designed here** (see below) |

### What "core" means

The minimum needed to boot OpenOS, run scripts, and reproduce in-game behavior:

- `eeprom` — serves the BIOS and boot address
- `screen` — text buffer with 1/4/8-bit color, resolution, viewport, palette
- `gpu` — bind, colors, palette, depth, resolution, viewport, `set`/`copy`/`fill`/`get`
- `keyboard` — `key_down`/`key_up`/`clipboard` with OpenComputers key codes
- `filesystem` — `open`/`read`/`write`/`seek`/`close`, listing, metadata; RO OpenOS root + RW mounts
- `computer` — `beep`, `getDeviceInfo`, `getProgramLocations`, users
- `internet` — HTTP(S) `request` with a streaming handle (`read`/`response`/`finishConnect`/`close`)

Redstone, modems and other cards are intentionally out of scope for the first phase and will be
added through the same component registry.

## Architecture

```
src/
  main.rs               # CLI (clap)
  config.rs             # serde_yaml schema
  machine/mod.rs        # Lua init, resume loop, signal queue, sleep scheduler
  machine/host_api.rs   # component/computer/system/unicode/os globals
  components/mod.rs     # Component trait + Registry (DI seam)
  components/…          # eeprom, screen, gpu, keyboard, filesystem, computer
  color.rs              # PackedColor, palettes, depth conversion, wcwidth
  term/render.rs        # text buffer -> ANSI (truecolor/256/16), diffing
  term/input.rs         # crossterm raw mode, key -> OC key code
  term/mod.rs           # alternate screen, cursor, restore on exit
  run.rs                # script injection + silent/interactive modes
assets/system/          # vendored machine.lua, bios.lua, loot/openos/
```

- The host runs `machine.lua` as a Lua thread and interprets yields like OpenComputers does:
  a `number` is a requested sleep (interruptible by signals), a `boolean` is shutdown/reboot,
  and a `function` is a host callback that is invoked and resumed with its result.
- Most component methods are **direct**; the few that aren't (e.g. `internet.request`) use the
  host-callback path above. HTTP runs on a background thread so the screen keeps updating while a
  request is in flight.
- Terminal output is produced by rendering the `gpu`/`screen` text buffer, not by parsing guest ANSI.

## Configuration (`computer.yaml`)

```yaml
name: my-computer

# KiB of RAM available to the Lua machine.
memory: 512

cpu:
  tier: 2

gpu:
  # Maximum color depth: 1 -> 1-bit, 2 -> 4-bit, 3 -> 8-bit.
  tier: 3
  screen:
    width: 80
    height: 25
    maxDepth: 8

eeprom:
  # "builtin" uses the vendored OpenComputers BIOS, otherwise a path to a .lua file.
  bios: builtin
  # "auto" picks the first bootable filesystem, otherwise a filesystem label.
  boot: auto

filesystems:
  # "builtin" serves a session copy of the vendored OpenOS; otherwise a host directory.
  - label: openos
    path: builtin
    mount: /
    readonly: false
  - label: data
    path: ./data
    mount: /home
    readonly: false

components:
  keyboard: true

internet:
  # Whether the internet card is present and HTTP requests are allowed.
  enabled: true
  # HTTP request timeout in seconds; 0 = no timeout.
  timeout: 30
  # TCP sockets are not implemented yet.
  tcp: false
  userAgent: opencomputers/ocplay

run:
  script: test.lua
  timeout: 0
  interactive: false
  # Seconds to keep the final screen visible after the machine stops.
  terminateDelay: 5
```

CLI overrides:

```
ocplay [--interactive | --silent] [--timeout N] <computer.yaml> [script.lua]
```

- **silent** (default): no stdin; run the script, render while it goes, exit when it finishes or times out.
- **`--interactive`**: run the script, then hand control to the live OpenOS shell.

## Usage

```sh
# Boot OpenOS, run a script, then exit (non-interactive)
cargo run -- examples/computer.yaml examples/test.lua

# Drop into the live OpenOS shell after the script
cargo run -- --interactive examples/computer.yaml examples/test.lua

# Stop after 30 seconds
cargo run -- --timeout 30 examples/computer.yaml examples/test.lua
```

The script is copied into the emulated filesystem and executed once OpenOS has
booted. In non-interactive mode the computer shuts down when the script finishes;
in `--interactive` mode it then drops into the shell. When stdout is not a
terminal the final screen is printed as plain text, which is convenient for CI.

## Status

Implemented and working end-to-end:

- Real OpenOS boots and runs (vendored `machine.lua` / BIOS / OpenOS).
- `screen` + `gpu` with 1/4/8-bit color, palette, resolution and viewport.
- ANSI truecolor terminal rendering (via `ratatui`), wide-character aware.
- `keyboard` with OpenComputers scancodes, modifiers and Ctrl+C interrupts.
- `filesystem` backed by a session copy of OpenOS plus configured mounts.
- `eeprom`, `computer` and the machine host API (`component`, `computer`,
  `system`, `unicode`, `os`).
- `internet` card: HTTP/HTTPS `request` (GET/POST/custom method, headers, POST
  bodies) with OpenComputers' streaming handle semantics, run on a worker thread.
- Script autostart through OpenOS's own `rc` mechanism.

Not yet implemented (tracked as follow-up work):

- TCP sockets (`internet.connect`) and the `internet_ready` signal.
- Mouse/touch events for screens.
- Modems, redstone and other components.
- GPU VRAM buffers (`allocateBuffer`, `bitblt`).
- Persistence / save data.

## Notes

- `font.hex` (~4.7 MB) is **not** vendored: ANSI rendering uses the host font, not the in-game bitmap font.

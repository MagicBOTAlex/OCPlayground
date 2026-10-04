# OCPlayground

`ocplay` — a console (CLI) emulator for [OpenComputers](https://github.com/MightyPirates/OpenComputers).
It boots the real OpenComputers Lua machine (`machine.lua` + `bios.lua` + OpenOS) and renders the
in-game screen to the terminal with full color support.

```
ocplay ./computer.yaml test.lua
```

## Installation

### Ubuntu / Linux (prebuilt)

```sh
curl -fsSL https://raw.githubusercontent.com/MagicBOTAlex/OCPlayground/master/install.sh | bash
```

The installer downloads the latest released build into `~/.local/share/ocplay`
and symlinks `~/.local/bin/ocplay`. Options: `--version <tag>` and
`--prefix <dir>`.

Prebuilt `x86_64-unknown-linux-gnu` tarballs (with `.sha256` checksums) are
attached to [releases](https://github.com/MagicBOTAlex/OCPlayground/releases).
Every push to `master` refreshes the rolling `latest` release; `v*` tags create
versioned releases. The workflow builds on Ubuntu 22.04 so the binary runs on
22.04 and newer.

Update an installed build in place:

```sh
ocplay --upgrade
```

`--upgrade` downloads the latest release and replaces both the binary and its
bundled system files. Nix-managed installs refuse it (Nix owns the store paths);
use `nix profile upgrade ocplay` there instead.

### Nix (flakes)

```sh
nix profile install .#ocplay     # install / upgrade
nix profile remove ocplay        # remove
```

Or run/build it without installing:

```sh
nix run .#ocplay -- --help
nix build .#ocplay
```

The Nix package bundles the vendored OpenOS and points `OCPLAY_SYSTEM` at it, so
the installed binary works from anywhere.

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
- `screen` — text buffer with 1/4/8-bit color, resolution, viewport, palette, and touch/pointer signals
- `gpu` — bind, colors, palette, depth, resolution, viewport, `set`/`copy`/`fill`/`get`
- `keyboard` — `key_down`/`key_up`/`clipboard` with OpenComputers key codes
- `filesystem` — `open`/`read`/`write`/`seek`/`close`, listing, metadata; RO OpenOS root + RW mounts
- `computer` — `beep`, `getDeviceInfo`, `getProgramLocations`, users
- `internet` — HTTP(S) `request` with a streaming handle (`read`/`response`/`finishConnect`/`close`)

Redstone, modems, robots and other cards are **unplanned** (see [Status](#status)); they can still be
added through the same component registry.

## Architecture

```
src/
  main.rs               # CLI (clap)
  config.rs             # serde_yaml schema
  machine/mod.rs        # Lua init, resume loop, signal queue, sleep scheduler
  machine/host_api.rs   # component/computer/system/unicode/os globals
  components/mod.rs     # Component trait + Registry (DI seam)
  components/…          # eeprom, screen, gpu, keyboard, filesystem, computer, internet
  color.rs              # PackedColor, palettes, depth conversion, wcwidth
  term/mod.rs           # alternate screen, ANSI rendering, key + mouse input
  run.rs                # script injection + silent/interactive modes
  tests/e2e.rs          # end-to-end tests against the compiled binary
assets/system/          # vendored machine.lua, bios.lua, loot/openos/
```

- The host runs `machine.lua` as a Lua thread and interprets yields like OpenComputers does:
  a `number` is a requested sleep (interruptible by signals), a `boolean` is shutdown/reboot,
  and a `function` is a host callback that is invoked and resumed with its result.
- Most component methods are **direct**; the few that aren't (e.g. `internet.request`) use the
  host-callback path above. HTTP runs on a background thread so the screen keeps updating while a
  request is in flight.
- Terminal output is produced by rendering the `gpu`/`screen` text buffer, not by parsing guest ANSI.
- In interactive mode the terminal mouse is captured and converted into OpenComputers screen
  `touch`/`drag`/`drop`/`scroll` signals, matching the in-game 1-based (or high-precision) coordinates.

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
  # Capture the terminal mouse and deliver it to the screen as touch signals
  # (interactive mode only).
  mouse: true

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
ocplay [--interactive | --silent] [--timeout N] [--upgrade | --llm] <computer.yaml> [script.lua] [args...]
```

- **silent** (default): no stdin; run the script, render while it goes, exit when it finishes or times out.
- **`--interactive`**: run the script, then hand control to the live OpenOS shell.
- **`[args...]`**: extra arguments are passed to the script as varargs, exactly like an OpenOS
  program run from the shell — read them with `local args, options = require("shell").parse(...)`
  or `local args = {...}`.
- **`--upgrade`**: update to the latest release and exit (not for Nix installs).
- **`--llm`**: print an extremely detailed usage reference and exit (no config needed).

## Usage

```sh
# Boot OpenOS, run a script, then exit (non-interactive)
cargo run -- examples/computer.yaml examples/test.lua

# Drop into the live OpenOS shell after the script
cargo run -- --interactive examples/computer.yaml examples/test.lua

# Interactive mouse/touch demo (click/drag to paint, right-click or any key to quit)
cargo run -- --interactive examples/computer.yaml examples/mouse.lua

# Stop after 30 seconds
cargo run -- --timeout 30 examples/computer.yaml examples/test.lua

# Pass arguments to the script (available as `...`)
cargo run -- examples/computer.yaml examples/test.lua "test"
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
- Terminal mouse input delivered as screen `touch`/`drag`/`drop`/`scroll` signals,
  including high-precision mode coordinates (demo: `examples/mouse.lua`).
- `keyboard` with OpenComputers scancodes, modifiers and Ctrl+C interrupts.
- `filesystem` backed by a session copy of OpenOS plus configured mounts.
- `eeprom`, `computer` and the machine host API (`component`, `computer`,
  `system`, `unicode`, `os`).
- `internet` card: HTTP/HTTPS `request` (GET/POST/custom method, headers, POST
  bodies) with OpenComputers' streaming handle semantics, run on a worker thread.
- Script autostart through OpenOS's own `rc` mechanism.

### Tests

```sh
cargo test        # unit tests + end-to-end tests that boot OpenOS
```

Unit tests cover config parsing, URL validation, the mouse→signal mapping and
screen signal emission. The end-to-end tests run the compiled binary against a
temporary computer: booting and running a script, an HTTP GET against a local
server, and the missing-root-filesystem error.

### Unplanned

The following are explicitly **not planned** for now (they may be contributed
through the component registry). Everything below is a known gap:

- **Components**: `modem` / `tunnel`, `data` (crypto/compress), `drive`,
  `redstone` (+ bundled / wireless / signaller), `robot`, `drone`, `tablet`,
  `server` (RPC), `geolyzer`, `motion_sensor`, `transposer`, `trading`, `sign`,
  `navigation`, `piston`, `leash`, `database`, `experience`, `generator`,
  `crafting`, `debug`, and all upgrades.
- **Internet**: TCP sockets (`internet.connect`, `internet_ready`), proxies, and
  the allow/deny address filtering rules.
- **GPU**: VRAM buffers (`allocateBuffer`, `freeBuffer`, `bitblt`, active
  buffer, buffer memory reporting).
- **Filesystem**: OpenComputers-style `userdata` file handles (we use integer
  tokens), symlinks, and accurate space accounting.
- **Host fidelity**: the `userdata` host API and persistence (`save`/`load`,
  NBT disk images), enforced memory limits, execution-time interrupts for
  runaway scripts, sandboxed bytecode, and alternate architectures (LuaJ / 5.2 / 5.3).
- **Front-end**: screen resize handling, the in-game `font.hex` bitmap font,
  audio (`computer.beep`), and multi-screen support.
- **Other**: save/load of machine state, network filtering config, and CI.

## Notes

- `font.hex` (~4.7 MB) is **not** vendored: ANSI rendering uses the host font, not the in-game bitmap font.

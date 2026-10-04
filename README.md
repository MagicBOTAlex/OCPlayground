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

Networking, redstone, and other cards are intentionally out of scope for the first phase and will be
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
  a `number` is a requested sleep (interruptible by signals), a `boolean` is shutdown/reboot.
- All component methods are registered as **direct**, so component calls never yield across the host
  boundary.
- Terminal output is produced by rendering the `gpu`/`screen` text buffer, not by parsing guest ANSI.

## Configuration (`computer.yaml`)

```yaml
name: my-computer
memory: 512                 # KiB
cpu:  { tier: 2 }
gpu:
  tier: 3                   # max depth: 1 -> 1-bit, 2 -> 4-bit, 3 -> 8-bit
  screen: { width: 80, height: 25, maxDepth: 8 }
eeprom: { bios: builtin, boot: auto }   # builtin | path
filesystems:
  - { label: openos, path: builtin, mount: /, readonly: false }  # served from a session copy
  - { label: data,   path: ./data,  mount: /home, readonly: false }
components: { keyboard: true }
run: { script: test.lua, timeout: 0, interactive: false }
```

CLI overrides:

```
ocplay [--interactive | --silent] [--timeout N] <computer.yaml> [script.lua]
```

- **silent** (default): no stdin; run the script, render while it goes, exit when it finishes or times out.
- **`--interactive`**: run the script, then hand control to the live OpenOS shell.

## Milestones

1. Scaffold + config + vendored system Lua + Lua embed → boot OpenOS to text.
2. Screen/GPU buffer + ANSI color renderer.
3. Keyboard + event loop.
4. Filesystem (RO root + RW mounts).
5. Script-run mode + YAML polish.
6. Color depth/palette modes, cursor blink, resize, optional mouse.

## Notes

- `font.hex` (~4.7 MB) is **not** vendored: ANSI rendering uses the host font, not the in-game bitmap font.

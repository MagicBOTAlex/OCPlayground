# ocplay — complete usage reference

ocplay is a console (terminal) emulator for OpenComputers. It boots the real
OpenComputers Lua machine (`machine.lua` + `bios.lua` + OpenOS) with a Rust host
and renders the in-game screen to your terminal. It can run a Lua program
non-interactively, or drop you into a live OpenOS shell.

This document is written to be read by a language model: it is exhaustive and
states defaults, types, and exact behaviors.

================================================================================
1. QUICK START
================================================================================

    # Run a program non-interactively (boot OpenOS, run script, exit)
    ocplay ./computer.yaml ./script.lua

    # Interactive shell (captures the terminal keyboard and mouse)
    ocplay --interactive ./computer.yaml ./script.lua

    # Pass arguments to the script (available as `...`)
    ocplay ./computer.yaml ./script.lua hello world

    # Print this reference
    ocplay --llm

================================================================================
2. PLATFORMS AND INSTALLATION
================================================================================

Supported target: x86_64 Linux (built on Ubuntu 22.04, runs on 22.04+).

Option A — prebuilt installer (Ubuntu/Linux):

    curl -fsSL https://raw.githubusercontent.com/MagicBOTAlex/OCPlayground/master/install.sh | bash

    The installer downloads the latest release tarball, verifies its .sha256,
    extracts it to ~/.local/share/ocplay (binary + bundled OpenOS), and symlinks
    ~/.local/bin/ocplay to it. Options: --version <tag>, --prefix <dir>.

Option B — Nix (flakes):

    nix profile install .#ocplay
    nix run .#ocplay -- --help
    nix build .#ocplay

    Nix-managed installs are self-contained in the store. `ocplay --upgrade`
    refuses to run for them; use `nix profile upgrade ocplay`.

Option C — build from source:

    cargo build --release
    ./target/release/ocplay ./computer.yaml ./script.lua

Updating a non-Nix install:

    ocplay --upgrade

    Downloads the latest release and replaces the running binary and its bundled
    assets in place. Prints "already up to date" when nothing changed. Refuses
    when the executable lives under /nix/store.

================================================================================
3. COMMAND-LINE INTERFACE
================================================================================

Synopsis:

    ocplay [OPTIONS] [CONFIG] [SCRIPT] [ARGS]...

Options:
    --interactive        Attach the terminal as a keyboard (and mouse) and drop
                         into the live OpenOS shell after the script finishes.
                         Ctrl+C force-quits ocplay; Ctrl+Alt+C interrupts the
                         guest. Without it, ocplay runs non-interactively.
    --timeout <SECONDS>  Stop after this many seconds. 0 (or omitted) means no
                         limit. Overrides run.timeout from the config.
    --upgrade            Download and install the latest release, then exit.
                         Not valid for Nix installs. Exits 1 when unsupported.
    --llm                Fetch and print this reference, then exit. No config
                         needed, but it requires network access. Override the
                         source with the OCPLAY_LLM_URL environment variable.
    -h, --help           Print short help and exit.
    -V, --version        Print version and exit.

Positionals:
    CONFIG   Path to a YAML configuration file (computer.yaml). Required unless
             --upgrade or --llm is given.
    SCRIPT   Optional Lua program to run once OpenOS has booted.
    ARGS...  Any remaining arguments are passed to SCRIPT as Lua varargs and are
             read inside the program as `...`. Use
                 local args, options = require("shell").parse(...)
             to parse flags/options, or `local args = {...}` for raw values.

Notes:
    - Flags must precede positional arguments (standard clap behavior).
    - ARGS capture everything after SCRIPT, including values starting with '-'.

Exit codes:
    0   Normal completion (including when --timeout stops the machine).
    1   Setup or runtime failure (bad config, missing root filesystem, Lua error
        surfaced by the machine, failed/unavailable --upgrade).
    2   CLI usage error (e.g. no config given, unknown option). clap also uses 2.

Environment:
    OCPLAY_SYSTEM   Directory containing `machine.lua`, `bios.lua` and
                    `loot/openos/`. Overrides automatic discovery. Otherwise
                    ocplay looks next to the executable (assets/system), then at
                    the compile-time source tree.

================================================================================
4. CONFIGURATION FILE (computer.yaml)
================================================================================

YAML, block style. Unknown keys are rejected (deny_unknown_fields). Every field
has a default, so an empty file `{}` is valid. Full example:

    name: my-computer        # string, default "computer"

    memory: 512              # integer KiB of RAM, default 512
                             # (reported to Lua; not hard-enforced)

    cpu:
      tier: 2                # 1..3, default 2

    gpu:
      tier: 3                # 1 -> 1-bit, 2 -> 4-bit, 3 -> 8-bit color
      screen:
        width: 80            # integer columns, default 80
        height: 25           # integer rows, default 25
        maxDepth: 8          # 1, 4 or 8 bits, default 8

    eeprom:
      bios: builtin          # "builtin" (vendored BIOS) or a path to a .lua file
      boot: auto             # "auto" or a filesystem label/address to boot from

    filesystems:
      - label: openos        # required string
        path: builtin        # "builtin" (session copy of OpenOS) or a host dir
        mount: /             # mount point, default "/"
        readonly: false      # default false
      - label: data
        path: ./data         # a host directory
        mount: /home
        readonly: false

    components:
      keyboard: true         # default true; terminal keys -> key signals
      mouse: true            # default true; terminal mouse -> touch signals
                             # (mouse capture is enabled only in --interactive)

    internet:
      enabled: true          # default true; false makes request() fail
      timeout: 30            # seconds; 0 = no timeout (default 0)
      tcp: false             # TCP sockets (not implemented; default false)
      userAgent: opencomputers/ocplay   # default User-Agent header

    run:
      script: test.lua       # optional default script path
      timeout: 0             # seconds, default 0 (no limit)
      interactive: false     # default false
      terminateDelay: 5      # seconds to hold the final screen, default 5

Root filesystem is required: there must be a filesystem mounted at "/". If
`path: builtin` and the mount is "/" and it is not read-only, ocplay copies the
vendored OpenOS into a fresh temporary session directory and runs there, so
changes are discarded on exit.

IMPORTANT: `filesystems[].path` is resolved against the process working directory,
NOT the directory containing computer.yaml. `path: .` therefore mounts wherever
you launched ocplay from. Prefer absolute paths. (See section 13.)

================================================================================
5. RUN MODES
================================================================================

Silent (default, no --interactive):
    - The script (if any) runs after OpenOS boots, then the machine shuts down.
    - No keyboard input is read.
    - If stdout is not a terminal, the final screen buffer is printed as plain
      text (handy for CI/pipelines).
    - If --interactive (or run.interactive) is set but stdout is not a usable
      terminal (not a TTY, or the terminal reports 0x0), ocplay prints a warning
      and runs non-interactively instead of waiting forever.

Interactive (--interactive):
    - The terminal is switched to an alternate screen and raw mode.
    - Keys are delivered to OpenOS; in mouse mode, mouse events become screen
      touch signals.
    - After the script finishes (or if there is no script) you get the live
      OpenOS shell at `/home`.
    - Ctrl+C is a HOST signal: it force-quits ocplay. To interrupt the running
      OpenOS program instead, use Ctrl+Alt+C (the standard OpenComputers
      interrupt), which is delivered to the guest.

After the machine stops, ocplay keeps the final screen for `run.terminateDelay`
seconds (default 5). Press Ctrl+C during that wait to skip it.

================================================================================
6. THE SCRIPT
================================================================================

The script is copied into the emulated filesystem at:
    /home/ocplay_autorun.lua

A service `/etc/rc.d/ocplay.lua` is created and enabled in `/etc/rc.cfg`; it runs
after OpenOS boots. It also mounts any configured extra filesystems. It then
loads the script and calls it with the CLI ARGS as varargs:

    pcall(function(...) return assert(loadfile("/home/ocplay_autorun.lua"))(...) end, <ARGS...>)

So inside the script `...` holds your arguments, exactly like an OpenOS program
launched from the shell. If the script errors, the error is reported and, in
silent mode, the process exits with code 1.

Caveat: the autorun service wraps the load in `pcall(...)` and ignores its return
value, so an error raised *inside* the script (for example a failed `require`)
does not stop ocplay and is not printed. If your script appears to do nothing,
suspect a swallowed error and reproduce it interactively. (See section 13.)

================================================================================
7. OPENOS / HOST API FROM LUA
================================================================================

Globals provided by the host: `component`, `computer`, `system`, `unicode`, `os`
(the machine sandbox adds the usual OpenOS libraries on top, e.g. `require`,
`term`, `event`, `filesystem`, `shell`, `internet`).

Colors (important): `require("colors")` returns palette *indices*
(white = 0, black = 15, blue = 11, ...). `gpu.setForeground`/`setBackground`
treat a bare number as a packed 24-bit RGB value unless you pass `true` as the
optional second (`palette`) argument. Mixing these up is the classic "black
screen": `gpu.setForeground(colors.white)` (0) paints black (0x000000). Either
call `gpu.setForeground(color, true)`, or use explicit RGB constants
(0xFFFFFF white, 0x000000 black, 0x333399 blue). Note the 8-bit format's 16
palette slots are a grayscale ramp, so RGB constants are also what you want for
real colors on tier-3 screens.

Components implemented (use `component.list()`, `component.methods(addr)`,
`component.invoke(addr, method, ...)` or `component.<type>`):

  gpu        bind, getScreen, getBackground, setBackground, getForeground,
             setForeground, getPaletteColor, setPaletteColor, getDepth,
             setDepth, maxDepth, getResolution, setResolution, maxResolution,
             getViewport, setViewport, get, set, copy, fill
             (no VRAM buffers: allocateBuffer/bitblt are not implemented)

  screen     isOn, turnOn, turnOff, getAspectRatio, getKeyboards, isPrecise,
             setPrecise, isTouchModeInverted, setTouchModeInverted
             Signals: touch, drag, drop, scroll (see section 9)

  keyboard   no component methods; input arrives as key_down/key_up/clipboard
             signals

  filesystem getLabel, setLabel, isReadOnly, spaceTotal, spaceUsed, exists,
             size, isDirectory, lastModified, list, makeDirectory, remove,
             rename, open, read, seek, write, close
             (file handles are integer tokens, not userdata)

  eeprom     get, set, getLabel, setLabel, getSize, getChecksum, getData,
             getDataSize

  computer   beep (no audio), getDeviceInfo, getProgramLocations, users,
             addUser, removeUser, isRunning; plus computer.* helpers: address,
             tmpAddress, realTime, uptime, energy, maxEnergy, freeMemory,
             totalMemory, getBootAddress, setBootAddress, pushSignal,
             getArchitectures, getArchitecture, setArchitecture, shutdown

  internet   isHttpEnabled, request, isTcpEnabled, connect (see section 8)

================================================================================
8. INTERNET
================================================================================

Idiomatic use through the OpenOS library:

    local internet = require("internet")

    -- GET, iterate the response body
    for chunk in internet.request("https://example.com/") do
      io.write(chunk)
    end

    -- POST with a body and headers
    local handle = internet.request("https://httpbin.org/post", "a=1&b=2", {
      ["Content-Type"] = "application/x-www-form-urlencoded",
    })

Component-level handle (what `internet.request` returns):
    handle:read([n])          -> string (up to 2048 bytes), "" while pending,
                                 nil at end of stream, or nil,reason on error
    handle:response()         -> code, message, headers (nil until complete)
    handle:finishConnect()    -> true when done, false while pending
    handle:close()            -> close the stream

Behavior notes:
    - Requests run on a background thread, so the machine keeps rendering.
    - Allowed schemes: http and https only. Other schemes return a soft error.
    - `read` on a response with status >= 400 fails (OpenComputers never exposes
      the error stream), but `response()` still reports the status and headers.
    - Response headers are returned as a table mapping name -> array of strings.
    - `internet.connect` (TCP) is not implemented; it returns unavailable.

================================================================================
9. MOUSE / TOUCH
================================================================================

In --interactive mode with `components.mouse: true`, terminal mouse events are
delivered to the machine as screen signals with the screen address first:

    signal:  touch  <screenAddress> <x> <y> <button>
    signal:  drag   <screenAddress> <x> <y> <button>
    signal:  drop   <screenAddress> <x> <y> <button>
    signal:  scroll <screenAddress> <x> <y> <delta>

    button: 0 = left, 1 = right, 2 = middle.  delta: +1 up, -1 down.

Coordinates are 1-based integers normally. Call `component.screen.setPrecise(true)`
to switch to 0-based fractional numbers. Events outside the screen are dropped.
Mouse capture is only enabled when running `--interactive` AND
`components.mouse: true`; without either, no touch/scroll signals are produced.
Coordinates map 1:1 onto the emulated screen buffer (top-left is 1,1) and are
independent of the host terminal's own size.

Example:

    local event = require("event")
    while true do
      local name, address, x, y, data = table.unpack(event.pull(120))
      if name == "touch" then print("click at " .. x .. "," .. y) end
    end

================================================================================
10. EXAMPLE PROGRAMS (in the repository)
================================================================================

    examples/computer.yaml   sample configuration
    examples/test.lua        colors, color depths, text buffer (non-interactive)
    examples/mouse.lua       interactive touch demo (click/drag/scroll, right-click quits)

Run them:

    ocplay ./examples/computer.yaml ./examples/test.lua
    ocplay --interactive ./examples/computer.yaml ./examples/mouse.lua
    ocplay ./examples/computer.yaml ./examples/test.lua "an argument"

================================================================================
11. TESTS
================================================================================

    cargo test

Unit tests cover config parsing, URL validation, the mouse->signal mapping,
screen signal emission, and the Ctrl+C host-quit logic (plain Ctrl+C quits,
Ctrl+Alt+C is left for the guest). End-to-end tests run the compiled binary to
boot OpenOS, run a script, perform an HTTP GET against a local server, check that
script arguments arrive, and check the missing-root-filesystem error.

================================================================================
12. LIMITATIONS / UNPLANNED
================================================================================

    - Components not implemented: modem/tunnel, data, drive, redstone, robot,
      drone, tablet, server, geolyzer, motion_sensor, transposer, trading, sign,
      navigation, piston, leash, database, experience, generator, crafting,
      debug, and all upgrades.
    - Internet: no TCP, proxies, or address filtering rules.
    - GPU: no VRAM buffers (allocateBuffer/freeBuffer/bitblt).
    - Filesystem: integer handles instead of userdata; no persistence (session
      temp dir); approximate space accounting.
    - Host fidelity: `userdata` API stubbed; no save/load (NBT); memory limits
      reported but not enforced; wall-clock timeout instead of injected
      interrupts; only the Lua 5.4 architecture.
    - Front-end: no resize handling; host font instead of the in-game font.hex;
      no audio; single screen.

================================================================================
13. KNOWN ISSUES / GOTCHAS
================================================================================

Behaviors to be aware of when authoring programs or scripting ocplay. Some are
emulator rough edges; they are listed so you do not have to rediscover them.

* filesystem paths are CWD-relative, not config-relative.
    `filesystems[].path` is resolved against the process working directory, not
    the directory containing computer.yaml. `path: .` mounts wherever you ran
    ocplay from, so the same config can mount different directories. Use absolute
    paths for predictable mounts.

* Bare color numbers are 24-bit RGB, not palette indices.
    `require("colors")` gives palette indices (white = 0), but the GPU treats a
    bare number as RGB and 0 means black. Pass the `palette` flag (second arg) or
    use RGB constants. This is the most common cause of a "black screen" where
    text is actually being drawn but is invisible. (See section 7.)

* Scripts can fail silently under the rc service.
    The autorun script is invoked through `pcall(...)` with no handler and its
    result is discarded, so an error inside it (e.g. `require` of a module not on
    package.path) neither stops ocplay nor prints anything. If a script "does
    nothing", suspect a swallowed error and reproduce it interactively.

* Errors, prints and the final frame share one screen.
    In silent mode the final screen buffer is dumped as plain text; a script
    error or `print` output is written into that same buffer and can overlap and
    be truncated by the rendered UI.

* Host messages are interleaved with the screen dump.
    `ocplay: timed out after Ns` and the interactive-fallback warning go to
    stderr while the final screen goes to stdout, but when captured together they
    sit on adjacent lines with no separator.

* No scripted input injection.
    There is no built-in way to replay a fixed sequence of keyboard/mouse events,
    so UI end-to-end tests must inject the event source inside the Lua program
    (for example a mock that replaces event.pull).

* Interactive requires a usable terminal.
    --interactive (or run.interactive) is downgraded to a non-interactive run
    when stdout is not a TTY or the terminal reports 0x0, printing a warning. If
    the program then blocks (e.g. event.pull forever), there is no auto-shutdown;
    use Ctrl+C / SIGINT to escape. (See section 5.)

* Relative `require` and in-game portability.
    ocplay copies a script to /home/ocplay_autorun.lua and any extra host
    filesystems are mounted at the configured points; a program that `require`s
    sibling modules therefore only works under ocplay's mounts. When the same
    program is run in-game, OpenOS only searches its own package.path, so keep the
    program self-contained (single file) or install its modules on the computer.

================================================================================
14. TROUBLESHOOTING
================================================================================

    "no root filesystem configured"    Add a filesystem with `mount: /`.
    "could not locate vendored system files"
                                       Set OCPLAY_SYSTEM to a directory with
                                       machine.lua/bios.lua/loot, or run from the
                                       project directory / a Nix install.
    Nix install refuses --upgrade      Run `nix profile upgrade ocplay`.
    Mouse does nothing                 Use --interactive and keep
                                       components.mouse: true. Clicks outside
                                       1..width/1..height are dropped.
    Screen is black / nothing drawn    Colors are 24-bit RGB, not palette
                                       indices; `colors.white` (0) is black.
                                       See section 13.
    Script sees no arguments           Confirm the config file comes before the
                                       script and args on the command line.
    Script does nothing / no error      The rc pcall swallows errors; check
                                       package.path and reproduce interactively.
    "module 'x' not found" in-game     OpenOS only searches its own package.path;
                                       keep the program self-contained or install
                                       its modules on the computer.

# Eza

**A programming language that reads like plain English, for games, apps and everyday tools.**
2D and 3D games, menus, charts, files, the web and small databases are all built in, and so is something most languages don't have: **time travel**. Every change is remembered, so you can rewind your program, or press F1 in a running game and step backwards frame by frame.

## ⬇ Download

### [**Download Eza for Windows**](https://github.com/Jayd567/Eza-Easy-A/releases/latest/download/Eza-Windows.zip)

1. **Unzip** the file anywhere.
2. **Double-click `install.bat`.** (If Windows says "Windows protected your PC", click *More info* → *Run anyway*. That appears because the program isn't signed.)
3. **Open a new terminal** in the unzipped folder and try:
   ```
   eza examples\menu.eza
   ```

That's it: nothing to compile, no other software needed. Works on 64-bit Windows 10 and 11. If you use VS Code, the installer also adds Eza support (colors, hints and error squiggles).

All versions are on the [Releases page](https://github.com/Jayd567/Eza-Easy-A/releases).

## A taste

```eza
data Enemy
    name = ""
    hp = 100

    define take_damage, amount
        change self.hp by -amount

goblin = Enemy(name="Goblin")
goblin.take_damage(30)
print("{goblin.name} has {goblin.hp} hp left")     # Goblin has 70 hp left

scores = [12, 5, 30, 8]
print(scores.filter(s -> s > 10))                  # [12, 30]

rewind goblin by 1 step
print(goblin.hp)                                   # 100 - back in time
```

A window with a working button takes six lines:

```eza
clicks = 0
gui window "counter" centered=true gap=12
    text "Clicks: 0" font_size=24
    button "Click me" then
        change clicks by 1
        change counter.children[0].label to "Clicks: {clicks}"
```

## Learn it

**[EZA_GUIDE.md](EZA_GUIDE.md)** explains the whole language step by step, from your first `print` to 3D worlds. Every feature has a working example in [`examples/`](examples).

| | |
|---|---|
| **Games** | 3D `scene`s and 2D `stage`s with sprites, tilemaps, physics, particles, sound, `spawn`/`destroy`, smooth `tween`s |
| **Menus and apps** | `gui` windows with buttons, text boxes, sliders, checkboxes, dropdowns, `style`s and charts |
| **Everyday tools** | files and folders, CSV, JSON, `fetch` from the web, `database`, dates, running other programs, command-line `args` |
| **Bigger programs** | functions, classes (`data` with methods, `from`, `super`), modules (`use "file.eza"`), tests (`eza test`) |
| **Time travel** | `rewind`, `persist` (changes that undo themselves), `mimic` (predict the future), and the F1 debugger |
| **Help with mistakes** | Errors underline the exact spot, show the values involved and **how they got that way** (from the change history), and suggest a fix. Games pause on the frame it went wrong, so you can step back and watch. `eza check` finds mistakes before running; `eza explain E003` explains any error |
| **Sharing** | `eza build game.eza` makes a `.exe` your friends can run without installing anything |

## Commands

```
eza game.eza          run a script (opens a window if it has a scene, stage or gui)
eza run tool.eza a b  run in the terminal only; a and b reach the script as `args`
eza check game.eza    find mistakes without running
eza test              run the tests in every .eza file in this folder
eza build game.eza    make dist/game/game.exe to share
eza                   type code line by line
```

## Building from source

Only needed if you want to change Eza itself. You need [Rust](https://rustup.rs) (the `x86_64-pc-windows-gnu` toolchain) and [WinLibs MinGW](https://winlibs.com) on your PATH.

```
cargo build --release --features engine
```

`powershell -ExecutionPolicy Bypass -File package\make-release.ps1` builds Eza and packs `target\Eza-Windows.zip`, the file that goes on the Releases page.

Eza is written in Rust and uses [Bevy](https://bevyengine.org) for windows, graphics and sound.

## License

[Apache 2.0](LICENSE)

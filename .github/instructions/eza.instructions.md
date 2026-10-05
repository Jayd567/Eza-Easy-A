---
name: 'Eza language'
description: 'How to read and write Eza (.eza files): a plain-English language for games, apps and tools. It is NOT Python.'
applyTo: '**/*.eza'
---

# Eza

Files ending in `.eza` are written in **Eza**, its own language (run with `eza file.eza`). It looks a bit like Python because blocks use indentation, but most words are different. **Never write Python, JavaScript or Lua syntax in a .eza file.** If unsure about something, check with `eza check file.eza` (it reports mistakes without running) or read EZA_GUIDE.md.

## The rules that matter most

| Don't write (other languages) | Write in Eza |
|---|---|
| `x += 1`, `x = x + 1` (to change an existing name) | `change x by 1` |
| `x = 10` when `x` already exists | `change x to 10` (`x = ...` only creates a new name, once per block) |
| `def f(a, b):` / `function f(a, b) {` | `define f, a, b` (or `define f(a, b)`), no colon |
| `class Enemy:` | `data Enemy` (Eza's classes) |
| `def __init__(self):` | `define setup` inside the `data` block |
| `this.hp` | `self.hp` |
| `for x in items:` / `for (...)` | `each x in items` |
| `for i in range(10):` | `each i in 10` (0 to 9), or `each i in 1 to 10` (1 to 10, both included) |
| `for k, v in d.items():` | `each k, v in d` |
| `switch` / `match x:` + `case 1:` | `match x` then indented `1 then ...`, `2, 3 then ...`, `else ...` |
| `x if cond else y`, `cond ? a : b` | an `if` / `else` block (Eza has no inline if) |
| `a ?? b`, `a \|\| b` | `a or b` (gives the first value that counts as true) |
| `x in list` / `list.includes(x)` | `x in list` (also text, dictionary keys, `x in 1 to 10`) |
| `elif` / `else:` | `else if` / `else` (no colons anywhere) |
| `None`, `null`, `True`, `False` | `none`, `true`, `false` |
| `import x` / `from x import y` | `use "x.eza"` (names reached as `x.name`), or `include "x.eza"` |
| `try:` / `except Exception as e:` | `attempt` / `handle as e` |
| `list.append(x)` | `push x to list` or `list.add(x)` (both change the list) |
| `list.pop()` | `pop list` |
| `len(x)` | `len(x)` or `x.length` (both fine) |
| `f"score {s}"` | `"score {s}"` (every string fills in `{...}`; write `{{` for a brace) |
| `while True:` with `time.sleep` in a game | `on every frame` and `wait 30 steps` / `wait 1 second` |
| `;` at line ends, `{ }` blocks | nothing; indentation makes blocks |

Comments start with `#`. `#FF0000` where a value goes is a color.

## Core syntax

```eza
score = 0                      # create a name (once)
change score by 10             # add (numbers, text, lists, vectors)
change score to 0              # replace
print("Score: {score}")        # print "Score:", score also works

if score > 10 then print("big")
if a
    print("a")
else if b
    print("b")
else
    print("neither")

each item in [1, 2, 3]
    print(item)
while score < 100
    change score by 1
    if score == 50 then break   # continue skips to the next round

define heal, target, amount = 10   # parameters after the name; amount has a default
    return target + amount
heal(5, 2)
heal(amount=2, target=5)      # by name

x, y = hero.position           # several names from one list
change x, y to [y, x]
name = saved or "Guest"        # or gives a default
if "key" in inventory then print("open")

match weapon
    "sword" then print("slash")
    "bow", "crossbow" then print("shoot")
    1 to 5 then print("a number from 1 to 5")
    else print("bonk")

double = n -> n * 2            # short function
big = nums.filter(n -> n > 3)
doubled = nums.map(n -> n * 2)

data Enemy                     # a type (class): fields with defaults, then functions
    name = ""
    hp = 100
    define setup               # runs on every new Enemy
        print("{self.name} appears")
    define take_damage, amount
        change self.hp by -amount
data Boss from Enemy           # inheritance
    define take_damage, amount
        super.take_damage(amount / 2)
goblin = Enemy(name="Goblin")  # or Enemy("Goblin", 50)
goblin.take_damage(5)
goblin.is_a(Enemy)

d = {name: "Ada", hp: 10}      # dictionary: d.name, d["hp"], d.get("x", 0), d.has("x"), d.keys()
s = stack()
push 5 to s
top = pop s

attempt
    risky()
handle as e
    print("failed:", e)

use "lib/tools.eza" as t       # module: t.some_function()
words = args                   # words after the file name: eza tool.eza a b -> ["a", "b"]
save d to "save.json"
d2 = load "save.json"
append "line" to "log.txt"

test "adds"                    # runs only with: eza test
    expect 1 + 1 == 2
```

Methods that take nothing work with or without `()`: `name.upper` = `name.upper()`. `.count` is "how many": `nums.count`, `nums.count(3)`, `nums.count(n -> n > 3)`.

## Games and windows

A script with `scene` (3D), `stage` (2D) or `gui` opens a window. Scene/GUI lines are `kind "label" key=value key=value` (no commas between settings; vectors as `0,1,0` or `[0, 1, 0]`; bare words are text; put code values in round brackets: `width=(size * 2)`).

```eza
stage gravity=-980
    tilemap "walls" tiles="tiles.png" layout="level.txt" tile_size=32
    sprite "hero" texture="hero.png" frame_size=32,32 position=0,0 physics=true animations={idle: [0], walk: [1, 2]} animation="idle" fps=10

prefab Coin
    sprite width=12 height=12 color=#FFD54F
coins = []
spawn Coin at 50, 0 into coins          # Coin.all, Coin.count, destroy Coin.all

on every frame                          # game logic, every frame
    if keyboard.held("d")
        change hero.velocity.x to 200
        change hero.animation to "walk"
on keyboard.pressed("space")            # fires once when it becomes true
    change hero.velocity.y to 500
on hero touches Coin as h, coin         # once per pair, when they start touching
    destroy coin
    trigger "coin_taken" with 1
on event "coin_taken" as n
    change score by n
on hero stops touching water
    print("dry")

route = walls.find_path(slime.position, hero.position)   # pathfinding: list of points, or none
change slime.position to slime.position.move_toward(route[0], 2)

tween door.position.y to 5 over 1 second
persist
    change hero.speed by 50
for 3 seconds                           # undoes itself afterwards
wait 30 steps                           # only inside define or on blocks

score = 0
gui window "hud" x=16 y=16
    text "Score: {score}"               # stays up to date by itself
    button "Pause" then change paused to true
```

## Terminal output, terminal apps and websites

```eza
print("Saved!", color="green", bold=true)       # styles: color, background, bold, italic, underline, dim
print(table(people))                             # list of dictionaries -> a text table
print(panel("Done", title="Backup"))
each f in progress(names, "Working")             # a progress bar as the loop runs
    print(f)
level = input("Pick one", choices=["Easy", "Hard"])   # arrow-key menu; hidden=true for passwords
page_html = """
    <h1>Hello {name}</h1>
    """                                          # text over several lines; { followed by a space is a plain brace

gui window "app" terminal=true centered=true title="My app"   # the same gui, drawn in the terminal
    text "Count: {count}"                        # {values} in labels update by themselves
    button "Add one" then change count by 1
    button "Quit" then quit

serve port=8000 folder="public"                  # web server; open http://localhost:8000
    page "/" then return "<h1>Hi</h1>"           # text starting with < is HTML
    page "/api/scores" then return scores        # lists and dictionaries go out as JSON
    page "/hello/{who}" then return "Hi {who}"   # {who} is text
    page "/add" method="POST"
        scores.add(request.data)                 # request: method, path, query, form, data, body, headers
        change response.status to 201            # response: status, type, headers
```

`a.touches(b)` checks overlap right now (inside `if`). `raycast(from=p, direction=[1, 0], distance=200)`. `play "coin.wav"`, `emit 30 from sparks at hero.position`, `go to "level2"`, `rewind hero by 60 steps`.

## Before finishing

Run `eza check file.eza` and fix what it reports. It catches misspelled names, wrong argument counts, `x = ...` used twice, and the wrong kind of value (like `score - "5"` or `name.uper()`).

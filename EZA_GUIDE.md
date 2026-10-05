# The Eza Beginner's Guide

This guide covers every operation and piece of syntax in the Eza language, from your first `print` to 3D scenes, 2D games, physics, particles, sound, time travel and menus. Read it top to bottom the first time, then use the [cheat sheet](#47-cheat-sheet) at the end as a quick reference.

## Contents

1. [Running Eza](#1-running-eza)
2. [The shape of a program](#2-the-shape-of-a-program)
3. [Values and types](#3-values-and-types)
4. [Variables: creating and changing](#4-variables-creating-and-changing)
5. [Operators](#5-operators)
6. [Text (strings)](#6-text-strings)
7. [Numbers](#7-numbers)
8. [Lists](#8-lists)
9. [Stacks and queues](#9-stacks-and-queues)
10. [Dictionaries](#10-dictionaries)
11. [Vectors (positions and directions)](#11-vectors-positions-and-directions)
12. [Colors](#12-colors)
13. [Making decisions: `if`](#13-making-decisions-if)
14. [Loops: `each` and `while`](#14-loops-each-and-while)
15. [Functions: `define`](#15-functions-define)
16. [Your own object types: `data`](#16-your-own-object-types-data)
17. [Working with lists: `filter`, `map`, `group_by`](#17-working-with-lists-filter-map-group_by)
18. [Handling errors: `attempt` / `handle`](#18-handling-errors-attempt--handle)
19. [Multiple files, settings and shared values](#19-multiple-files-settings-and-shared-values)
20. [Modules: `use`](#20-modules-use)
21. [Files and pictures: `load`, `save`, `append`](#21-files-and-pictures-load-save-append)
22. [Dates and times](#22-dates-and-times)
23. [The web: `fetch`](#23-the-web-fetch)
24. [Saving records: `database`](#24-saving-records-database)
25. [Files and folders](#25-files-and-folders)
26. [Running other programs: `run`](#26-running-other-programs-run)
27. [Command-line arguments: `args`](#27-command-line-arguments-args)
28. [Built-in functions](#28-built-in-functions)
29. [Time: frames and `tick`](#29-time-frames-and-tick)
30. [Reacting to things: `on`](#30-reacting-to-things-on)
31. [Pausing: `wait`](#31-pausing-wait)
32. [Temporary effects: `persist`](#32-temporary-effects-persist)
33. [Smooth animation: `tween`](#33-smooth-animation-tween)
34. [Time travel: `rewind`](#34-time-travel-rewind)
35. [Predicting the future: `mimic`](#35-predicting-the-future-mimic)
36. [3D worlds: `scene`](#36-3d-worlds-scene)
37. [2D worlds: `stage`](#37-2d-worlds-stage)
38. [Particles: sparks, smoke, snow](#38-particles-sparks-smoke-snow)
39. [Templates and live objects: `prefab`, `spawn`, `destroy`](#39-templates-and-live-objects-prefab-spawn-destroy)
40. [Sound and music: `play`, `stop`](#40-sound-and-music-play-stop)
41. [Menus and HUDs: `gui`](#41-menus-and-huds-gui)
42. [Styles: making menus look good](#42-styles-making-menus-look-good)
43. [Charts](#43-charts)
44. [Switching scripts: `go to`](#44-switching-scripts-go-to)
45. [Finding and fixing mistakes](#45-finding-and-fixing-mistakes)
46. [Sharing your program: `eza build`](#46-sharing-your-program-eza-build)
47. [Cheat sheet](#47-cheat-sheet)
48. [Common errors and what they mean](#48-common-errors-and-what-they-mean)

---

## 1. Running Eza

Eza scripts are plain text files ending in `.eza`. Run them from a terminal:

| Command | What it does |
|---|---|
| `eza` | Starts the interactive prompt (type code line by line) |
| `eza game.eza` | Runs a script. If it has a `scene`, `stage` or `gui`, a window opens |
| `eza run game.eza` | Runs a script in the terminal only, never opens a window |
| `eza tool.eza a b` | Runs a script and hands it `a` and `b` in [`args`](#27-command-line-arguments-args) |
| `eza check game.eza` | Finds mistakes without running anything (prints `OK`). `eza check` on its own checks every file in the folder. See [finding mistakes](#45-finding-and-fixing-mistakes) |
| `eza explain E003` | Explains an error code in detail (`eza explain` lists them all) |
| `eza test` | Runs the `test` blocks in every `.eza` file in this folder (or `eza test game.eza`) |
| `eza build game.eza` | Makes `dist/game/game.exe` to share. See [sharing](#46-sharing-your-program-eza-build) |
| `eza --version` | Shows the version (`-v` also works) |
| `eza help` | Shows the list of commands (`--help` and `-h` also work) |

> The download from GitHub already includes everything, windows too. Only if you build Eza yourself from the source: use `cargo build --release --features engine`, or windows won't open.

### The interactive prompt

Type `eza` on its own to get the `eza>` prompt. Each line runs as soon as you press Enter. When you start a block (like `if` or `each`), the prompt changes to `...` and waits for more lines. **Finish a block with an empty line.** Type `exit` to quit.

```
eza> score = 10
eza> if score > 5
...      print("big score")
...
big score
```

### VS Code

The installer adds Eza to VS Code: colors, hover help, snippets, a Run command, and red or yellow underlines under mistakes while you type.

It also teaches **VS Code's AI chat** (Copilot) what Eza looks like, so it stops writing Python into `.eza` files. That's a short reference file, `eza.instructions.md`, which VS Code hands to the AI whenever a `.eza` file is involved. It's in your VS Code settings folder (`%APPDATA%\Code\User\prompts`), so it works in every folder you open. To share it with everyone working on a project, copy it into the project as `.github/instructions/eza.instructions.md`.

This helps the **chat** and agent. The grey suggestions that pop up while you type (inline completions) can't read instruction files, so they may still guess wrong. They improve when an `.eza` file or this guide is open in another tab. If they get in the way, turn them off for Eza only: click the Copilot icon in the status bar while a `.eza` file is open and choose to disable completions for `eza`.

---

## 2. The shape of a program

### Statements run top to bottom, one per line

```eza
print("Hello!")
print("This runs second.")
```

### Comments start with `#`

```eza
# This whole line is a comment
score = 10   # a comment after code
```

`#` is also how colors are written (`#FF0000`). Eza tells them apart by where they are: a line that starts with `#` is always a comment, and so is a `#` after a finished value (like the `10` above). A `#` where a value goes, such as after `=` or `to`, is a color.

### Blocks use indentation

Lines that belong to an `if`, a loop, a function and so on are **indented** under it, like in Python. Use 4 spaces (a tab counts as 4 spaces). All lines in the same block must line up exactly.

```eza
if score > 5
    print("big score")      # inside the if
    print("well done")      # still inside
print("always runs")        # back outside
```

### One-line blocks with `then`

When a block is a single statement, you can put it on the same line after `then`:

```eza
if score > 5 then print("big score")
```

`then` is also allowed (and ignored) before an indented block, so both of these are fine:

```eza
each fruit in fruits then
    print(fruit)

each fruit in fruits
    print(fruit)
```

### Long lines

A line can continue onto the next line while you're inside `( )` or `[ ]`:

```eza
colors = [
    "red",
    "green",
    "blue",
]
```

---

## 3. Values and types

Every value in Eza has a type. `type(x)` (or `x.type`) tells you which.

| Type | Examples | `type()` says |
|---|---|---|
| Number | `5`, `3.14`, `-2` | `number` |
| Text | `"hello"`, `'hi'` | `text` |
| True/false | `true`, `false` | `bool` |
| Nothing | `none` | `none` |
| List | `[1, 2, 3]`, `["a", "b"]` | `list` |
| Color | `#FF0000`, `#F00` | `color` |
| Object | `Item(name="Sword")`, `player` | the object's kind, e.g. `Item` |
| Dictionary | `{name: "Ada", hp: 10}` | `dict` |
| Stack / queue | `stack()`, `queue()` | `stack` / `queue` |
| Picture | `load "hero.png"` | `image` |
| Date | `now()`, `date("2026-10-04")` | `date` |
| Database | `database("notes.db")` | `database` |
| Function | `print`, your own `define`s | `function` |

### Truthiness

`if`, `while`, `and`, `or` and `not` treat values as true or false:

- **False:** `false`, `none`, `0`, empty text `""`, empty list `[]`
- **True:** everything else

```eza
name = ""
if name
    print("has a name")
else
    print("no name")        # this one runs: empty text counts as false
```

---

## 4. Variables: creating and changing

This is the most important rule in Eza: **you create a variable with `=` and update it with `change`.**

### Create with `=`

```eza
score = 0
name = "Ada"
```

You can only create a name once per block. Writing `score = 5` again in the same block is an error:

```
'score' already exists - use 'change score to ...' to update it
```

### Several names at once

Names separated by commas take the items of a list, one each:

```eza
x, y = hero.position          # x is position[0], y is position[1]
name, hp = ["Ada", 30]
change x, y to [y, x]         # swap them
```

There must be exactly one item per name. A function that returns several values (`return a, b`) gives a list, so `low, high = min_max(scores)` works too.

### Update with `change ... to` (replace)

```eza
change score to 100
change name to "Grace"
```

### Update with `change ... by` (add)

`by` adds to the current value. What "add" means depends on the type:

| Current value | `change x by ...` | Result |
|---|---|---|
| Number `10` | `change x by 5` | `15` |
| Number `10` | `change x by -3` | `7` (subtract with a negative) |
| Text `"ab"` | `change x by "c"` | `"abc"` (appends) |
| Text `"hp: "` | `change x by 5` | `"hp: 5"` (anything appended as text) |
| List `[1, 2]` | `change x by 3` | `[1, 2, 3]` (appends the item) |
| Vector `[1, 1, 1]` | `change x by [0, 2, 0]` | `[1, 3, 1]` (adds each part) |

> A list of numbers plus a list of numbers **of the same length** is treated as vector math (each part added). Anything else gets appended as one item.

### Changing parts of things

`change` works on properties, list items and vector parts:

```eza
change player.health by -10        # a property
change scores[0] to 99             # a list item
change player.position.y by 2      # one part of a vector
change enemy.mood to "angry"       # 'to' can create a new property
```

`change ... to` can add a property that doesn't exist yet. `change ... by` can't, because there's nothing to add to.

### Scope: where names live

- A variable made inside a block (an `if`, a loop, a function) only exists inside that block.
- Inner blocks can read **and `change`** variables from outer blocks.

```eza
total = 0
each n in [1, 2, 3]
    doubled = n * 2           # only exists inside this loop
    change total by doubled   # changes the outer variable
print(total)                  # 12
```

### Every change is remembered

Each `change` is a step on a timeline, and Eza remembers the last 1000 steps of every variable. That's what powers [`rewind`](#34-time-travel-rewind).

---

## 5. Operators

### Math

| Operator | Meaning | Example | Result |
|---|---|---|---|
| `+` | add | `7 + 2` | `9` |
| `-` | subtract | `7 - 2` | `5` |
| `*` | multiply | `7 * 2` | `14` |
| `/` | divide | `7 / 2` | `3.5` |
| `%` | remainder | `7 % 3` | `1` |
| `-x` | negative | `-score` | |

- `%` always gives a result with the same sign as the right side: `-7 % 3` is `2`.
- Dividing by zero (`/` or `%`) is an error.

### Comparison (give `true` or `false`)

| Operator | Meaning |
|---|---|
| `==` | equal |
| `!=` | not equal |
| `<` `>` | less / greater than |
| `<=` `>=` | less-or-equal / greater-or-equal |

- `==` compares contents, so `[1, 2] == [1, 2]` is `true`, and two objects are equal if all their properties match.
- `<`, `>`, `<=` and `>=` work on two numbers or two texts (texts compare alphabetically: `"apple" < "banana"` is `true`).

### Logic

| Operator | Meaning | Example |
|---|---|---|
| `and` | both true | `x > 0 and x < 10` |
| `or` | at least one true | `key == "a" or key == "left"` |
| `not` | flip | `not game_over` |

They stop early: in `a and b`, `b` isn't even looked at if `a` is false.

`or` gives back the first value that counts as true (or the last one), which makes **defaults** easy:

```eza
name = saved_name or "Guest"      # "Guest" if saved_name is none or ""
```

(`and` works the same way: it gives back the first value that counts as false, or the last one.) In an `if`, this behaves exactly like true and false.

### Is it in there? `in`

```eza
if "key" in inventory             # is it in the list?
if "@" in email                   # is this text inside the text?
if "theme" in settings            # does the dictionary have this key?
if hp in 1 to 50                  # is the number between these two (both included)?
```

`not "key" in inventory` is the opposite.

### Ranges: `to`

`a to b` is every whole number from `a` to `b`, **both included**:

```eza
print(1 to 5)                     # [1, 2, 3, 4, 5]
each level in 1 to 10
    print("Level {level}")
each n in 3 to 1                  # counts down: 3, 2, 1
```

With `in`, a range checks **any** number between the two, not only whole ones: `12.5 in 1 to 50` is `true`.

### Bits (bitwise operators)

These work on the 0s and 1s inside whole numbers. You'll mostly need them for flags, colors packed into numbers, or puzzle and emulator projects.

| Operator | Meaning | Example | Result |
|---|---|---|---|
| `&` | AND: bits on in both | `0b1100 & 0b1010` | `8` (`0b1000`) |
| `\|` | OR: bits on in either | `0b1100 \| 0b1010` | `14` (`0b1110`) |
| `^` | XOR: bits on in one but not both | `0b1100 ^ 0b1010` | `6` (`0b0110`) |
| `<<` | shift left (multiply by 2 each step) | `1 << 4` | `16` |
| `>>` | shift right (divide by 2 each step) | `256 >> 2` | `64` |
| `~` | flip every bit | `~0` | `-1` |

They need whole numbers. Each one also has a method version, listed under [numbers](#7-numbers).

### Order of operations (highest first)

1. `( )` grouping, `.property`, `[index]`, `call()`
2. `-x` (negative), `~x`
3. `*` `/` `%`
4. `+` `-`
5. `<<` `>>`
6. `&`
7. `^`
8. `|`
9. `to`
10. `==` `!=` `<` `>` `<=` `>=` `in`
11. `not`
12. `and`
13. `or`

Use parentheses whenever you're unsure: `(a + b) * 2`.

---

## 6. Text (strings)

### Writing text

Use double `"..."` or single `'...'` quotes. "Smart quotes" pasted from a word processor also work. Text must end on the same line it starts.

| Escape | Means |
|---|---|
| `\n` | new line |
| `\t` | tab |
| `\"` `\'` `\\` | a literal quote or backslash |

### Joining text

`+` joins text with anything; the other side is turned into text automatically:

```eza
print("Score: " + 10)       # Score: 10
print(10 + " points")       # 10 points
```

`*` repeats text: `"ha" * 3` is `"hahaha"`.

### Filling in values with `{ }`

Put any expression inside `{ }` in a piece of text:

```eza
name = "eza"
score = 42
print("Hello {name.upper}, you have {score} points, double is {score * 2}")
# Hello EZA, you have 42 points, double is 84
```

To write a real brace, double it: `"{{like this}}"` prints `{like this}`.

### Getting characters

```eza
word = "hello"
print(word[0])     # h        (counting starts at 0)
print(word[-1])    # o        (negative counts from the end)
print(len(word))   # 5
```

### Text methods

Methods are called with a dot. **When a method takes no arguments, the `()` is optional**: `name.upper` and `name.upper()` are the same.

| Method | What it does | Example | Result |
|---|---|---|---|
| `.upper` | all capitals | `"hi".upper` | `"HI"` |
| `.lower` | all lowercase | `"HI".lower` | `"hi"` |
| `.capitalize` | first letter capital, rest lowercase | `"hELLO".capitalize` | `"Hello"` |
| `.length` | number of characters | `"hello".length` | `5` |
| `.trim` | remove spaces at both ends | `"  hi  ".trim` | `"hi"` |
| `.trim_left` | remove spaces at the start | `"  hi".trim_left` | `"hi"` |
| `.trim_right` | remove spaces at the end | `"hi  ".trim_right` | `"hi"` |
| `.reverse` | backwards | `"abc".reverse` | `"cba"` |
| `.split(sep)` | break into a list | `"a,b,c".split(",")` | `["a", "b", "c"]` |
| `.split` | split on spaces | `"a b".split` | `["a", "b"]` |
| `.split("")` | split into characters | `"abc".split("")` | `["a", "b", "c"]` |
| `.replace(a, b)` | swap every `a` for `b` | `"cats".replace("c", "b")` | `"bats"` |
| `.contains(x)` | is `x` inside? | `"eza".contains("z")` | `true` |
| `.starts_with(x)` | begins with `x`? | `"eza".starts_with("e")` | `true` |
| `.ends_with(x)` | ends with `x`? | `"eza".ends_with("a")` | `true` |
| `.repeat(n)` | repeat `n` times | `"ab".repeat(2)` | `"abab"` |
| `.lines` | list of lines | `"a\nb".lines` | `["a", "b"]` |
| `.words` | list of words (any spaces between) | `"a  b c".words` | `["a", "b", "c"]` |
| `.pad_left(n, c)` | fill on the left to `n` characters (`c` defaults to a space) | `"7".pad_left(3, "0")` | `"007"` |
| `.pad_right(n, c)` | fill on the right | `"ab".pad_right(4, ".")` | `"ab.."` |

Methods can be chained: `name.trim.capitalize`.

### Patterns: checking and finding text

A **pattern** describes what text should look like, such as "some digits, a dash, more digits". They're the same patterns (called regular expressions) used in most programming languages.

| Method | What it does | Example | Result |
|---|---|---|---|
| `.matches(p)` | does the **whole** text fit the pattern? | `"ada@mail.com".matches("[a-z]+@[a-z]+\.[a-z]+")` | `true` |
| `.find_all(p)` | every piece that fits | `"a1b22".find_all("[0-9]+")` | `["1", "22"]` |
| `.replace_pattern(p, with)` | replace every piece that fits | `"a1b22".replace_pattern("[0-9]+", "#")` | `"a#b#"` |

The most useful pattern pieces:

| Piece | Means |
|---|---|
| `[0-9]` `[a-z]` `[A-Za-z]` | one character from a range |
| `+` | the piece before, one or more times |
| `*` | the piece before, any number of times (even none) |
| `?` | the piece before is optional |
| `{{3}}` | the piece before exactly 3 times (see the note below) |
| `.` | any character; `\.` means a real dot |
| `\s` / `\d` / `\w` | a space / a digit / a letter, digit or `_` |

> **Note:** inside Eza text, `{ }` fills in values, so a pattern's repeat count must be written with double braces: `"[0-9]{{3}}"` means "exactly 3 digits".

---

## 7. Numbers

Numbers can be whole (`5`) or have a decimal point (`2.5`). Write a leading zero for small decimals: `0.5`, not `.5`. Numbers print without a trailing `.0`, so `5.0` shows as `5`.

Whole numbers can also be written in other bases, and `_` can split long numbers to make them easier to read:

| Written | Base | Value |
|---|---|---|
| `0b1101` | binary | `13` |
| `0xFF` | hexadecimal | `255` |
| `0o17` | octal | `15` |
| `1_000_000` | normal | `1000000` |

### Number methods

As with text, `()` is optional when there are no arguments.

| Method | What it does | Example | Result |
|---|---|---|---|
| `.abs` | drop the minus sign | `(-7).abs` | `7` |
| `.floor` | round down | `7.6.floor` | `7` |
| `.ceil` | round up | `7.2.ceil` | `8` |
| `.round` | round to nearest | `7.5.round` | `8` |
| `.sqrt` | square root | `16.sqrt` | `4` |
| `.pow(n)` | to the power of `n` | `2.pow(10)` | `1024` |
| `.clamp(lo, hi)` | keep within a range | `150.clamp(0, 100)` | `100` |
| `.min(n)` | the smaller one | `3.min(1)` | `1` |
| `.max(n)` | the bigger one | `3.max(9)` | `9` |
| `.format(d)` | text with exactly `d` decimals | `3.14159.format(2)` | `"3.14"` |
| `.commas` | text with thousands separated | `1234567.commas` | `"1,234,567"` |

For negative numbers, use parentheses: `(-7).abs`. Taking the square root of a negative number is an error.

### Bit methods

The same as the [bitwise operators](#5-operators), written as methods (whole numbers only):

| Method | Same as | Example | Result |
|---|---|---|---|
| `.bit_and(m)` | `&` | `12.bit_and(10)` | `8` |
| `.bit_or(m)` | `\|` | `12.bit_or(10)` | `14` |
| `.bit_xor(m)` | `^` | `12.bit_xor(10)` | `6` |
| `.bit_not` | `~` | `0.bit_not` | `-1` |
| `.shift_left(n)` | `<<` | `1.shift_left(4)` | `16` |
| `.shift_right(n)` | `>>` | `256.shift_right(2)` | `64` |
| `.bit(i)` | | is bit number `i` on? `13.bit(2)` | `true` |
| `.to_binary` | | `13.to_binary` | `"1101"` |
| `.to_hex` | | `255.to_hex` | `"FF"` |

### Math functions

| Function | What it does |
|---|---|
| `sin(x)` `cos(x)` `tan(x)` | trigonometry (angles in **radians**) |
| `asin(x)` `acos(x)` `atan(x)` | inverse trigonometry (answer in radians) |
| `atan2(y, x)` | angle of the point `(x, y)` in radians |
| `radians(deg)` | degrees to radians: `radians(180)` is `pi` |
| `degrees(rad)` | radians to degrees |
| `pi` | the number 3.14159... |

```eza
print(sin(pi / 2))               # 1
print(degrees(atan2(1, 1)))      # 45
```

---

## 8. Lists

A list holds several values in order, inside `[ ]`, separated by commas. Items can be any type, even mixed.

```eza
fruits = ["apple", "banana", "kiwi"]
empty = []
mixed = [1, "two", true, #FF0000]
```

### Reading items

```eza
print(fruits[0])      # apple    (first item is 0)
print(fruits[-1])     # kiwi     (last item)
print(len(fruits))    # 3
```

Using an index that doesn't exist is an error.

### Changing a list

```eza
change fruits[0] to "mango"     # replace one item
change fruits by "fig"          # add to the end
```

### List methods

> **Important:** list methods **never change the original list**. They give you a *new* list. To keep the result, store it: `change nums to nums.sort`.

| Method | What it gives back | Example (`nums = [5, 3, 9, 1]`) | Result |
|---|---|---|---|
| `.length` | number of items | `nums.length` | `4` |
| `.first` | first item (`none` if empty) | `nums.first` | `5` |
| `.last` | last item (`none` if empty) | `nums.last` | `1` |
| `.sum` | total of all numbers | `nums.sum` | `18` |
| `.min` | smallest | `nums.min` | `1` |
| `.max` | largest | `nums.max` | `9` |
| `.sort` | sorted copy | `nums.sort` | `[1, 3, 5, 9]` |
| `.sort_by(f)` | sorted by what function `f` returns | `["kiwi", "fig"].sort_by(len)` | `["fig", "kiwi"]` |
| `.reverse` | reversed copy | `nums.reverse` | `[1, 9, 3, 5]` |
| `.contains(x)` | is `x` in the list? | `nums.contains(9)` | `true` |
| `.add(x)` | adds `x` at the end (the list changes) | `nums.add(7)` | `[5, 3, 9, 1, 7]` |
| `.remove(x)` | takes the first `x` out (the list changes) | `nums.remove(3)` | `[5, 9, 1]` |
| `.count` | how many items (the same as `len(nums)`) | `nums.count` | `4` |
| `.join(sep)` | all items as one text | `nums.join("-")` | `"5-3-9-1"` |
| `.join` | joined with nothing between | `["a", "b"].join` | `"ab"` |
| `.repeat(n)` | the list repeated `n` times | `[0].repeat(3)` | `[0, 0, 0]` |

`.sort`, `.min` and `.max` need all numbers or all text.

Lists also work with `push` and `pop` (see [stacks](#9-stacks-and-queues)): `push 4 to nums` adds to the end, `pop nums` takes the last item off and gives it back.

> **Big lists are fast too.** Lists (and dictionaries) with more than 256 items are changed in place instead of being copied for [`rewind`](#34-time-travel-rewind), so things like `memory = [0].repeat(30000)` stay quick. The catch: those big ones can't be rewound.

### Joining lists

`+` puts two lists together:

```eza
print([1, "a"] + [2])     # [1, "a", 2]
```

> Exception: two lists of **numbers with the same length** are treated as vectors, so `[1, 2] + [3, 4]` gives `[4, 6]`, not `[1, 2, 3, 4]`. See the next section.

---

## 9. Stacks and queues

A **stack** is a pile: the last thing you put on is the first thing you take off. A **queue** is a line: the first thing in is the first thing out.

```eza
s = stack()               # empty
s2 = stack([1, 2, 3])     # with starting items (3 is on top)
q = queue(["first", "second"])
```

### Adding and taking: `push` and `pop`

```eza
push 5 to s               # put 5 on top
push 10 to s
top = pop s               # takes 10 off and gives it back

push "third" to q         # join the end of the line
next = pop q              # "first" - queues give the OLDEST item
```

- `pop` on something empty is an error. Check `.empty` first.
- Plain lists work with `push` and `pop` too. They behave like a stack (`pop` takes the last item).

### Looking without taking

| Property | Meaning |
|---|---|
| `.peek` | the item `pop` would give next, without removing it |
| `.length` | how many items |
| `.empty` | `true` when there's nothing left |

`each item in s` loops over the items, oldest first.

```eza
# undo history with a stack
moves = stack()
push "left" to moves
push "jump" to moves
print("undo:", pop moves)      # undo: jump
```

---

## 10. Dictionaries

A **dictionary** stores values under names (called **keys**), inside `{ }`:

```eza
settings = {theme: "dark", volume: 7, tags: ["a", "b"]}
```

Keys can be written as plain names or as text (`{"high score": 10}`). `type(settings)` is `"dict"`.

### Reading and changing

```eza
print(settings.theme)            # dark       (dot works for simple keys)
print(settings["volume"])        # 7          (brackets work for any key)
change settings.volume by 1
change settings["lang"] to "en"  # adds a new key
```

### Dictionary methods

| Method | What it does | Example | Result |
|---|---|---|---|
| `.keys` | list of the keys | `settings.keys` | `["theme", "volume", ...]` |
| `.values` | list of the values | `{a: 1, b: 2}.values` | `[1, 2]` |
| `.length` | number of keys | `{a: 1}.length` | `1` |
| `.has(key)` | is the key there? | `settings.has("lang")` | `true` |
| `.get(key, default)` | the value, or the default if the key is missing | `settings.get("size", 12)` | `12` |
| `.remove(key)` | takes the key out | `settings.remove("tags")` | |

Reading a key that doesn't exist with `.` or `[ ]` is an error. Use `.get` when a key might be missing.

**Keys come first.** `settings.theme` reads the key `theme`. If a dictionary has a key with the same name as a method (say a key called `keys` or `length`, which happens with data loaded from files), the dot reads the key. Add brackets to use the method instead: `settings.keys()` is always the list of keys.

### Looping

`each` goes through the keys. With two names, it gives each key **and** its value:

```eza
each key in settings
    print(key)

each key, value in settings
    print(key, "=", value)
```

Dictionaries are also what `.json` files turn into. See [files](#21-files-and-pictures-load-save-append).

---

## 11. Vectors (positions and directions)

A **vector** is just a list of numbers, like a position `[x, y, z]`. Eza gives these lists extra powers.

### Reading parts

`.x`, `.y`, `.z` and `.w` read items 0, 1, 2 and 3:

```eza
pos = [4, 0, 7]
print(pos.x, pos.z)              # 4 7
change pos.y by 2                # pos is now [4, 2, 7]
```

### Vector math

| Operation | Example | Result |
|---|---|---|
| add | `[1, 2, 3] + [4, 5, 6]` | `[5, 7, 9]` |
| subtract | `[4, 5, 6] - [1, 2, 3]` | `[3, 3, 3]` |
| scale | `[1, 2, 3] * 2` or `2 * [1, 2, 3]` | `[2, 4, 6]` |
| divide | `[4, 5, 6] / 2` | `[2, 2.5, 3]` |
| flip | `-[1, 2, 3]` | `[-1, -2, -3]` |

Adding and subtracting need two vectors of the same length.

### Vector methods and functions

| Name | What it does | Example | Result |
|---|---|---|---|
| `.magnitude` | length of the vector | `[3, 4].magnitude` | `5` |
| `.normalize` | same direction, length 1 | `[0, 0, 5].normalize` | `[0, 0, 1]` |
| `.dot(v)` | dot product | `[1, 2, 3].dot([4, 5, 6])` | `32` |
| `.cross(v)` | cross product (3D only) | `[1, 0, 0].cross([0, 1, 0])` | `[0, 0, 1]` |
| `distance(a, b)` | distance between two points | `distance([0, 0, 0], [3, 4, 0])` | `5` |
| `lerp(a, b, t)` | point `t` of the way from `a` to `b` | `lerp([0, 0, 0], [10, 20, 0], 0.5)` | `[5, 10, 0]` |
| `.move_toward(target, step)` | at most `step` closer to `target`, never past it | `[0, 0].move_toward([10, 0], 3)` | `[3, 0]` |

`lerp` also works on plain numbers (`lerp(0, 10, 0.25)` is `2.5`) and on colors.

For 2D vectors there's also `.angle` (direction in degrees) and `.rotate(degrees)`. See [2D vectors](#2d-vectors).

A common pattern is getting the direction from one thing to another:

```eza
direction = (enemy.position - player.position).normalize
```

---

## 12. Colors

Write a color as `#` followed by hex digits, in any of these forms:

| Form | Example | Meaning |
|---|---|---|
| `#RGB` | `#F00` | red (short form) |
| `#RGBA` | `#F008` | red, half see-through |
| `#RRGGBB` | `#FF0000` | red |
| `#RRGGBBAA` | `#FF000080` | red, half see-through |

### Color methods

| Method | What it does | Example |
|---|---|---|
| `.r` `.g` `.b` | red/green/blue amount, 0 to 255 | `#FF8000.g` is `128` |
| `.a` | opacity, 0 (invisible) to 1 (solid) | `#FF000080.a` is about `0.5` |
| `.hex` | the color as text | `#F00.hex` is `"#FF0000"` |
| `.lighten(t)` | move toward white by `t` (0 to 1) | `#FF0000.lighten(0.5)` is `#FF8080` |
| `.darken(t)` | move toward black by `t` (0 to 1) | `#FF0000.darken(0.5)` is `#800000` |
| `.mix(other, t)` | blend with another color (`t` defaults to 0.5) | `#FF0000.mix(#0000FF)` is `#800080` |
| `.invert` | opposite color | `#FF0000.invert` is `#00FFFF` |
| `.saturate(t)` | more vivid (negative `t` = duller) | `#808080.saturate(0.5)` |

Colors can be compared with `==` and animated with [`tween`](#33-smooth-animation-tween).

---

## 13. Making decisions: `if`

```eza
if score >= 100
    print("You win!")
else if score >= 50
    print("Halfway there")
else
    print("Keep going")
```

- `else if` and `else` are optional, and each goes on its own line, lined up with the `if`.
- Only the first matching branch runs.
- Each branch can be a one-liner with `then`:

```eza
if lives == 0 then print("Game over")
```

### Choosing between many: `match`

When one value decides between many choices, `match` is shorter than a long `if` / `else if` chain:

```eza
match weapon
    "sword" then change damage to 10
    "bow", "crossbow" then change damage to 6
    "staff"
        change damage to 4
        change mana by -5
    else change damage to 1
```

- Each line under `match` is a choice: one value, or several separated by commas. Then `then` and one statement, or an indented block, just like `if`.
- A range matches any number between the two: `1 to 30 then print("hurt")`.
- `else` (optional, and last) runs when nothing else matched.
- Only the first matching choice runs, and the value after `match` is worked out just once.

```eza
match hp
    0 then print("defeated")
    1 to 30 then print("badly hurt")
    31 to 99 then print("hurt")
    else print("full health")
```

---

## 14. Loops: `each` and `while`

### `each`: do something for every item

```eza
each fruit in ["apple", "banana"]
    print(fruit)
```

What you can loop over:

| Loop over | Example | The variable becomes |
|---|---|---|
| a list | `each f in fruits` | each item in turn |
| a number `n` | `each i in 5` | `0, 1, 2, 3, 4` |
| text | `each ch in "abc"` | `"a"`, `"b"`, `"c"` |
| a range | `each i in 2 to 5` | `2, 3, 4, 5` (see [ranges](#ranges-to)) |
| a dictionary | `each key in settings` | each key |
| a stack or queue | `each item in s` | each item, oldest first |

**Changing the loop variable changes the list.** When you loop over a list stored in a variable, `change`-ing the loop variable writes back into the list:

```eza
fruits = ["apple", "kiwi"]
each fruit in fruits
    change fruit to fruit.upper
print(fruits)              # ["APPLE", "KIWI"]
```

This works for objects in a list too: `each d in dots then change d.x by 1` moves every dot.

**Two names unpack each item:** `each x, y in [[1, 2], [3, 4]]` gives `x = 1, y = 2`, then `x = 3, y = 4`. On a dictionary, two names give each key and its value: `each name, score in high_scores`.

### `while`: repeat as long as something is true

```eza
count = 3
while count > 0
    print(count)
    change count by -1
print("Liftoff!")
```

Make sure the condition eventually becomes false, or the loop runs forever.

### `break` and `continue`

- `break` leaves the loop right away.
- `continue` skips the rest of this round and goes to the next one.

```eza
each n in 10
    if n % 2 == 0
        continue      # skip even numbers
    if n > 7
        break         # stop completely
    print(n)          # 1, 3, 5, 7
```

---

## 15. Functions: `define`

A function is a named, reusable block of code.

### Making and calling functions

```eza
define greet, name
    print("Hello, " + name)

greet("Ada")             # Hello, Ada
```

- The name comes first, then the parameter names separated by commas. The comma after the function name is optional: `define greet name` also works.
- If you like brackets (like when calling it), those work too: `define greet(name)`, `define heal(target, amount)`.
- A function with no parameters is just `define say_hi`.
- **Calling always uses parentheses**, even with no arguments: `say_hi()`.

### Default values

Give a parameter a value with `=`, and calls can leave it out:

```eza
define greet, name, greeting = "Hello"
    print("{greeting}, {name}!")

greet("Ada")                  # Hello, Ada!
greet("Bo", "Hi")             # Hi, Bo!
greet("Cy", greeting="Hey")   # Hey, Cy!
```

- Parameters with a default go after the ones without one.
- The default is worked out on each call, and can use the parameters before it: `define area, w, h = w`.

### Returning values

```eza
define calculate_damage, base, multiplier
    return base * multiplier

hit = calculate_damage(50, 1.5)     # 75
```

- `return` stops the function and hands back a value.
- A function with no `return` (or a bare `return`) gives back `none`.
- **Several values:** `return 100, 0, 450` gives back the list `[100, 0, 450]`.

### Named arguments

You can pass arguments by name, in any order:

```eza
print(calculate_damage(multiplier=2, base=10))    # 20
```

Passing too many arguments, or leaving one out, is an error.

### Functions can change objects and lists you pass in

When you pass an object or a list stored in a variable, the function works on the real thing, not a copy:

```eza
define push_back, thing
    change thing.position.x by -5

push_back(enemy)       # enemy really moves
```

```eza
define add_bonus, scores
    change scores by 100

add_bonus(my_scores)   # my_scores really gets the 100
```

Numbers and text are copied, so changing them inside a function doesn't affect the caller.

### Functions stored in variables

You can create a function and store it in a variable in one line:

```eza
vortex = define entity
    change entity.position.x by 2

vortex(enemy)
```

Functions are values: you can pass them to other functions, like `fruits.sort_by(len)`.

<a id="short-functions-"></a>
### Short functions: `->`

When a function only works out one value, write it in one go with `->`:

```eza
double = n -> n * 2
add = (a, b) -> a + b
print(double(4), add(2, 3))     # 8 5
```

The names before `->` are the parameters; the expression after it is what the function gives back. Short functions are made for [list tools](#17-working-with-lists-filter-map-group_by) like `scores.filter(s -> s > 10)`.

### Functions remember where they were made

A function can read and change variables from the place it was defined:

```eza
clicks = 0
define click
    change clicks by 1
click()
click()
print(clicks)      # 2
```

### Recursion

Functions can call themselves. If one goes more than 2000 calls deep, Eza stops with "too much recursion".

---

## 16. Your own object types: `data`

`data` describes a kind of object and the default value of each of its properties.

```eza
data Item
    name = ""
    weight = 0.0
    equippable = false
```

The defaults can be any expression. Use the type name like a function to make a new object:

```eza
sword = Item(name="Sword", weight=3.5, equippable=true)   # by name
rock  = Item("Rock", 10)                                   # in order
blank = Item()                                             # all defaults

print(sword.name)        # Sword
print(sword)             # Item(name: "Sword", weight: 3.5, equippable: true)
```

- Properties you don't give keep their defaults.
- Naming a property the type doesn't have is an error.
- You can still add new properties later with `change sword.rarity to "epic"`.
- Coming from Python or another language? `data` is Eza's **class**: fields, functions that use `self`, `setup` (like `__init__`), and `from` / `super` for building one type on another. Writing `class Item` gives a hint to write `data Item`.

### Functions inside a type

A type can have its own functions (often called **methods**). Inside them, `self` is the object the function was called on:

```eza
data Enemy
    name = ""
    hp = 100

    define take_damage, amount
        change self.hp by -amount

    define is_dead
        return self.hp <= 0

goblin = Enemy(name="Goblin")
goblin.take_damage(30)
print(goblin.hp)            # 70
print(goblin.is_dead)       # false
```

- Call them with a dot: `goblin.take_damage(30)`.
- A function with no arguments works without `()`, just like `.length` or `.upper`: `goblin.is_dead`.
- Changes to `self` change the real object, wherever it lives: `goblin`, `enemies[2]`, `team.leader`, or an object passed into a function.
- They're ordinary changes, so `rewind` and the F1 debugger see them too.

### `setup`: when a new object is made

A function called `setup` runs on every new object, right after its properties are filled in:

```eza
data Player
    name = "Player"
    hp = 100
    inventory = []

    define setup
        print("{self.name} joins the game")
        change self.inventory to ["map"]

ada = Player(name="Ada")    # prints: Ada joins the game
```

### Building on another type: `from`

`data Boss from Enemy` makes a new type that starts with everything `Enemy` has: its properties, with their defaults, and its functions. Then it adds its own, or replaces some:

```eza
data Boss from Enemy
    hp = 300                 # a different default
    phase = 1                # a new property

    define take_damage, amount
        super.take_damage(amount / 2)      # Enemy's version, with half the damage
        if self.hp < 150
            change self.phase to 2

dragon = Boss(name="Dragon")
dragon.take_damage(100)
print(dragon.hp, dragon.phase)     # 250 1
print(dragon.is_dead)              # false - is_dead comes from Enemy
```

- `super.name(...)` calls the version from the type it was built from, on the same object.
- `x.is_a(Enemy)` is `true` for an `Enemy`, and for anything built from `Enemy` (like a `Boss`). `type(x)` gives the exact type's name, like `"Boss"`.
- A type can be built from a type that was itself built from another one, as many levels as you like.

> Saving an object with `save` keeps its properties but not its type. Loading it back gives a plain dictionary.

---

## 17. Working with lists: `filter`, `map`, `group_by`

These methods take a **short function** (see [`->`](#short-functions-)) and run it on every item. Like the other list methods, they give back something new and leave the original list alone.

```eza
scores = [12, 5, 30, 8, 30]
print(scores.filter(s -> s > 10))       # [12, 30, 30]   keep the items where it's true
print(scores.map(s -> s * 2))           # [24, 10, 60, 16, 60]   change every item
print(scores.find(s -> s > 20))         # 30     the first one where it's true (none if nothing)
print(scores.count(s -> s < 10))        # 2      how many
```

| Method | Gives back | Example | Result |
|---|---|---|---|
| `.filter(f)` | the items where `f` is true | `[1, 5, 9].filter(n -> n > 3)` | `[5, 9]` |
| `.map(f)` | `f` applied to every item | `[1, 2].map(n -> n * 10)` | `[10, 20]` |
| `.find(f)` | the first item where `f` is true, or `none` | `[1, 5, 9].find(n -> n > 3)` | `5` |
| `.count(f)` | how many items make `f` true | `[1, 5, 9].count(n -> n > 3)` | `2` |
| `.count(x)` | how many items equal `x` | `[1, 1, 2].count(1)` | `2` |
| `.any(f)` | is `f` true for at least one? | `[1, 5].any(n -> n > 3)` | `true` |
| `.all(f)` | is `f` true for every one? | `[1, 5].all(n -> n > 3)` | `false` |
| `.unique` | the list without repeats | `[1, 1, 2, 1].unique` | `[1, 2]` |
| `.index_of(x)` | where `x` is, or `-1` | `["a", "b"].index_of("b")` | `1` |
| `.group_by(f)` | a dictionary of lists, grouped by what `f` gives | see below | |

### Lists of dictionaries

Real data often looks like a list of dictionaries (that's what [CSV files](#csv-files-spreadsheets) and [databases](#24-saving-records-database) give you). Two shortcuts help:

- A **dictionary works as a pattern** in `filter`, `find`, `count`, `any` and `all`: it matches items that have those fields with those values.
- `group_by` can take a **field name** instead of a function.

```eza
people = [{name: "Ada", city: "London", age: 36}, {name: "Lin", city: "Paris", age: 29}, {name: "Bo", city: "London", age: 41}]

print(people.filter({city: "London"}).map(p -> p.name))   # ["Ada", "Bo"]
print(people.map(p -> p.age).sum)                          # 106
print(people.sort_by(p -> p.age).first.name)                # Lin

by_city = people.group_by("city")
each city in by_city
    print(city, "has", by_city[city].length, "people")      # London has 2 people ...
```

---

## 18. Handling errors: `attempt` / `handle`

Normally an error stops your program. `attempt` lets you catch it and carry on:

```eza
attempt
    result = 10 / 0
handle
    print("Something went wrong:", error)
```

- Inside `handle`, the variable `error` holds the error message as text.
- You can pick a different name for it: `handle as problem` (or just `handle problem`).
- `handle` goes on its own line, lined up with `attempt`.

### What errors look like

When an error isn't caught, Eza prints its kind, the file and line, and a message:

```
[Runtime Error] game.eza:12: can't divide by zero
```

- **Syntax errors** are mistakes in how the code is written, found before anything runs (`eza check` finds these).
- **Runtime errors** happen while the code runs, like dividing by zero or using a name that doesn't exist.
- When you misspell a name, the message suggests the closest one: `Did you mean 'volume'?`
- An error inside a function also lists the calls that led to it. See [stack traces](#stack-traces).
- Use `eza check` to find many mistakes before you even run the program.

---

## 19. Multiple files, settings and shared values

### `include`: use another file

```eza
include "enemies.eza"
include "lib/helpers.eza"
```

Runs the other file right there, so its variables and functions become available. The path is relative to the file doing the including. Each file is only ever included once, even if several files include it. To keep its names separate from yours, [`use`](#20-modules-use) it instead.

### `param`: settings

```eza
param mimic_budget = 500
param gravity = -9.8
```

`param` sets one of Eza's settings, creating it or replacing it. Two settings are built in:

| Setting | Default | What it controls |
|---|---|---|
| `gravity` | `-20` | how fast 3D physics objects fall (you can also write `scene gravity=-9.8`, like `stage gravity=`; see [physics](#physics)) |
| `mimic_budget` | `1000` | how much `mimic` simulation work is allowed per frame |

`param` is for **settings**; `global` (below) is for **your own values** that need to reach another script.

### `global`: a shared notebook

`global` is a [dictionary](#10-dictionaries) that always exists. Put anything you like in it:

```eza
change global["online_mode"] to false
print(global.get("online_mode", true))
```

It's also the one thing (besides the volume) that survives [`go to`](#44-switching-scripts-go-to), so it's how one script passes values to the next.

### Other built-in names

| Name | What it is |
|---|---|
| `pi` | 3.14159... |
| `keyboard` | keyboard state (see [input](#keyboard-and-mouse-input)) |
| `mouse` | mouse state |
| `screen` | window size: `screen.width`, `screen.height` (1280 x 720 until a window opens) |
| `scene` | your 3D world, once you write a `scene` block |
| `stage` | your 2D world, once you write a `stage` block |
| `sound` | `sound.volume`, the master volume (see [sound](#40-sound-and-music-play-stop)) |
| `args` | the words typed after the script's name (see [`args`](#27-command-line-arguments-args)) |

---

## 20. Modules: `use`

When a program grows, split it into files. `include` pastes another file's names into yours. `use` keeps them in their own box, called a **module**, and you reach inside with a dot:

```eza
# enemies.eza
count = 0

define make, name
    change count by 1
    return {name: name, hp: 30}
```

```eza
# game.eza
use "enemies.eza"

goblin = enemies.make("goblin")
orc = enemies.make("orc")
print(enemies.count)       # 2
```

- The module is named after its file: `use "enemies.eza"` gives you `enemies`. Pick another name with `as`: `use "lib/enemy_tools.eza" as foes`.
- `use enemies` (no quotes) is short for `use "enemies.eza"`.
- The path is relative to the file that has the `use` line.
- A module runs **once**, the first time something uses it. Using it again (from any file) gives the same module, with the same variables.
- Its names don't clash with yours: both files can have their own `count`.
- Names starting with `_` are **private**: `_cache` can only be used inside its own file.
- `print(enemies)` shows what's inside: `<module enemies: count, make>`.

### Changing a module's variables

From outside, you can read a module's variables but not change them. Give the module a function that does it:

```eza
# enemies.eza
difficulty = 1

define set_difficulty, level
    change difficulty to level
```

```eza
# game.eza
use "enemies.eza"
enemies.set_difficulty(3)          # fine
change enemies.difficulty to 3     # error: only enemies.eza can change it
```

That way a module stays in charge of its own data, and you always know where a change came from.

### What a module can see

- The built-in names (`keyboard`, `mouse`, `screen`, `global`, `sound`, `args`, `pi`) work inside a module as usual.
- Your main file's variables don't. Pass what a module needs into its functions: `enemies.chase(player)`.
- Keep `scene`, `stage` and `gui` blocks in your main file (or a file it `include`s), not in modules.
- Two modules can't `use` each other. Eza stops with a message if they do: put what they share into a third file that both use.

### `include` or `use`?

| | `include "x.eza"` | `use "x.eza"` |
|---|---|---|
| Its names | become yours | stay in the module: `x.name` |
| Same name in both files | one replaces the other | no problem |
| Private names | no | names starting with `_` |
| Good for | splitting one program into parts | reusable tools, libraries, bigger projects |

[`eza check`](#eza-check-catch-mistakes-before-running) follows modules too. It checks each module file, and catches typos like `enemies.mkae("orc")` with `Did you mean 'make'?`.

---

## 21. Files and pictures: `load`, `save`, `append`

Paths are always relative to the script's own folder, so `"saves/game.json"` means the `saves` folder next to your script.

### Saving

```eza
save settings to "saves/settings.json"    # anything else becomes JSON
save "hello" to "notes.txt"               # text is written as-is
save pic to "copy.png"                    # pictures are saved as PNG
append "player died" to "log.txt"         # adds one line at the end
```

Missing folders are created for you.

### Loading

`load` looks at the file's ending:

| File | `load` gives |
|---|---|
| `.png` `.jpg` `.gif` `.bmp` | a picture (loaded once, then reused) |
| `.json` | the data inside: dictionaries, lists, numbers, text... |
| `.csv` | a list of dictionaries, one per row (see below) |
| anything else | the text |

```eza
back = load "saves/settings.json"
print(back.volume)

if exists("saves/settings.json")
    change settings to load "saves/settings.json"
```

- `exists("path")` says whether a file is there.
- Loading a file that doesn't exist is an error. Use `exists` or `attempt` to be safe.

### Pictures

```eza
pic = load "assets/map.png"
print(pic.width, pic.height)      # also pic.size -> [width, height]
c = pic.pixel(0, 0)               # the color at x=0, y=0 (top-left)
if c == #FF0000
    print("a red pixel")
print(c.a)                        # 0 means see-through
```

Pictures are also what you give to [sprites](#37-2d-worlds-stage) with `texture=`.

<a id="csv-files-spreadsheets"></a>
### CSV files (spreadsheets)

CSV is the simple table format every spreadsheet program can open and save. The first row names the columns:

```
date,item,category,amount
2026-09-01,Groceries,Food,71.35
2026-09-06,Concert,Fun,55
```

`load` turns every other row into a dictionary. Numbers become numbers and `true`/`false` become true/false:

```eza
rows = load "expenses.csv"
print(rows[0].item, rows[0].amount)              # Groceries 71.35
food = rows.filter({category: "Food"})
print(food.map(r -> r.amount).sum)
```

`save` with a `.csv` file writes a list of dictionaries back as a table, one column per key, ready to open in Excel or Google Sheets:

```eza
save food to "food.csv"
```

A list of lists works too: each inner list is one row.

---

## 22. Dates and times

```eza
right_now = now()                  # the date and time right now
day = today()                      # today, at midnight
trip = date("2026-12-24")          # from text: "YYYY-MM-DD" or "YYYY-MM-DD hh:mm"
party = date(2026, 12, 31, 20, 0)  # from numbers: year, month, day (hour, minute, second)
print(trip)                        # 2026-12-24
```

`type(trip)` is `"date"`. A date has these properties:

| Property | Example |
|---|---|
| `.year`, `.month`, `.day` | `2026`, `12`, `24` |
| `.hour`, `.minute`, `.second` | `0`, `0`, `0` |
| `.weekday` | `"Thursday"` |

### Showing a date: `.format`

```eza
print(trip.format("DD/MM/YYYY"))            # 24/12/2026
print(trip.format("Weekday DD Month YYYY"))  # Thursday 24 December 2026
print(now().format("hh:mm:ss"))              # 14:25:03
```

| Write | Becomes |
|---|---|
| `YYYY` / `YY` | year: `2026` / `26` |
| `MM` / `Month` / `Mon` | month: `12` / `December` / `Dec` |
| `DD` | day: `24` |
| `Weekday` / `Wkd` | day of the week: `Thursday` / `Thu` |
| `hh`, `mm`, `ss` | hours (0-23), minutes, seconds |

Anything else in the text stays as it is.

### Date math

```eza
print(trip.add_days(7))                      # 2026-12-31
print(today().days_until(trip), "days to go")
print(now().add_hours(3).format("hh:mm"))
```

| Method | Gives back |
|---|---|
| `.add_seconds(n)`, `.add_minutes(n)`, `.add_hours(n)` | a new date that much later (negative = earlier) |
| `.add_days(n)`, `.add_weeks(n)` | |
| `.add_months(n)`, `.add_years(n)` | (31 January plus one month is the end of February) |
| `.days_until(other)` | whole days from this date to `other` (negative if `other` is earlier) |
| `.seconds_until(other)` | seconds between them |
| `.date_only` | the same day at midnight |

Dates compare with `<`, `>` and `==`, so `if now() > deadline` works.

When a date is saved to a JSON file or a database it's written as text (`"2026-12-24"`). Turn it back into a date with `date(...)`.

---

## 23. The web: `fetch`

`fetch` downloads something from the internet. When the answer is JSON (which most web APIs use), you get dictionaries and lists; otherwise you get text.

```eza
weather = fetch("https://api.open-meteo.com/v1/forecast?latitude=51.5&longitude=-0.12&current=temperature_2m")
print("London:", weather.current.temperature_2m, "degrees")

page = fetch("https://example.com")     # a web page comes back as text
print(page.length)
```

### Sending data

```eza
reply = fetch("https://httpbin.org/post", send={name: "Ada", score: 10})
```

- `send=` with a dictionary or list sends it as JSON. With text, it sends the text. Sending uses POST.
- `method="PUT"` (or `"DELETE"`, `"PATCH"`...) picks another kind of request.
- `headers={Authorization: "Bearer abc123"}` adds headers, for APIs that need a key.

### When things go wrong

No internet, a wrong address, or an error from the server (like 404) is a normal Eza error, so wrap `fetch` in [`attempt`](#18-handling-errors-attempt--handle):

```eza
attempt
    data = fetch("https://api.example.com/scores")
handle problem
    print("couldn't download the scores:", problem)
    data = []
```

- `fetch` waits for the answer (up to 20 seconds). In a game, the window pauses while it waits, so fetch at the start or between levels, not every frame.
- Addresses must start with `https://` (or `http://`).

---

## 24. Saving records: `database`

A database keeps **records** (dictionaries) in a file, and saves after every change, so the data is still there next time the program runs. It's perfect for to-do lists, high scores, inventories and notes.

```eza
notes = database("notes.db")         # opens the file, or starts a new one

notes.add({title: "Buy milk", done: false})
notes.add({title: "Call Ada", done: false})
print(notes.count)                   # 2
print(notes.all)                     # [{id: 1, title: "Buy milk", done: false}, {id: 2, ...}]
```

Each record gets an `id` number automatically.

### Finding records

```eza
open = notes.find({done: false})            # every record matching a pattern...
open = notes.find(n -> not n.done)          # ...or a short function
first = notes.first(n -> n.title.contains("milk"))   # just the first match (or none)
same = notes.get(2)                          # by id
```

### Changing and removing

```eza
milk = notes.first({title: "Buy milk"})
notes.update(milk, {done: true})            # change some fields
notes.remove(milk)                           # or remove it
notes.remove({done: true})                   # remove every record that matches
notes.clear()                                # remove everything
```

| Method | What it does |
|---|---|
| `.add(record)` | saves a record and gives it back with its `id` |
| `.all` | every record, as a list |
| `.count` | how many records (`.count(pattern)` counts matches) |
| `.find(f)` / `.find(pattern)` | every matching record |
| `.first(f)` / `.first(pattern)` | the first matching record, or `none` |
| `.get(id)` | the record with that id, or `none` |
| `.update(record or id, changes)` | changes those fields; with a pattern, changes every match |
| `.remove(record, id, f or pattern)` | removes them, and tells you how many |
| `.clear()` | removes every record |

- The path is relative to the script. The file is plain JSON, so you can open it in any text editor.
- Records you get back are copies: change them with `.update`, not `change`.
- A database isn't part of the [`rewind`](#34-time-travel-rewind) timeline. It's real saved data.

---

## 25. Files and folders

These work with whole files and folders. To read and write what's *inside* a file, see [`load` and `save`](#21-files-and-pictures-load-save-append). As everywhere else, paths are relative to the script's folder.

### Looking around

```eza
print(files())                          # every file next to the script
print(files("photos"))                  # ["photos/cat.png", "photos/dog.jpg"]
print(files("photos", "*.png"))         # only the .png files
print(folders("saves"))                 # the folders inside saves
print(find_files("notes", "*.txt"))     # also looks inside every folder within notes
```

The paths they give back are sorted A to Z, and can go straight into `load`, `copy_file` and the rest.

In a pattern, `*` means "anything" and `?` means "any one character": `"*.csv"`, `"report_??.txt"`, `"*2026*"`. Capitals don't matter.

### Facts about a file

```eza
info = file_info("notes.txt")
print(info.size)          # in bytes
print(info.modified)      # a date, like 2026-10-04 18:30:00
```

| Field | What it is |
|---|---|
| `name` | `"notes.txt"` |
| `extension` | `"txt"` (`""` if there isn't one) |
| `folder` | the folder part of the path |
| `size` | the size in bytes (`0` for folders) |
| `modified` | when it last changed, as a [date](#22-dates-and-times) |
| `is_folder` | `true` for folders |

`exists(path)` tells you whether a file or folder is there, and `is_folder(path)` whether it's a folder.

### Making, copying, moving, deleting

| Function | What it does |
|---|---|
| `make_folder("saves/slot1")` | makes the folder (and any missing folders above it) |
| `copy_file("a.txt", "backup/a.txt")` | copies a file, or a whole folder with everything in it |
| `copy_file("a.txt", "backup")` | into a folder that already exists: keeps the name, so you get `backup/a.txt` |
| `move_file("a.txt", "old/a.txt")` | moves a file or folder; also renames: `move_file("a.txt", "b.txt")` |
| `delete_file("old.txt")` | deletes a file (not a folder) |
| `delete_folder("temp")` | deletes a folder **and everything in it** |

Any folders the destination needs are made automatically.

> Deleted files don't go to the Recycle Bin, so take care with `delete_file` and `delete_folder`. As a safety net, `delete_folder` refuses to delete the folder your script is in, your home folder, or a whole drive.

### Example: sort a messy folder

```eza
# moves every file in "downloads" into a folder named after its ending: downloads/png, downloads/pdf, ...
each path in files("downloads")
    info = file_info(path)
    if info.extension != ""
        folder = "downloads/" + info.extension.lower
        make_folder(folder)
        move_file(path, folder)
```

---

## 26. Running other programs: `run`

`run` starts another program, waits for it to finish, and gives back what it printed:

```eza
print(run("git status"))

result = run("git log --oneline -5")
if result.ok
    each line in result.output.lines
        print("commit:", line)
else
    print("git failed:", result.errors)
```

| Field | What it is |
|---|---|
| `output` | everything the program printed |
| `errors` | its error messages |
| `code` | its exit code (`0` usually means success) |
| `ok` | `true` if it succeeded |

Printing the result itself shows the output, so `print(run("dir"))` just works.

Write the command the way you'd type it in a terminal (Command Prompt on Windows). To start a program directly, with no terminal in between, give a list instead: `run(["git", "commit", "-m", "fixed the jump"])`. Then spaces and quotes inside the parts can't cause trouble.

### Settings

| Setting | What it does | Example |
|---|---|---|
| `input=` | types text into the program | `run("sort", input="pear\napple\n")` |
| `folder=` | runs it in another folder (relative to the script) | `run("git pull", folder="my_game")` |
| `show=true` | shows its output while it runs (then `output` stays empty) | `run("cargo build", show=true)` |

Without `folder=`, programs run in the script's folder. A program can't ask you questions while it runs, so give it its answers up front with `input=`.

If the program can't be started at all (it isn't installed, or the name is misspelled), that's an error: `can't start "gti status": ...`. A program that starts but then fails isn't an error, so check `.ok`.

---

## 27. Command-line arguments: `args`

Words typed after the script's name reach the script in `args`, a list of text:

```
eza greet.eza Ada 3
```

```eza
# greet.eza
if len(args) < 2
    print("usage: eza greet.eza <name> <times>")
else
    name = args[0]
    times = num(args[1])
    each i in times
        print("Hello, {name}!")
```

- `args` is always a list, and `[]` when nothing was typed.
- Every item is text: turn numbers into numbers with `num(args[0])`.
- Put words with spaces in quotes: `eza notes.eza "shopping list"` gives `["shopping list"]`.
- It works the same with `eza run tool.eza a b`.
- A program made with [`eza build`](#46-sharing-your-program-eza-build) gets them too: `greet.exe Ada 3`.

---

## 28. Built-in functions

| Function | What it does | Example | Result |
|---|---|---|---|
| `print(a, b, ...)` | shows values, separated by spaces | `print("hp:", 10)` | `hp: 10` |
| `input(prompt)` | asks the user to type something; gives back text | `name = input("Name? ")` | |
| `len(x)` | length of text or a list | `len("hey")` | `3` |
| `str(x)` | turns anything into text | `str(12) + "!"` | `"12!"` |
| `num(x)` | turns text (or true/false) into a number | `num("2.5")` | `2.5` |
| `int(x)` | like `num` but drops the decimals | `int(3.9)` | `3` |
| `type(x)` | the type's name | `type(5)` | `"number"` |
| `chr(n)` | the letter with code `n` | `chr(65)` | `"A"` |
| `ord(letter)` | the code of one letter | `ord("A")` | `65` |
| `random()` | random number from 0 up to (but not including) 1 | `random()` | e.g. `0.42` |
| `random_int(a, b)` | random whole number from `a` to `b` (both included) | `random_int(1, 6)` | e.g. `4` |
| `distance(a, b)` | distance between two points | | see [vectors](#11-vectors-positions-and-directions) |
| `lerp(a, b, t)` | blend between two values | | see [vectors](#11-vectors-positions-and-directions) |
| math | `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `radians`, `degrees` | | see [numbers](#7-numbers) |
| `stack(list)` / `queue(list)` | a new stack or queue, optionally with starting items | `stack([1, 2])` | |
| `exists(path)` | is there a file or folder at this path? | `exists("save.json")` | `true` / `false` |
| `files` / `folders` / `find_files` / `file_info` | look around folders | `files("photos", "*.png")` | see [files and folders](#25-files-and-folders) |
| `make_folder` / `copy_file` / `move_file` / `delete_file` / `delete_folder` | change files and folders | `copy_file("a.txt", "backup")` | see [files and folders](#25-files-and-folders) |
| `run(command)` | runs another program | `run("git status").output` | see [`run`](#26-running-other-programs-run) |
| `now()` / `today()` / `date(...)` | dates and times | `date("2026-10-04")` | see [dates](#22-dates-and-times) |
| `fetch(url)` | download from the web | | see [the web](#23-the-web-fetch) |
| `database(path)` | records saved in a file | | see [databases](#24-saving-records-database) |
| `raycast(from=, direction=, distance=)` | the first thing along a line | | see [2D](#touching-and-looking) |
| `find_path(grid, start, goal)` | the way through a grid, around walls | `find_path(["..#", "..."], [0, 0], [2, 1])` | see [pathfinding](#finding-a-way-pathfinding) |

**Brackets on methods:** a method that needs nothing passed to it can be written with or without `()`: `name.upper` and `name.upper()` are the same. A method that takes something always needs them: `name.replace("a", "b")`.

Every value also has two universal methods:

- `x.type` is the same as `type(x)`
- `x.to_string` is the same as `str(x)`

Some things that read files or change things are **statements** instead of functions (no parentheses): `load`, `save`, `append`, `push`, `pop`, `play`, `stop`, `emit`, `go to`, `use`. They're explained in their own sections.

> `print` works with or without brackets: `print("hi", score)` or `print "hi", score`. Every other function needs them: `len(name)`. `num("abc")` is an error because `"abc"` isn't a number.

---

## 29. Time: frames and `tick`

The features from here on are about **time**. Eza counts time in **frames** (also called **steps**). In the 3D window there are 60 frames per second, and the engine advances one frame automatically each time.

In a script without a window, time stands still until you move it forward yourself with `tick`:

```eza
tick        # advance 1 frame
tick 30     # advance 30 frames
```

Each frame, in this order, Eza:

1. clears `mimic` results from the previous frame and runs waiting simulations,
2. moves every running `tween` one step,
3. counts down every `persist` and undoes the ones that have ended,
4. checks every `on` handler,
5. resumes anything that was `wait`-ing,
6. moves physics objects.

`tick` is mainly for testing time-based code in the terminal. In a game with a window you normally don't write it at all.

---

## 30. Reacting to things: `on`

`on` sets up code that runs **when a condition becomes true**:

```eza
on player.health <= 0
    print("You died!")
```

- `on` doesn't run straight away. It's checked once per frame.
- It fires **once when the condition turns true**, not on every frame while it stays true. It can fire again after the condition has been false and then turns true again.

### Every frame: `on every frame`

To run code on every single frame, use `on every frame`. This is where game logic usually goes (older scripts write `on scene.ticks`, which means the same):

```eza
on every frame
    if keyboard.held("d")
        change player.position.x by 0.1
```

### Leaving early with `return`

Inside an `on` block, `return` stops this frame's run of the block:

```eza
on every frame
    if paused
        return
    change enemy.position.x by 0.05
```

### Common triggers

```eza
on keyboard.pressed("space")            # a key goes down
on mouse.pressed("left")                # a mouse click
on player touches coin                  # starts touching something (see below)
on timer == 60                          # a value reaches something
```

### Touching: `on ... touches`

`on a touches b` runs **once each time two things start touching**, and `on a stops touching b` runs once each time they come apart:

```eza
on hero touches lava
    change hero.health by -10

on hero stops touching water
    print("out of the water")
```

Either side can be one object, a list of objects, or a **prefab**, which means every live copy of it. Then `as` names the two things that touched, so the block knows which ones:

```eza
on Bullet touches Enemy as b, e
    change e.hp by -b.damage
    destroy b

on hero touches Coin as h, coin
    destroy coin
    change score by 1
```

- Each pair counts on its own: two coins touched at once run the block twice, once per coin.
- A pair that keeps touching doesn't run it again; it has to come apart and touch again.
- An invisible sprite (`visible=false`) makes a good **trigger zone**: `on hero touches exit_zone`.
- To check right now instead (inside `if`), use the method: `if hero.touches(lava)`. Older scripts call it `collides_with`, which still works.

### Your own events: `trigger` and `on event`

An **event** is a name you make up. `trigger` sends it, and every `on event` block with that name runs straight away:

```eza
on event "boss_dead"
    play "fanfare.wav"
    change door.visible to false

on event "boss_dead"
    print("Level complete!")

# somewhere else, maybe deep inside a function:
trigger "boss_dead"
```

This keeps code apart: whatever kills the boss doesn't need to know about doors, sounds or messages.

`with` hands the blocks a value, and `as` names it:

```eza
on event "scored" as points
    change score by points

trigger "scored" with 10
```

- Several blocks can listen for the same event; they run in the order they were written.
- An `on event` block with `wait` in it carries on in the background, like other `on` blocks.
- [`eza check`](#eza-check-catch-mistakes-before-running) warns about an event that's triggered but nothing listens for (or the other way round), which is usually a spelling mistake: `trigger "boss_ded"`.

---

## 31. Pausing: `wait`

`wait` pauses a function or `on` block for a while, then carries on where it left off:

```eza
wait 30 steps        # 30 frames
wait 1 step          # 1 frame
wait 2 seconds       # 2 seconds (120 frames)
wait 5               # 'steps' is optional
```

### Rules for `wait`

- It only works inside a function or an `on` block, including inside `if`, `each` or `while` blocks there.
- It can't be used at the top level of a file, or inside `attempt`, `persist` or `mimic`.

### A function with `wait` runs in the background

When you call a function that contains `wait`, the caller doesn't wait for it. It carries on immediately and the function finishes on later frames. Such a function gives back `none`.

```eza
define countdown, n
    each i in n
        print(n - i)
        wait 1 second
    print("Go!")

countdown(3)
print("this prints right away")
```

---

## 32. Temporary effects: `persist`

`persist` makes changes that **undo themselves** later, which is useful for power-ups, buffs and status effects. Write the changes, then say when they should end.

### Ending after a number of frames: `for`

```eza
persist
    change player.speed by 15
    change player.invincible to true
for 180 steps
```

Note the `for ...` line goes after the indented block, lined up with `persist`. Seconds work too: `for 3 seconds` (one second is 60 frames).

### Ending when something happens: `until`

```eza
persist
    change player.speed by -5
until player.touches(dry_ground)
```

### Whichever comes first

```eza
persist
    change player.speed by -5
until player.touches(dry_ground) or 2 seconds
```

### On one line

Several `change`s can share a single line:

```eza
persist change player.speed by 15 change player.animation to "roll" for 3 steps
```

### Lasting as long as an `on` condition

Inside an `on` block, a `persist` with no ending lasts exactly as long as the `on` condition stays true:

```eza
on keyboard.held("shift")
    persist
        change player.speed by 0.3
```

Outside an `on` block, a `persist` must have an ending.

### How the undo works

When the effect ends:

- `change ... by` is undone by subtracting the same amount. Other changes made in the meantime are kept: if speed was boosted by 15 and the player also picked up +2 meanwhile, they end up +2.
- `change ... to` puts the old value back.
- A property that didn't exist before is removed again. If it was a true/false flag, it becomes `false` instead.

---

## 33. Smooth animation: `tween`

`tween` slides a value smoothly to a new value over a number of frames:

```eza
tween door.position.y to 5 over 60 steps
tween box.color to #0000FF over 30 steps ease linear
tween player.position to [0, 2, 0] over 60 steps ease ease_out
```

- You can tween numbers, colors, and lists of the same length (like positions).
- The thing you tween must already exist.
- Seconds work too: `tween door.position.y to 5 over 1.5 seconds`.

### Easing (how it speeds up and slows down)

| Ease | Feels like |
|---|---|
| `ease_in_out` (default) | starts slow, speeds up, ends slow |
| `ease_in` | starts slow, ends fast |
| `ease_out` | starts fast, ends slow |
| `linear` | constant speed |

### Interrupting

- Starting a new `tween` on the same value replaces the old one, starting from wherever the value is right now.
- A `change` to that value also stops the tween.

### Is it still moving?

While an object has a tween running, `.tweening` is `true` (older scripts call it `.animating`):

```eza
if not door.tweening
    tween door.position.y to 0 over 60 steps
```

---

## 34. Time travel: `rewind`

Every `change` is recorded as a step, and `rewind` steps back through them.

### One variable

```eza
score = 10
change score by 10       # 20
change score by 5        # 25

rewind score by 1 step   # back to 20
rewind score by 2 steps
rewind score to beginning    # back to 10, its first value
```

If you rewind further back than the history goes, the value stops at its first value and Eza prints a notice.

### Everything at once

```eza
rewind scene by 3 steps
rewind scene to beginning
```

`rewind scene` undoes the most recent changes across **all** variables, newest first. (`rewind all ...` and plain `rewind by 3 steps` mean the same thing.)

- In a script, `by N steps` means the last N changes.
- In the 3D window, `rewind scene by N steps` means the last N **frames**, so holding a key that runs `rewind scene by 3 steps` every frame plays time backwards.

### Writing after a rewind

If you `change` something after rewinding, the rewound "future" is erased and a new timeline starts from that point.

```eza
rewind score by 1 step
change score by 100       # the old future is gone
```

History keeps the most recent 1000 steps.

---

## 35. Predicting the future: `mimic`

`mimic` runs a simulation on a **copy** of an object (a "shadow"), so you can see what would happen without changing the real thing. An AI can use this to look ahead.

```eza
mimic future_enemy to enemy
    each step in 60
        change future_enemy.position.x by future_enemy.speed
        if future_enemy.position.x > scene.bounds.max
            change future_enemy.mimic_collided to true

if future_enemy.ready
    if future_enemy.mimic_collided
        change enemy.strategy to "retreat"
```

`mimic <shadow name> to <object>` copies the object, then runs the block on the copy.

### What the shadow gives you

| Property | Meaning |
|---|---|
| `.ready` | `true` once the simulation has finished |
| `.mimic_steps` | how many loop rounds the simulation ran |
| `.mimic_collided` | a flag you can set yourself; starts as `false` |
| `.mimic_state` | the shadow's final properties, without the `mimic_` extras |
| everything else | the copied object's properties, as the simulation left them |

### The purity rule

Inside a `mimic` block you may **only change the shadow** and variables you create inside the block. Changing anything else is an error:

```
Cannot modify global variable 'score' inside an isolated simulation block.
```

`wait`, `spawn`, `destroy`, `save`, `play`, `stop`, `emit` and `go to` aren't allowed inside `mimic` either, because a simulation must be silent and invisible. Random numbers inside a simulation follow the same sequence the real game would, and running one doesn't disturb the game's own random numbers.

### Timing

- The shadow stays readable until the end of the next frame, then resets to `ready = false`. Read it straight away.
- Running `mimic` again for a shadow that's still being simulated does nothing.
- **Budget:** each frame allows `mimic_budget` loop rounds in total (1000 by default, changed with `param mimic_budget = ...`). Simulations that don't fit wait with `ready = false` and finish on a later frame.

---

## 36. 3D worlds: `scene`

A `scene` block describes a 3D world. **If your script has a `scene`, `eza yourfile.eza` opens a window and shows it.** Without one (and without a `stage` or `gui`), the script just runs in the terminal. For 2D games, see [`stage`](#37-2d-worlds-stage).

```eza
scene name="demo"
    camera position=0,14,20
    light position=5,12,8 brightness=10000
    plane width=30 height=30 color=#4A7A4A
    cube name="wall" position=5,1,0 width=1 height=2
    player position=0,2,0 speed=0.15
    enemy position=8,2,8
```

### How a scene line is written

```
kind "optional label" key=value key=value ...
```

- Children are indented under their parent.
- The label names the thing: `cube "wall" position=5,1,0` makes a variable called `wall` (the same as `name="wall"`), just like `gui window "hud"` makes `hud`.
- Property values can be:
  - numbers: `width=4`, `x=-2`
  - text: `name="wall"`
  - colors: `color=#FF0000`
  - `true` / `false`
  - bare words, which are text: `mood=angry` is the same as `mood="angry"`
  - vectors and lists, in brackets like everywhere else (`position=[0, 1, 0]`), or as numbers with commas and no spaces (`position=0,1,0`)
  - **anything from your code, in round brackets**: `width=(size * 2)`, `position=([x, 0, z])`, `color=(team_color)`
- Without the round brackets, a word is text: `position=x,0,0` would store the text `"x"`.
- Characters (any kind that isn't a shape, like `player` or `enemy`) start with `health=100` and `speed=1`, so games can use them straight away.

### Kinds of things

| Kind | What it is | Size properties |
|---|---|---|
| `plane` | flat ground | `width` (x), `height` (depth along z), `seg` (detail) |
| `cube` | box | `width`, `height`, `depth` (defaults to `width`) |
| `sphere` | ball | `width` (diameter), `seg` (detail) |
| `cylinder` | tube | `width` (diameter), `height` |
| `camera` | where you look from (always looks at the center) | `position` |
| `light` | sunlight pointing at the center | `position`, `brightness` |
| `particles` | sparks, smoke, fire (see [particles](#38-particles-sparks-smoke-snow)) | |
| any other word | a character such as `player`, `enemy`, `tree` (drawn as a capsule) | |

- If there's no `camera`, it sits at `0,12,18`. If there's no `light`, a default one is added.
- Default colors: planes green, other shapes grey, `player` blue, `enemy` red, other characters orange.

### Properties everything has

| Property | Default | Meaning |
|---|---|---|
| `position` | `0,0,0` | where it is (x = right, y = up, z = toward you) |
| `rotation` | `0,0,0` | turn around each axis, in **degrees** |
| `scale` | `1,1,1` | stretch along each axis |
| `visible` | `true` | `false` hides it |
| `color` | depends on kind | its color |

Characters (any kind that isn't a shape) also start with `speed=1` and `health=100`. You can add any properties of your own, like `damage=5`.

### Using things from the scene in code

- Anything with `name="..."` becomes a variable with that name, so `cube name="wall"` gives you `wall`.
- Characters without a name become a variable named after their kind, so `player ...` gives you `player`.
- Everything is also reachable through `scene.children[i]` (and `.children[j]` for nested items).

```eza
change player.position.y by 1
change wall.color to #FF0000
print(scene.children[0].kind)
```

**While the window is open, these changes show up live:** `position`, `rotation`, `scale`, `visible` and `color`. Sizes are fixed once the scene is built.

### Scene facts

| Property | Meaning |
|---|---|
| `scene.bounds.min` / `scene.bounds.max` | how far the world reaches from the center |
| `x.bounds.min` / `.max` | half the width of a shape, on each side |
| `x.vertexCount` | number of points in a shape's surface |
| `x.kind` | the kind as text, e.g. `"cube"` |

### Bumpy terrain with noise

Wrap a `plane` or `sphere` in a noise block to make its surface bumpy:

```eza
scene
    simplex freq=0.25 amp=0.5 octaves=3
        plane width=30 height=30 seg=96
```

| Noise kind | Look |
|---|---|
| `simplex` (also `noise`) | smooth rolling hills |
| `perlin` | classic smooth noise |
| `worley` | cell-like, bubbly |
| `value_noise` | softer, blobby |

| Property | Meaning |
|---|---|
| `freq` | how close together the bumps are (bigger = more bumps) |
| `amp` | how tall the bumps are |
| `octaves` | layers of finer detail, from 1 to 8 |

Use a high `seg` (like 64 to 128) so the surface has enough detail to show the bumps.

### Touching: `touches`

```eza
if player.touches(coin)
    print("Got it!")

if player.touches([wall1, wall2, lava])    # touching any of them?
    print("bump")
```

This compares each object's box-shaped area. Objects that have been destroyed never collide.

<a id="physics"></a>
### Physics: falling and landing

Add `physics=true` to make something fall and stand on things:

```eza
scene
    plane width=40 height=40
    cube position=0,1,0 width=2 height=2
    player position=-6,5,0 physics=true
```

- Physics objects get a `velocity` (units per second) and a `grounded` flag (`true` while standing on something).
- `plane`, `cube`, `sphere` and `cylinder` are solid by default. Add `solid=false` to walk through one, or `solid=true` to make anything else an obstacle.
- Gravity is `-20` by default. Change it with `scene gravity=-9.8` (or `param gravity = -9.8`).
- There's no friction: something sliding keeps going until it hits an obstacle.

```eza
# jump
if keyboard.pressed("space") and player.grounded
    change player.velocity.y to 9
```

<a id="keyboard-and-mouse-input"></a>
### Keyboard and mouse input

| Check | True when |
|---|---|
| `keyboard.pressed("w")` | the key went down **this frame** |
| `keyboard.held("w")` | the key is down right now |
| `keyboard.released("w")` | the key came up this frame |
| `mouse.pressed("left")` | the button was clicked this frame |
| `mouse.held("left")` | the button is down right now |
| `mouse.released("left")` | the button came up this frame |
| `mouse.position` | the pointer as `[x, y]` in pixels from the top-left |

**Key names** are lowercase: letters `"a"`–`"z"`, digits `"0"`–`"9"`, `"space"`, `"enter"`, `"escape"`, `"tab"`, `"backspace"`, `"shift"`, `"ctrl"`, `"alt"`, and the arrows `"up"`, `"down"`, `"left"`, `"right"`. **Mouse buttons** are `"left"`, `"right"` and `"middle"`.

Input only does anything in a window. In a terminal-only script, keys and buttons always read as not pressed. While someone is typing in a [textbox](#typing-dragging-ticking-choosing-inputs), the game doesn't see those keys.

There are two ways to use input, and both are fine:

- **Checking every frame** (best for movement): `if keyboard.held("a")` inside `on every frame`.
- **Reacting once** (best for jumps, shooting, menus): `on keyboard.pressed("space")`.

---

## 37. 2D worlds: `stage`

A `stage` block is the 2D version of a `scene`. **If your script has a `stage`, `eza yourfile.eza` opens a window.** You can even have a `scene` and a `stage` together; the 2D layer is drawn on top.

```eza
hero_sheet = load "assets/hero_sheet.png"

stage name="forest" gravity=-1400
    tilemap tiles="assets/tilesheet.png" layout="maps/level1.txt" tile_size=32 position=-320,100
    sprite name="hero" texture=hero_sheet frame_size=32,32 position=-240,-40 origin=bottom_center physics=true
    sprite name="slime" texture="assets/slime.png" position=150,-92 origin=bottom_center
    sprite name="coin" position=0,52 width=14 height=14 color=#FFD54F
```

### Coordinates

- Positions are in **pixels**. `0,0` is the middle of the screen at the start.
- **y goes up**, just like in 3D. (The mouse is the exception: `mouse.position` counts from the top-left of the window. See [camera](#the-camera) to convert.)
- A sprite's `position` is where its **origin point** sits (the middle, unless you choose another).

### Sprites

`sprite` is a picture (or a plain colored rectangle if it has no `texture`). Sprites with `name="hero"` become a variable called `hero`.

| Property | Default | Meaning |
|---|---|---|
| `texture` | none | a picture variable (`texture=hero_sheet`) or a file path (`texture="assets/hero.png"`) |
| `position` | `0,0` | where the origin point is |
| `width`, `height` | the picture's size | size in pixels |
| `origin` | `center` | which point of the sprite `position` refers to (see below) |
| `rotation` | `0` | turn, in degrees |
| `scale` | `1` | `scale=2` or `scale=2,1` |
| `flip_x`, `flip_y` | `false` | mirror the picture |
| `color` | white | tint (or the rectangle's color without a texture) |
| `layer` | `0` | draw order: higher layers are drawn on top |
| `visible` | `true` | `false` hides it |
| `solid` | `false` | `true` makes it an obstacle for physics sprites |

**Origin points:** `center`, `top_left`, `top_center`, `top_right`, `center_left`, `center_right`, `bottom_left`, `bottom_center`, `bottom_right`, or two numbers (`origin=16,16`, pixels from the picture's top-left). `bottom_center` is handy for characters standing on the ground.

Sprites that share a picture are drawn together in one batch automatically, so hundreds of them are fine.

### Sprite sheets and animation

`frame_size=32,32` cuts the picture into frames (left to right, then top to bottom, counting from 0), and `frame` picks one:

```eza
change hero.frame to 2
```

To **animate**, give the sprite an `animation`: the frames to play, in order. It loops by itself:

```eza
sprite name="torch" texture=fire_sheet frame_size=16,16 animation=[0, 1, 2, 3] fps=12
```

Most characters have several animations. Name them with `animations`, then pick one by name:

```eza
stage
    sprite name="hero" texture=hero_sheet frame_size=32,32 animations={idle: [0], walk: [1, 2, 0], jump: [3]} animation="idle" fps=10

on every frame
    if hero.velocity.x != 0
        change hero.animation to "walk"
    else
        change hero.animation to "idle"
```

| Property | Default | Meaning |
|---|---|---|
| `animation` | none | the frames to play (`[1, 2, 3]`), or the name of one of the `animations` |
| `animations` | none | named frame lists: `{walk: [1, 2], idle: [0]}` |
| `fps` | `8` | animation frames per second |
| `loop` | `true` | `false` plays it once and stops on the last frame |
| `animation_done` | | becomes `true` when an animation with `loop=false` has finished |

- Changing `animation` to a different one starts it from its first frame. Setting it to the one already playing does nothing, so setting it every frame (like above) is fine.
- `change hero.animation to none` stops it, and `frame` stays where it was.
- Spawned sprites can animate too: put `animation=...` in the prefab, or `spawn Coin animation=[0, 1, 2, 3]`.
- A one-off animation: `change hero.loop to false` and `change hero.animation to "attack"`, then `on hero.animation_done` to go back to `"idle"`.

### Tilemaps

A tilemap draws a grid of tiles from one picture (`tiles=`) and a layout text file (`layout=`):

```
...........
.......2...
00000.00000
```

- One character per tile: `.` or a space is empty, `0`-`9` then `a`-`z` are tiles 0 to 35 (counted left to right, top to bottom in the tile picture).
- Or write numbers separated by commas, with `-1` for empty: `0,0,-1,3`.
- `tile_size=32` is the size of one tile, and `position` is the map's top-left corner.
- Tiles are solid unless you add `solid=false`.
- `level.tile_at([x, y])` gives the tile number at a world position (`-1` = empty). Give the tilemap a `name` to use it.

<a id="the-camera"></a>
### The camera

```eza
change stage.camera.position to hero.position     # follow the hero
change stage.camera.zoom to 2                     # zoom in (or tween it for a smooth zoom)
change stage.camera.rotation to 10                # tilt, in degrees
world_point = stage.camera.to_world(mouse.position)   # screen pixels -> world
screen_point = stage.camera.to_screen(hero.position)  # world -> screen pixels
```

### 2D physics

Add `physics=true` to a **named** sprite to make it fall and land:

- It gets `.velocity` (pixels per second, `[x, y]`) and `.grounded` (`true` while standing on something).
- The stage's `gravity` is `-980` unless you set it (`stage gravity=-1400`, or `gravity=0` for top-down games).
- Tilemaps and sprites with `solid=true` are obstacles.

```eza
on every frame
    change hero.velocity.x to 0
    if keyboard.held("d")
        change hero.velocity.x to 220
on keyboard.pressed("space")
    if hero.grounded
        change hero.velocity.y to 620
```

### Touching and looking

- `hero.touches(slime)` checks if two sprites overlap right now (origin and scale are taken into account).
- `hero.touches(level)` checks against the solid tiles of a tilemap.
- `raycast` looks along a line and tells you the first thing it hits:

```eza
hit = raycast(from=slime.position, direction=[-1, 0], distance=250)
if hit and hit.object == hero
    print("the slime sees you!")
```

A hit has `.object`, `.point` and `.distance`, and for tilemaps also `.tile` and `.cell`. With nothing in the way it's `none`. `raycast` works in 3D too, with 3-number vectors.

To run code when things start (or stop) touching, use [`on hero touches slime`](#touching-on--touches).

### Finding a way: pathfinding

`find_path` on a tilemap works out how to walk from one point to another **around the solid tiles**. It gives a list of points (the middles of the tiles to walk through), or `none` if there's no way through:

```eza
route = walls.find_path(slime.position, hero.position)
```

To follow it, move toward the first point; when you get there, drop it and head for the next. `.move_toward(target, step)` moves a point at most `step` closer, without going past:

```eza
route = []
timer = 0
on every frame
    change timer by 1
    if timer % 30 == 1                 # twice a second, look for the way again
        found = walls.find_path(slime.position, hero.position)
        if found != none
            change route to found
    if len(route) > 0
        change slime.position to slime.position.move_toward(route[0], 1.5)
        if distance(slime.position, route[0]) < 0.5
            route.remove(route[0])
```

- It only goes up, down, left and right. `walls.find_path(a, b, true)` also allows diagonal steps (it never cuts across the corner of a wall).
- It's made for top-down games (`gravity=0`): it doesn't know about jumping or falling.
- Things that walk the path should be a bit smaller than a tile, so they fit through gaps.

For a grid that isn't a tilemap, like a board game or a dungeon you made in a list, use the function `find_path(grid, start, goal)` (`find_path(walls, a, b)` works for tilemaps too). The grid is a list of text rows where `#` is a wall (or a list of lists where `true` or `1` is a wall), and places are `[column, row]`:

```eza
dungeon = [
    "..#.....",
    "..#.##..",
    "....#...",
]
print(find_path(dungeon, [0, 0], [7, 0]))     # [[1, 0], [1, 1], [1, 2], [2, 2], ...]
```

It gives the cells to step through after the start, ending at the goal, or `none` if the goal can't be reached. Add `true` at the end for diagonal steps.

See [`examples/maze_chase.eza`](examples/maze_chase.eza) for a whole game: a slime that chases you through a maze.

### 2D vectors

Positions are 2-number vectors, and all the [vector math](#11-vectors-positions-and-directions) works: `hero.position + [0, 50]`, `velocity * 1.5`. Two extras for 2D:

| Name | What it does | Example | Result |
|---|---|---|---|
| `.angle` | direction in degrees (0 = right, 90 = up) | `[0, 1].angle` | `90` |
| `.rotate(deg)` | turned by some degrees | `[1, 0].rotate(90)` | `[0, 1]` |

---

## 38. Particles: sparks, smoke, snow

Particles are lots of tiny dots that fly out, fade and disappear. Put a `particles` line in a `stage` (2D) or a `scene` (3D):

```eza
stage
    particles name="trail" rate=60 life=0.5 speed=30 size=10 end_size=1 color=#E1F5FE
    particles name="sparks" rate=0 life=0.8 speed=260 size=6 end_size=1 color=#FFE082 end_color=#FF6D0000 gravity=-400
```

- With `rate` above 0 they flow all the time, like a fountain or a trail.
- With `rate=0` nothing happens until you **burst** them with `emit`:

```eza
emit 40 from sparks                    # at the particles' own position
emit 40 from sparks at coin.position   # somewhere else
emit 60 from sparks at 120, -30        # or two (or three) numbers
```

`emit` needs particles that have a `name`.

### Settings

| Setting | Meaning | Default |
|---|---|---|
| `rate` | particles per second (`0` = only bursts) | `20` |
| `life` | how many seconds each particle lives | `1` |
| `speed` | how fast they fly out | `100` pixels/s in 2D, `3` in 3D |
| `direction` | which way: degrees in 2D (`90` = up, `270` = down), a vector in 3D (`direction=0,1,0`) | up |
| `spread` | how wide the cone is, in degrees (`360` = every direction) | `360` |
| `size`, `end_size` | size at the start and at the end | `6` pixels / `0.15` in 3D |
| `color`, `end_color` | they fade from one to the other; without `end_color` they fade out | white |
| `gravity` | pull along y per second (negative = down, positive = up like smoke) | `0` |
| `area` | start anywhere inside a box: `area=1300,0` for rain or snow across the screen | a single point |
| `position` | where they come from | `0,0` |
| `texture` | 2D only: a picture instead of the soft round dot | |
| `visible` | `false` stops new particles | `true` |

Each particle gets a little randomness in its speed and life, so effects look natural.

### Changing particles while the game runs

They're normal named objects, so `change` works:

```eza
on every frame
    change trail.position to hero.position     # the trail follows the hero
change smoke.rate to 0                         # stop the smoke
change sparks.color to #80D8FF
```

- Order matters in a stage: particles written before a sprite are drawn underneath it.
- Particles are only drawn on screen: they aren't part of the timeline (so they don't `rewind`), they never collide, and they freeze while the [time-travel debugger](#the-time-travel-debugger-f1) is open.
- In a script without a window, `emit` does nothing.

---

## 39. Templates and live objects: `prefab`, `spawn`, `destroy`

### `prefab`: a template

A prefab describes one object you want to make copies of later. It's written like a scene line:

```eza
prefab Bullet
    sphere width=0.5 color=#FFEB3B damage=5
```

A prefab is a single object: it can't have indented children.

### `spawn`: make a copy

```eza
b = spawn Bullet
b = spawn Bullet at 1,2,3
b = spawn Bullet at player.position + [0, 1, 0] damage=9 dir=[0, 0, -1]
```

- `at` sets the position: three numbers (or two for a 2D sprite), or any vector.
- `key=value` pairs after it set or add properties. Unlike in a scene, these **can** be expressions.
- In the window, spawned objects appear immediately and show live changes.
- Spawned objects can use `physics=true`, and `solid=true` makes them obstacles.

**A spawned object is live and shared.** Every variable holding it points at the same object:

```eza
copy = b
change copy.damage to 99
print(b.damage)        # 99
```

### `destroy`: remove something

```eza
destroy b
print(b.alive)         # false
```

- `.alive` is `true` until a spawned object is destroyed.
- Destroying something declared in the scene hides it instead: it gets `visible = false` and `destroyed = true`, and stops colliding and falling.
- `destroy` takes one object, or a list of spawned objects: `destroy Bullet.all` removes every bullet.

### Keeping track of copies: `.all`, `.count` and `into`

Games often need "every enemy that's still alive". A prefab keeps track of its copies by itself:

```eza
print(Enemy.count)              # how many are alive right now
each e in Enemy.all             # every live copy, oldest first
    change e.position.x by 1
destroy Bullet.all              # clear the screen
```

To keep your own list, spawn **into** it. Destroying a copy takes it out of the list again, so the list is always up to date:

```eza
coins = []
spawn Coin at 10, 50 into coins
spawn Coin at 90, 50 into coins
print(len(coins))               # 2

on hero touches Coin as h, coin
    destroy coin                # coins loses it straight away
    if len(coins) == 0
        trigger "all_coins"
```

- The list has to exist first (`coins = []`).
- One copy can be in several lists: spawn it into one, and `push` it to others by hand (but only the `into` list updates by itself).
- `into` works with any list, like `level.enemies` or `teams[0]`.

### A full example: homing bullets

```eza
define fly, shot
    each step in 90
        change shot.position by shot.dir * 0.5
        wait 1 step
    destroy shot

on mouse.pressed("left")
    fly(spawn Bullet at player.position dir=(enemy.position - player.position).normalize)
```

Because `fly` contains `wait`, each bullet flies on its own in the background.

---

## 40. Sound and music: `play`, `stop`

```eza
play "assets/sounds/coin.wav"                         # a sound effect
play "assets/sounds/music.wav" loop=true volume=0.5   # background music
play "assets/sounds/jump.wav" speed=1.3               # faster and higher
stop "assets/sounds/music.wav"                        # stop one sound
stop all                                              # stop every sound
```

- Works with `.wav`, `.ogg` and `.mp3` files. The path is relative to the script.
- A sound file that doesn't exist is an error that tells you the name, so typos are caught straight away.
- The same effect can play many times at once (every coin gets its own "ding").

| Setting | Meaning | Default |
|---|---|---|
| `loop` | `true` keeps repeating it | `false` |
| `volume` | 0 is silent, 1 is full | `1` |
| `speed` | `2` is twice as fast (and higher-pitched) | `1` |

### Master volume

`sound.volume` controls everything at once, from 0 to 1:

```eza
change sound.volume to 0.3
```

It's a great match for a [slider](#typing-dragging-ticking-choosing-inputs): `slider name="volume" min=0 max=1 then change sound.volume to volume`.

### Music that keeps going

If a looping sound is **already playing**, `play` with the same file doesn't restart it: it just updates its volume and speed. That means every level can start with the same `play "music.wav" loop=true` line, and when you [switch scripts](#44-switching-scripts-go-to) the music carries on smoothly. Looping sounds that the new script doesn't play are stopped.

Sound only plays in a window. In a terminal-only script, `play` still checks that the file exists, but stays silent.

---

## 41. Menus and HUDs: `gui`

A `gui` block builds a 2D window of boxes, text, buttons and inputs drawn on top of everything else. A script with only a `gui` (no scene or stage) still opens a window, so you can make plain apps too.

```eza
gui window "pause_menu" width=400 height=260 centered=true visible=false
    box position=top_bar height=50 color=#1A1A1A
        text "GAME PAUSED" position=center color=#FFFFFF
    box position=fill
        button "Resume" size=200,40 position=center then change pause_menu.visible to false
```

### Naming

The label of the top element becomes a variable, with any characters that aren't letters or digits turned into `_`. `"pause_menu"` becomes `pause_menu`, and `"Main Menu"` becomes `Main_Menu`.

### Text that keeps itself up to date

Put values in a label with `{ }`, and it **keeps showing the current value**, every frame, with no extra code:

```eza
score = 0
gui window "hud" x=16 y=16
    text "Score: {score}"
    text "Coins left: {len(coins)}"

change score by 10      # the window now says Score: 10
```

To change a label by hand instead, get at the parts inside with `.children[i]`:

```eza
title = pause_menu.children[0].children[0]
change title.label to "PAUSED!"
```

### Element kinds

| Kind | Looks like |
|---|---|
| `window` | dark, slightly see-through panel |
| `box` | invisible container (give it a `color` to see it) |
| `text` | a line of text (the label) |
| `button` | grey clickable button with the label, shades when hovered |
| `textbox` | a box you can type in (see [inputs](#typing-dragging-ticking-choosing-inputs)) |
| `slider` | a bar with a knob you drag |
| `checkbox` | a tick box with a label |
| `dropdown` | a button that opens a list of choices |
| `chart` | a bar, line or pie chart of a variable (see [charts](#43-charts)) |

### Properties

| Property | Meaning |
|---|---|
| `width`, `height` | size in pixels |
| `size=w,h` | both at once |
| `x`, `y` | top-left corner of the top element, in pixels |
| `centered=true` | put the top element in the middle of the screen |
| `position=...` | where a child sits inside its parent (see below) |
| `padding` | space inside the edges (default 8, but 0 for windows and anything with a `position`) |
| `gap` | space between the items stacked inside (default 0) |
| `color` | background (or the text color, for `text`) |
| `text_color` | label color on a button |
| `font_size` | text size in points (default 12, which is 16 pixels) |
| `background`, `rounded`, `border`, `glow`, `shadow`, `font` | see [styles](#42-styles-making-menus-look-good) |
| `style=name` | use a [style](#42-styles-making-menus-look-good) |
| `visible` | `false` hides it and everything inside |

Elements without a size fit their contents.

### Where children go: `position`

Parts are placed in three passes:

1. **Edges first**, in the order you wrote them. Each one claims a strip of the parent:
   `top_bar`, `bottom_bar`, `side_left`, `side_right`.
2. **Floating spots**, inside whatever space the edges left:
   `center`, `top_left`, `top_right`, `bottom_left`, `bottom_right`.
   Elements with no `position` stack top to bottom.
3. **`fill`** takes all the space that's still left.

Anything that would spill outside its parent is cut off at the edge. After the layout, every element has `x`, `y`, `width`, `height` and `clipped` (`true` if it was cut off) properties you can read.

### Clicking

Code after `then` runs when the button is clicked. Use one line or an indented block:

```eza
button "Quit" then print("bye")
button "Start" then
    change game_started to true
    change menu.visible to false
```

You can also click a button from code with `.click()`: `menu.children[0].click()`.

### Hovering

Inside an element, an `on` line can react to the mouse. The element's kind name (like `button`) or `self` refers to that element:

```eza
button "Play" color=#2E7D32 then start()
    on button.hover persist change button.color to #43A047
```

`.hover` is `true` while the mouse is over the element. Since a `persist` inside `on` lasts while the condition holds, the color goes back when the mouse leaves.

<a id="typing-dragging-ticking-choosing-inputs"></a>
### Inputs: typing, dragging, ticking, choosing

```eza
volume = sound.volume

gui window "settings" centered=true padding=20 gap=10
    textbox "Your name..." name="player_name"
    slider name="volume" min=0 max=1 then change sound.volume to volume
    checkbox "Show sparks" name="sparks_on" checked=true
    dropdown name="difficulty" options="Easy","Normal","Hard" value="Normal"
    button "Start" then print("Hi {player_name}, playing on {difficulty}")
```

**With `name="x"`, the input's value lives in the variable `x`.** Read it like any variable. Changing the variable (`change volume to 0.2`) updates the input on screen too.

| Kind | The variable holds | Settings |
|---|---|---|
| `textbox "placeholder"` | text (`""` until someone types) | `text=` starting text, `placeholder=`, `max_length=` (200) |
| `slider` | a number | `min=` (0), `max=` (100), `value=`, `step=` (1/100 of the range), `accent=` color |
| `checkbox "label"` | `true` / `false` | `checked=true`, `accent=` color |
| `dropdown` | the chosen option as text | `options="A","B","C"`, `value=` (the first option) |

- If the variable already exists and you don't give a starting value, the input shows what's in it. That's why the example sets `volume = sound.volume` first.
- Code after `then` runs **whenever the user changes the value**: every move of a slider, every tick of a checkbox, every choice in a dropdown. For a textbox it runs when **Enter** is pressed.
- Click a textbox to type in it. Backspace deletes, and Enter or Escape (or clicking elsewhere) finishes.
- Inputs use `style=` and the [style properties](#42-styles-making-menus-look-good) like everything else.

### Updating the screen

Changing `visible`, `label` or `color` redraws the GUI, and so does `rewind`:

```eza
on keyboard.pressed("escape")
    change pause_menu.visible to not pause_menu.visible
```

---

## 42. Styles: making menus look good

A `style` is a reusable look, a bit like CSS on websites. Write it once and use it on as many elements as you like:

```eza
style neon
    width = 160
    background = #050505
    color = #00FFCC            # text color
    font = sans-serif          # or a .ttf / .otf file next to your script
    font_size = 16             # points
    rounded = 8                # round corners
    border = 1, solid, #00FFCC # width, style, color
    glow = 10, #00FFCC, 0.4    # size, color, strength
    padding = 10, 20           # top/bottom, left/right
    on hover
        background = #00FFCC
        color = #050505
    on press
        scale = 0.9

gui window "menu" width=420 height=320 centered=true
    button "Play" style=neon then start_game()
    button "Quit"                      # elements without a style still work
```

- Every line is optional, and so are styles.
- `on hover` and `on press` change the look while the mouse is over the element, or while it's being clicked. Leaving puts everything back.
- Any style line also works directly on one element: `button "Go" background=#222 rounded=6`. When both are given, the element's own value wins.

### Style properties

| Property | Example | Meaning |
|---|---|---|
| `background` | `#050505` | background color |
| `color` | `#00FFCC` | text color (on a box or button without `background`, it's the background, as in older scripts) |
| `font` | `sans-serif`, `"fonts/neon.ttf"` | the font |
| `font_size` | `16` | text size in points |
| `rounded` | `8` | corner radius in pixels |
| `border` | `1, solid, #00FFCC` or `none` | an outline |
| `glow` | `10, #00FFCC, 0.4` | a soft glow: size, color, strength |
| `shadow` | `0, 6, 14, #00000088` | a drop shadow: x, y, blur, color |
| `padding` | `10` / `10, 20` / `5, 10, 5, 10` | space inside the edges |
| `width`, `height` | `160` | size |
| `scale` | `0.9` | shrink or grow (useful in `on press`) |

CSS-style names work too: `background-color`, `border-radius`, `font-size`, `font-family`.

---

## 43. Charts

A `chart` in a `gui` draws a bar, line or pie chart of a variable, and **redraws by itself whenever that variable changes**:

```eza
sales = {Jan: 120, Feb: 90, Mar: 160, Apr: 140}
temps = [12, 14, 13, 17, 21]
budget = {Rent: 900, Food: 400, Fun: 150}

gui window "dashboard" centered=true padding=16 gap=12
    chart "Sales" kind=bar data=sales width=420 height=220
    chart "Temperature" kind=line data=temps width=420 height=200 color=#FF8A65
    chart "Budget" kind=pie data=budget width=420 height=200

on keyboard.pressed("space")
    change sales["May"] to 210          # the bar chart grows a new bar
```

- `data=` names a variable (created at the top level of your script) holding a **dictionary** (the keys become the labels) or a **list of numbers**.
- For a list, `labels=` can name another variable holding the labels, like `labels=months`. Without it, the bars are numbered 1, 2, 3...
- The label after `chart` is the title.

| Setting | Meaning | Default |
|---|---|---|
| `kind` | `bar`, `line` or `pie` | `bar` |
| `data` | the variable to show | |
| `labels` | a variable with the labels (for lists) | 1, 2, 3... |
| `color` | one color for every bar / the line | |
| `colors=#4C9AFF,#FF8A65,...` | colors to cycle through | a built-in palette |
| `width`, `height` | size | `360`, `220` |
| `background`, `rounded`, ... | the panel's look, like any [element](#42-styles-making-menus-look-good) | dark panel |

Bar and line charts get a scale with round numbers. Pie charts get a legend with each value and its percentage.

---

## 44. Switching scripts: `go to`

Bigger games are easier to build as several files: a menu, a few levels, a game-over screen. `go to` switches to another script **in the same window**:

```eza
on keyboard.pressed("escape")
    go to "menu"              # runs menu.eza from the same folder (.eza is optional)
```

- Everything from the old script is cleared away: its variables, its 3D/2D world, its menus, its `on` handlers.
- Nothing after `go to` runs: the switch happens straight away, even from inside a function.
- `go to` the file you're already in restarts it.
- It works in terminal-only scripts too.

### Passing values along: `global`

Only two things survive the switch: `sound` (the master volume) and `global`, a dictionary meant for exactly this:

```eza
# menu.eza
change global["player"] to player_name
change global["difficulty"] to difficulty
go to "level1"

# level1.eza
player = global.get("player", "Player")      # a default, in case you run level1.eza directly
if global.get("difficulty", "Normal") == "Hard"
    change enemy.speed by 2
```

Using `.get` with a default means each level still works when you run it on its own while testing.

See `examples/menu.eza` and `examples/arena.eza` for a complete menu → game → menu loop with music.

---

## 45. Finding and fixing mistakes

### Reading an error message

When something goes wrong, Eza shows the code with the exact spot underlined, the values involved, **how those values got that way**, and a suggested fix:

```
[Runtime Error] game.eza:14: can't divide by zero  (E003)
 12 | each enemy in enemies
 13 |     if enemy.hp > 0
 14 |         change player.health by enemy.attack_power / enemy.defense
    |                                 ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    |                                                      ------------- this is 0
 15 |         if player.health <= 0
  ...
 10 | change enemies[1].defense to armor - 5
    |                   ------- enemies[1].defense became 0 here

   enemy.defense is 0, so enemy.attack_power / enemy.defense has no answer.
   enemy = Enemy(name: "Orc", hp: 30, attack_power: 12, defense: 0)
     enemies[1].defense became 0 at line 10
     enemies[1].defense was 2 when it was created, at line 8

   help: check it first:   if enemy.defense != 0
     or: never divide by less than 1:   enemy.attack_power / max(enemy.defense, 1)
```

Read it from the top:

- **The first line** says what went wrong, where (file and line), and the error's code.
- **`^^^`** marks the code that failed. **`---`** marks related spots: a value involved, or the line where a value was set, even far away in the file or in another file.
- **The notes** show the values involved. Because Eza remembers every change (that's what makes `rewind` work), it can tell you **how a value got there**: when it last changed, on which line, and what it was before. Values copied from somewhere else are followed back: here `enemy` is `enemies[1]`, so its history is shown.
- **`help:`** suggests a fix, written with your own names. `or:` gives another way.
- **`in heal (called from line 4)`** lines show the chain of functions it happened inside (see [stack traces](#stack-traces)).

Eza also spots common habits from other languages and says how Eza writes them: `x += 1` (Eza: `change x by 1`), `if x = 5` (Eza: `==`), `def`/`function` (Eza: `define`), `elif` (Eza: `else if`), `null`/`None` (Eza: `none`), `True` (Eza: `true`), and a `:` at the end of a line.

### Error codes: `eza explain`

Every kind of error has a code, like `(E003)` at the end of the first line. For a longer explanation and how to fix it:

```
eza explain E003
```

`eza explain` on its own lists every code.

### When a game hits an error

In a window (a game or an app), an error doesn't close everything:

- The game **pauses on the exact frame** where it happened, with the error on screen and the [time-travel debugger](#the-time-travel-debugger-f1) open. Press **Left** / **Right** to step back and watch how it went wrong.
- Press **F1** to keep playing. The `on` block that broke is **switched off**; everything else carries on. **F2** shows or hides the error again; a badge in the corner counts the errors.
- The same error again is only counted, not shown again. When the window closes, the console lists each error and how many times it happened.
- After 10 different errors, the game stops (you can still step through time), so one mistake doesn't bury you in hundreds of messages.
- The message also says which frame it was, how many seconds in, and which `on` block it was inside.

### `eza check`: catch mistakes before running

```
eza check game.eza
```

This reads your program (and every file it `include`s or `use`s) without running it, and reports:

- **names that are never created**, with a suggestion: `'scroe' isn't created anywhere in this program. Did you mean 'score'?`
- **wrong argument counts**: `heal takes 2 argument(s) but you gave 3`, `heal is missing the argument 'amount'`
- **named arguments that don't exist**: `heal has no parameter called 'amont'`
- **`=` used twice for the same name** in one block (use `change`)
- **`go to` a script that isn't there**
- **names a module doesn't have**: `the module enemies has no 'mkae'. Did you mean 'make'?`, and private `_names` used from outside
- **the wrong kind of value**, worked out from how your program makes each name (you never write types):
  - math that can't work: `score - "5"` (`score` is a number, `"5"` is text), `-name`, comparing text with a number using `<`
  - a function a value doesn't have: `name.uper()` (did you mean `.upper`?), `names.upper()` on a list (it suggests `.map`)
  - fields and functions your `data` types don't have: `goblin.nmae`, `Enemy(hp=5, speeed=2)`, `goblin.take_damage()` without its argument
  - calling something that isn't a function (`score()`), `[ ]` on a number, `names["first"]` on a list, `each` over true/false
- every **syntax error**

```
[Check Error] game.eza:14: goblin is an Enemy, which has no field or function 'nmae'. Did you mean 'name'?  (E007)
 13 | goblin = Enemy(name="Goblin")
 14 | print(goblin.nmae)
    |       ^^^^^^^^^^^
   goblin is an Enemy - it's made on line 13
   help: Enemy has fields: name, hp; functions: take_damage
```

It only reports what's **sure** to fail. If a name can hold different kinds of values (a number here, text there), or comes from a file or the web, `eza check` doesn't guess, so it never complains about code that works.

It also **warns** about things that are probably mistakes, but won't stop the program:

- **code that can never run**, because it comes right after a `return`, `break` or `continue`
- **a variable a function creates but never uses** (start the name with `_`, like `_unused`, if that's on purpose)
- **an event nobody listens for**, or an `on event` nothing ever triggers (usually a spelling mistake)

It prints `OK` (or `OK (2 warning(s))`) when it finds no errors. In VS Code it runs **while you type** (a moment after you stop), and underlines the exact spot: red for errors, yellow for warnings. `eza check` with no file checks every `.eza` file in the folder (and the folders inside it).

### Stack traces

When an error happens inside a function, Eza shows the chain of calls that led there, so you can see how it got there:

```
[Runtime Error] game.eza:2: 'amont' doesn't exist yet ... Did you mean 'amount'?
  in heal (called from line 4)
  in turn (called from line 6)
  in round (called from line 7)
```

Read it from the top: the error happened on line 2, inside `heal`, which was called from line 4, and so on.

### Tests: `test` and `expect`

A test is a small block that checks your code gives the right answers:

```eza
define double, n
    return n * 2

test "double works"
    expect double(4) == 8
    expect double(-1) == -2

test "names are capitalized"
    expect "ada".capitalize == "Ada"
```

- Tests **only run with `eza test`**, so they can live right next to your code without slowing your game down.
- `eza test` runs the tests in every `.eza` file in the folder (and its subfolders). `eza test game.eza` runs one file. In VS Code: **Eza: Run Tests**.
- A failing `expect` shows both sides, like `expect failed: 12 == 8 is not true`.
- At the end you get a count: `4 passed, 0 failed`.
- `expect` also works outside a test: it stops the program if the condition is false. That's handy for "this should never happen" checks.

<a id="the-time-travel-debugger-f1"></a>
### The time-travel debugger (F1)

Press **F1** in any Eza window. The game pauses, and a panel lists every variable, with `*` next to the ones that changed on the frame you're looking at.

| Key | What it does |
|---|---|
| **Left / Right** | step one frame back or forward (hold to keep going) |
| **Shift** + Left / Right | step 10 frames |
| **Home** / **End** | jump to the oldest / newest recorded frame |
| **F1** | resume the game from the frame you're on |

Everything on screen follows along as you step, so you can watch the moment something went wrong. If you resume from an earlier frame, the old "future" is replaced as soon as the game changes something.

Spawned and destroyed objects, particles, and lists or dictionaries with more than 256 items aren't recorded, so they don't move back in time.

---

## 46. Sharing your program: `eza build`

```
eza build game.eza
```

This makes a folder `dist/game/` next to your script, with:

- `game.exe`, which runs `game.eza` when you double-click it. **The people you share it with don't need Eza installed.**
- a copy of everything in your script's folder (pictures, sounds, data, other scripts), except `dist`, `target` and `.git` folders.

Zip the `dist/game` folder and send it. Building again replaces the old build.

- `eza build` runs [`eza check`](#eza-check-catch-mistakes-before-running) first and refuses to build a program with mistakes in it.
- The exe is about 60 MB, because it contains the whole Eza language and the game engine.
- A console window opens next to the program, which shows anything you `print`. If the program stops with an error, the console waits for Enter so the message can be read.
- Errors are also saved in a file next to the program (`game.errors.txt`), so if it goes wrong on a friend's computer, they can send you that file.
- Words typed after the program's name reach the script as [`args`](#27-command-line-arguments-args): `game.exe easy`.
- It makes Windows programs (`.exe`).

---

## 47. Cheat sheet

```eza
# ---- basics
x = 5                         # create
change x to 10                # replace
x, y = [3, 4]                 # several names from a list
name = saved or "Guest"       # or gives a default
if "key" in inventory         # in: lists, text, dictionaries, 1 to 10
change x by 1                 # add (numbers, text, lists, vectors)
print("x is {x}")             # text with values filled in

# ---- decisions and loops
if x > 3 then print("big")
if a
    ...
else if b
    ...
else
    ...
each item in list_or_number_or_text
    ...
each i in 1 to 10               # a range, both ends included
each key, value in dictionary   # two names unpack each item
match weapon
    "sword" then ...
    "bow", "crossbow" then ...
    1 to 5 then ...
    else ...
while condition
    ...
break / continue

# ---- functions and types
define name, p1, p2 = 10        # p2 has a default value
    return p1 + p2
name(1, 2)    name(p2=2, p1=1)
f = define p
    ...
data Point
    x = 0
    y = 0
pt = Point(1, 2)
data Enemy                    # Eza's classes
    hp = 100
    define setup              # runs on every new one
        ...
    define hit, n
        change self.hp by -n
data Boss from Enemy
    define hit, n
        super.hit(n / 2)
e = Enemy()   e.hit(5)   e.is_a(Enemy)

# ---- errors and files
attempt
    ...
handle error
    ...
include "other.eza"
param gravity = -9.8

# ---- time
tick / tick 10
on condition                  # fires when it becomes true
on every frame
on hero touches Coin as h, c  # once per pair when they start touching (also: stops touching)
on event "won" / trigger "won" with 10
wait 30 steps / wait 1 second
persist ... for 60 steps / for 2 seconds / until cond / until cond or 60 steps
tween x.position to [0, 0, 0] over 60 steps ease ease_out      (or: over 1 second)
rewind x by 2 steps / rewind x to beginning / rewind scene by 3 steps
mimic shadow to thing
    ...

# ---- 3D
scene name="world"
    camera position=0,12,18
    plane width=20 height=20
    player position=0,1,0 physics=true
prefab Coin
    sphere width=0.5 color=#FFD700
c = spawn Coin at 1,1,1 value=10
spawn Coin at 5,1,0 into coins       Coin.all   Coin.count   destroy Coin.all
destroy c
player.touches(c)
keyboard.pressed("space")    mouse.held("left")

# ---- GUI
gui window "hud" x=16 y=16 width=200 height=60
    text "Score: 0" color=#FFFFFF
    button "Pause" then change paused to true

# ---- data
s = stack()   push 5 to s   top = pop s   s.peek   s.empty
q = queue()   push "a" to q   first = pop q
d = {name: "Ada", hp: 10}   d.name   d["hp"]   d.get("x", 0)   d.has("x")
save d to "save.json"   d2 = load "save.json"   append "line" to "log.txt"
pic = load "map.png"   pic.pixel(0, 0)   exists("map.png")
flags & 0b100   1 << 4   0xFF.to_binary

# ---- 2D
stage gravity=-980
    tilemap tiles="tiles.png" layout="level.txt" tile_size=32
    sprite name="hero" texture="hero.png" position=0,0 origin=bottom_center physics=true
    sprite name="bat" texture=bat_sheet frame_size=16,16 animations={fly: [0, 1, 2]} animation="fly" fps=10
change stage.camera.position to hero.position
route = level.find_path(slime.position, hero.position)    pos.move_toward(route[0], 2)
find_path(["..#", "..."], [0, 0], [2, 1])
hit = raycast(from=a.position, direction=[1, 0], distance=200)

# ---- particles and sound
particles name="sparks" rate=0 life=0.8 speed=200 color=#FFD54F
emit 30 from sparks at hero.position
play "coin.wav"   play "music.wav" loop=true volume=0.5   stop all
change sound.volume to 0.5

# ---- GUI inputs and styles
style big
    background = #222
    rounded = 8
gui window "settings" centered=true gap=10
    textbox "Name..." name="player_name"
    slider name="volume" min=0 max=1
    checkbox "Music" name="music_on"
    dropdown name="mode" options="Easy","Hard"
    button "Go" style=big then go to "level1"

# ---- switching scripts
change global["score"] to 10   go to "level2"   global.get("score", 0)

# ---- testing
test "adds"
    expect 1 + 1 == 2
eza check game.eza      eza explain E003      # in a game: F1 = step through time, F2 = show the error

# ---- everyday tools
nums.filter(n -> n > 3)   nums.map(n -> n * 2)   nums.find(n -> n > 3)   nums.count(3)
people.filter({city: "Paris"})   people.group_by("city")   nums.unique
"a b".words   "x".pad_left(3, "0")   "ab12".matches("[a-z]+[0-9]+")   3.14159.format(2)
d = date("2026-10-04")   now()   today()   d.add_days(7)   d.days_until(other)   d.format("DD Month YYYY")
rows = load "data.csv"   save rows to "out.csv"
data = fetch("https://...")   fetch(url, send={a: 1})
db = database("notes.db")   db.add({title: "x"})   db.find({done: false})   db.update(rec, {done: true})   db.remove(rec)
chart "Sales" kind=bar data=sales      # inside a gui
eza build game.eza                     # in the terminal: dist/game/game.exe

# ---- modules, files and programs
use "enemies.eza"   enemies.make("orc")   use "lib/tools.eza" as t   _private_name
args                                   # eza tool.eza a b  ->  ["a", "b"]
files("photos", "*.png")   folders(".")   find_files("notes", "*.txt")   file_info("a.txt").size
make_folder("out")   copy_file("a.txt", "out")   move_file("a.txt", "b.txt")   delete_file("b.txt")   delete_folder("out")
r = run("git status")   r.output   r.ok   run(["git", "add", "."])   run("sort", input="b\na")
```

---

## 48. Common errors and what they mean

The message itself usually says what to do (see [reading an error message](#reading-an-error-message)). The code at the end of the first line, like `(E003)`, can be looked up with `eza explain E003`.

| Message | What's wrong | Fix |
|---|---|---|
| `'x' already exists - use 'change x to ...' to update it` | you used `=` on a name that already exists | use `change x to ...` |
| `'x' doesn't exist yet - create it with 'x = ...'` | typo, or used before it was created | check spelling, or create it first |
| `indentation doesn't line up with any outer block` | a line is indented by an amount that matches no block | line it up with the block it belongs to |
| `expected an indented block on the next line` | `if`/`each`/`define` with nothing under it | indent the next line, or use `then` and one statement |
| `text is missing its closing quote` | a `"` wasn't closed before the end of the line | close the quote |
| `can't divide by zero` | `/` or `%` by `0` | check the number first with `if` |
| `index 5 is out of range (length is 3)` | a list or text position that doesn't exist | use `len()` to check, or `-1` for the last item |
| `X has no property 'y'` | reading a property the object doesn't have | check spelling; create it with `change x.y to ...` |
| `f expects 2 argument(s) but got 3` | wrong number of arguments | match the `define` line |
| `f is missing the argument 'p'` | an argument wasn't given | pass it, by position or by name |
| `Cannot modify global variable '...' inside an isolated simulation block.` | a `mimic` block changed something other than its shadow | only change the shadow inside `mimic` |
| `wait only works inside a function or an 'on' block ...` | `wait` at the top level, or in `attempt`/`persist`/`mimic` | move it into a function or `on` block |
| `persist needs an ending like 'for 45 steps' ...` | a `persist` outside `on` with no ending | add `for N steps` or `until ...` |
| `too much recursion` | a function keeps calling itself | make sure it stops calling itself at some point |
| `there's nothing to pop - it's empty` | `pop` on an empty stack, queue or list | check `.empty` (or `.length`) first |
| `can't open "file"` / `can't load the image ...` | the file isn't there | check the path; it's relative to the script; use `exists()` |
| `can't find the sound "..."` | a `play` path is wrong | check the path and the file ending |
| `can't go to "..." - there's no such script next to this one` | a `go to` name is wrong | the file must be in the same folder |
| `emit needs particles with a name` | `emit` on unnamed particles | add `name="..."` to the `particles` line |
| `there's no style called '...'` | `style=` uses a style that doesn't exist | define it first with `style name` |
| `expect failed: ...` | an `expect` wasn't true | the message shows both sides; fix the code or the test |
| `couldn't reach ...` | `fetch` had no internet, or the address is wrong | check the address and your connection; use `attempt` |
| `the server at ... answered with error 404` | the web address doesn't exist (or the server refused) | check the address; some APIs need `headers=` with a key |
| `can't read "..." as a date` | `date(...)` got text in another format | write it like `"2026-10-04"` or `"2026-10-04 18:30"` |
| `"..." isn't a valid pattern` | a mistake in a `.matches` / `.find_all` pattern | check the brackets; repeat counts need `{{3}}` |
| `there's no record like that to update` | `.update` found nothing | check the id or pattern, e.g. with `.find` first |
| `X has no method '.y'` | calling a function the type doesn't have | check spelling, and that the `define` is indented inside the `data` block |
| `X.y needs 1 argument(s)` | used a function that takes arguments without `()` | write `x.y(...)` with its arguments |
| `there's no data type called 'X' to build Y from` | `data Y from X`, but X doesn't exist (yet) | check spelling, and declare X above Y |
| `the module m has no 'x'` | a typo, or the module doesn't create that name | check spelling; the message suggests the closest name |
| `'_x' is private to the module m` | names starting with `_` stay inside their file | rename it without the `_`, or add a function that gives it back |
| `a module's variables can only be changed by its own code` | `change module.x to ...` from another file | add a function to the module that changes it |
| `"x.eza" is already being loaded - two modules can't use each other` | a.eza uses b.eza, and b.eza uses a.eza | move what they share into a third file |
| `can't use "x.eza"` / `can't find the module file` | the module file isn't there | check the path; it's relative to the file with the `use` line |
| `can't subtract text from a number` (and add, multiply, divide) | the two sides are different kinds of values | turn text into a number with `num(...)`; `eza check` finds these before running |
| `name is text, which has no method '.uper'` | that kind of value doesn't have that function | check the spelling; the help line suggests the closest one |
| `goblin is an Enemy, which has no field or function 'nmae'` | the `data` type doesn't have that name | check the spelling; the help line lists what it has |
| `score is a number, not a function` | `( )` after something that isn't a function | remove the `( )` |
| `touches needs an object, a list of objects or a prefab` | `on x touches y` where one side is a number, text ... | use the objects themselves (or a prefab, for every copy) |
| `a prefab only has .all (its live copies) and .count` | reading a property of the prefab itself, like `Coin.value` | spawn a copy first (`c = spawn Coin`) and use `c.value` |
| `'coins' doesn't exist yet - make an empty list first` | `spawn ... into coins` before `coins = []` | create the list first |
| `sprite has no animation called "wlak"` | `animation` names one that isn't in `animations` | check the spelling; the message lists the names it has |
| `nothing listens for the event "x"` (a warning) | `trigger "x"`, but there's no `on event "x"` | check the spelling of both |
| `can't start "..."` | `run` couldn't find the program | check it's installed and spelled right |
| `there's no file "..." to delete` | a path is wrong | check it with `exists()` first |
| `"..." is a folder - use delete_folder for folders` | `delete_file` on a folder | use `delete_folder` |
| `delete_folder won't delete "..."` | it's your script's folder, your home folder or a drive | delete something more specific |

// Eza language support: autocomplete, hover docs, error squiggles (via `eza check`) and a Run command.
const vscode = require('vscode');
const { execFile } = require('child_process');

const KEYWORDS = {
  if: 'Run a block only when a condition is true.  `if x > 3 then`',
  then: 'Optional word that ends an `if`, `each` or button header.',
  else: 'Runs when the `if` condition was false. `else if` chains another check.',
  each: 'Loop over a list, text, or a number of steps.  `each fruit in fruits then`  /  `each step in 60`',
  in: 'Used by `each item in list`.',
  while: 'Repeat a block while a condition stays true.',
  change: 'Update an existing variable.  `change score by 10`  /  `change name to "eza"`',
  to: '`change x to value` sets a new value.  `mimic shadow to enemy`.',
  by: '`change x by 5` adds (numbers), appends (text/lists).  `rewind x by 2 steps`.',
  define: 'Define a function.  `define calculate_damage, base_power, multiplier`',
  return: 'Send a value back from a function. `return 100, 0, 450` returns a list.',
  rewind: 'Slide a variable (or the whole scene) back along its history.  `rewind score by 1 step`  /  `rewind score to beginning`  /  `rewind scene by 30 steps`',
  beginning: '`rewind score to beginning` resets to the creation value.',
  step: 'Unit of time for rewind/persist (in the engine, one frame).',
  steps: 'Unit of time for rewind/persist (in the engine, one frame).',
  data: 'A type: `data Item` then indented `name = ""` fields, and optionally functions (`define use_it` - inside, `self` is the object). Create with `Item(name="Sword")`, call with `sword.use_it()`. `define setup` runs on every new one. `data Boss from Enemy` builds on another type.',
  class: 'Another word for `data` (like Python): `class Enemy` then fields and `define` functions that use `self`. `class Boss from Enemy` builds on Enemy.',
  from: '`data Boss from Enemy` - Boss gets all of Enemy\'s fields and functions, then adds or replaces some. Also `emit 30 from sparks`.',
  self: 'Inside a type\'s function: the object it was called on.  `change self.hp by -amount`',
  super: 'Inside a function of `data Boss from Enemy`: `super.take_damage(n)` runs Enemy\'s version on the same object.',
  setup: '`define setup` inside a data type runs on every new object, right after its fields are filled in.',
  attempt: 'Try something; if it fails, run the `handle error` block instead of crashing.',
  handle: '`handle error` runs when the attempt failed. `error` holds the message.',
  include: 'Run another file; its names become yours.  `include "scripts/core/player.eza"`  (to keep them separate, use `use`)',
  use: 'Load a module: a file whose names stay in their own box.  `use "enemies.eza"` then `enemies.make("orc")`. `use "lib/tools.eza" as t` picks the name. Runs once; names starting with _ are private; only the module\'s own code can change its variables.',
  as: '`use "lib/enemy_tools.eza" as foes` - the name to reach the module by. Also `on Bullet touches Enemy as b, e` names the pair, and `on event "x" as info` names the value.',
  touches: '`on hero touches coin` runs once each time they start touching. Either side can be an object, a list, or a prefab (every live copy): `on Bullet touches Enemy as b, e` then use b and e.',
  stops: '`on hero stops touching water` runs once each time they stop touching.',
  touching: '`on hero stops touching water` runs once each time they stop touching.',
  trigger: '`trigger "boss_dead"` runs every `on event "boss_dead"` block right away. `trigger "scored" with 10` hands them a value.',
  event: '`on event "boss_dead"` runs when something does `trigger "boss_dead"`. `on event "scored" as points` gets the value given with `with`.',
  into: '`spawn Coin at 10,20 into coins` also adds the new copy to the list coins; destroying it takes it out again.',
  animation: 'A sprite plays these frames: `animation=[1, 2, 3]` or the name of one of its `animations`: `change hero.animation to "walk"`. `fps=10` sets the speed (default 8), `loop=false` plays once (then `.animation_done` is true).',
  animations: 'Named frame lists for a sprite: `animations={idle: [0], walk: [1, 2, 0]}`, then `change hero.animation to "walk"`.',
  fps: 'How many animation frames per second a sprite plays (default 8).',
  find_path: 'find_path(grid, [col, row], [col, row]) - the cells to walk through, around walls ("#" in text rows, or true/1 in lists). none if there is no way. Add `true` for diagonal steps.',
  path_to: 'level.path_to(from, to) on a tilemap - world points to walk through around solid tiles (none if there is no way). Follow them with .move_toward.',
  move_toward: 'pos.move_toward(target, step) - a point at most `step` closer to target (never past it).  `change slime.position to slime.position.move_toward(next, 2)`',
  args: '`args` - the words typed after the script\'s name, as a list of text: `eza tool.eza a b` gives ["a", "b"]. Built programs get them too.',
  param: 'A global setting.  `param mimic_budget = 1000`',
  scene: 'Declare a 3D scene: indented nodes like `plane width=10 height=10 seg=64`.',
  gui: 'Declare a GUI window.  `gui window "pause_menu" width=400 height=400 centered=true`',
  on: 'Event handler. `on scene.ticks` runs every frame; `on <condition>` fires once each time the condition becomes true.  `on player.collides_with(mud)`  /  `on button.hover`',
  persist: 'Temporary changes that undo themselves.  `persist ... for 45 steps`  /  `until <condition> or 120 steps`  /  no ending inside `on` = lasts while the on-condition is true',
  for: 'Ends a persist block: `for 45 steps`.',
  until: 'Ends a persist block: `until player.collides_with(dry_ground) or 120 steps`.',
  mimic: 'Simulate a copy of an entity. Inside, only the shadow may change. Gives .ready, .mimic_steps, .mimic_collided, .mimic_state.',
  tween: 'Smoothly move a value over time; the whole path is planned up front. A new tween or `change` on the same target interrupts it.  `tween player.position to [5, 0, 5] over 60 steps ease ease_out`',
  over: '`tween x to 10 over 60 steps`',
  ease: 'Tween curve: `linear`, `ease_in`, `ease_out`, `ease_in_out` (default).',
  stage: 'A 2D world. Indented `sprite` and `tilemap` lines; sprites with name="hero" become variables. `stage.camera.position`, `.zoom`, `.to_world(mouse.position)`.',
  sprite: '2D picture: `sprite name="hero" texture="assets/hero.png" position=100,50 origin=bottom_center physics=true`. Also frame_size=32,32 frame=0, flip_x, flip_y, layer, color (tint), solid.',
  tilemap: 'Grid of tiles: `tilemap tiles="assets/tiles.png" layout="maps/level1.txt" tile_size=32 position=-320,100`. Layout: one character per tile (. = empty, 0-9 a-z), or comma-separated numbers.',
  style: 'Reusable GUI look: `style neon` then lines like `background = #050505`, `rounded = 8`, `glow = 10, #00FFCC, 0.4`, plus `on hover` / `on press`. Use with `button "Play" style=neon`.',
  play: '`play "assets/jump.wav"` - plays a sound (.wav, .ogg, .mp3). Settings: loop=true, volume=0.5, speed=1.2. Playing a looping sound that is already on just updates it.',
  stop: '`stop "assets/music.ogg"` stops one sound, `stop all` stops every sound.',
  sound: '`sound.volume` - the master volume (0 to 1). `change sound.volume to 0.5`',
  emit: '`emit 30 from sparks` - a burst of particles, optionally `at 10, 20`. The particles need a name.',
  particles: 'Particle effect in a stage or scene: `particles name="sparks" rate=30 life=1 speed=120 direction=90 spread=360 size=6 end_size=1 color=#FFCC00 end_color=#FF000000 gravity=-300 area=200,0`. rate=0 means bursts only.',
  go: '`go to "level2"` - switch to another script (same folder, .eza optional). Only `global` and `sound` carry over.',
  textbox: 'Text input: `textbox "placeholder" name="player_name"`. The text is in the variable player_name. `then` runs on Enter.',
  slider: 'Slider: `slider name="volume" min=0 max=1 step=0.1`. The number is in the variable volume. `then` runs while dragging.',
  checkbox: 'Checkbox: `checkbox "Music" name="music_on" checked=true`. true/false is in the variable music_on.',
  dropdown: 'Dropdown: `dropdown name="difficulty" options="Easy","Normal","Hard" value="Normal"`. The choice is in the variable difficulty.',
  gap: '`gap=10` on a window or box: space between the items stacked inside it.',
  push: '`push 5 to stack` - adds to the end of a list, stack or queue.',
  pop: '`top = pop stack` - removes and gives back the newest item (queues give the oldest).',
  load: '`load "file"` - images (.png/.jpg/.gif/.bmp) become images, .json becomes data, anything else is text. Cached.',
  save: '`save value to "file"` - text is written as-is, images as PNG, everything else as JSON.',
  append: '`append "line" to "log.txt"` - adds one line to the end of a file.',
  test: 'A test: `test "double works"` then indented `expect` lines. Runs with `eza test` (Eza: Run Tests), skipped otherwise.',
  expect: '`expect double(4) == 8` - inside a test, fails it with both values shown if not true.',
  prefab: 'A template for objects you create while the game runs.  `prefab Bullet` then an indented object like `sphere width=0.5 damage=5`.',
  spawn: 'Create a live copy of a prefab.  `b = spawn Bullet at 1,2,3 damage=9`. Copies of `b` all point at the same object. `b.alive` says if it still exists.',
  destroy: 'Remove a spawned object, or hide something declared in the scene.  `destroy b`',
  wait: 'Pause a function or `on` block between frames.  `wait 30 steps` / `wait 2 seconds`. A function containing wait runs in the background: the caller carries on immediately.',
  at: '`spawn Bullet at 1,2,3` sets where the new object appears.',
  seconds: '`wait 2 seconds` (60 steps per second).',
  break: 'Leave the current `each` / `while` loop.',
  continue: 'Skip to the next round of the loop.',
  pi: 'The number pi (3.14159...).',
  tick: 'Advance frames in script mode (`tick` or `tick 30`). The engine does this automatically.',
  and: 'True when both sides are true.', or: 'True when either side is true.', not: 'Flips true/false.',
  true: 'Boolean true.', false: 'Boolean false.', none: 'No value.',
};

const BUILTINS = {
  print: 'print(a, b, ...) - show values separated by spaces',
  len: 'len(x) - length of text or a list',
  str: 'str(x) - turn anything into text',
  num: 'num("3.5") - turn text into a number',
  int: 'int(x) - number without its decimals',
  range: 'range(n) or range(a, b) - list of numbers',
  random: 'random() - number between 0 and 1',
  random_int: 'random_int(a, b) - whole number from a to b',
  input: 'input("prompt") - read a line typed by the user',
  type: 'type(x) - name of the value\'s type',
  sin: 'sin(x) - sine (radians)', cos: 'cos(x) - cosine (radians)', tan: 'tan(x) - tangent (radians)',
  asin: 'asin(x) - inverse sine', acos: 'acos(x) - inverse cosine', atan: 'atan(x) - inverse tangent',
  atan2: 'atan2(y, x) - angle of the point (x, y)',
  now: 'now() - the date and time right now. Has .year .month .day .hour .minute .second .weekday',
  today: 'today() - today\'s date (midnight)',
  date: 'date("2026-10-04"), date("2026-10-04 18:30") or date(2026, 10, 4) - a date. Methods: .format("DD Month YYYY"), .add_days(n), .days_until(other)...',
  fetch: 'fetch("https://...") - downloads from the web; JSON comes back as dictionaries/lists, anything else as text. fetch(url, send={...}) posts data; also method= and headers=.',
  database: 'database("todo.db") - records saved in a file. .add({...}), .all, .find(r -> ...) or .find({done: false}), .first, .get(id), .update(record, {...}), .remove(...), .count, .clear()',
  chart: 'Chart in a gui: `chart "Sales" kind=bar data=sales` (kind: bar, line, pie). data is a variable holding a list or a dictionary; it redraws when the variable changes. Also labels=, color=, colors=.',
  raycast: 'raycast(from=a, direction=[1, 0], distance=300) - none, or a hit with .object, .point, .distance (+ .tile/.cell for tilemaps). 2D or 3D.',
  stack: 'stack() or stack([1, 2]) - last in, first out. Use push/pop, .peek, .length, .empty',
  queue: 'queue() - first in, first out. Use push/pop, .peek, .length, .empty',
  exists: 'exists("file.txt") - is there a file or folder there (relative to the script)?',
  is_folder: 'is_folder("saves") - is it a folder?',
  files: 'files(), files("photos") or files("photos", "*.png") - the files in a folder, as paths like "photos/cat.png", sorted A to Z',
  folders: 'folders("saves") - the folders inside a folder (optionally with a pattern like "level*")',
  find_files: 'find_files("notes", "*.txt") - like files, but also looks inside every folder within',
  file_info: 'file_info("notes.txt") - .name, .extension, .folder, .size (bytes), .modified (a date), .is_folder',
  make_folder: 'make_folder("saves/slot1") - makes the folder (and any missing folders above it)',
  copy_file: 'copy_file("a.txt", "backup/a.txt") - copies a file or a whole folder; into an existing folder it keeps the name',
  move_file: 'move_file("a.txt", "old/a.txt") - moves or renames a file or folder',
  delete_file: 'delete_file("old.txt") - deletes a file (not a folder). Not undoable!',
  delete_folder: 'delete_folder("temp") - deletes a folder and everything in it. Not undoable! Refuses the script\'s own folder, your home folder and whole drives.',
  run: 'run("git status") or run(["git", "status"]) - runs another program and gives back .output, .errors, .code, .ok (printing it shows the output). Settings: input="...", folder="...", show=true',
  chr: 'chr(65) - the letter for a character code ("A")', ord: 'ord("A") - the code of a letter (65)',
  distance: 'distance(a, b) - distance between two vectors (or numbers)', lerp: 'lerp(a, b, t) - blend from a to b (numbers, vectors or colors), t from 0 to 1', radians: 'radians(deg) - degrees to radians', degrees: 'degrees(rad) - radians to degrees',
};

const M = (sig, doc, group) => ({ sig, doc, group });
const METHODS = {
  upper: M('.upper', 'UPPERCASE text', 'text'), lower: M('.lower', 'lowercase text', 'text'),
  length: M('.length', 'number of characters / items', 'text, list'),
  trim: M('.trim', 'remove spaces from both ends', 'text'), trimleft: M('.trimleft', 'remove spaces from the left', 'text'),
  trimright: M('.trimright', 'remove spaces from the right', 'text'),
  split: M('.split(sep)', 'split text into a list', 'text'), replace: M('.replace(a, b)', 'replace a with b', 'text'),
  contains: M('.contains(x)', 'true if x is inside', 'text, list'), reverse: M('.reverse', 'reversed copy', 'text, list'),
  capitalize: M('.capitalize', 'First letter uppercase', 'text'), repeat: M('.repeat(n)', 'repeat text n times', 'text'),
  abs: M('.abs', 'absolute value', 'number'), floor: M('.floor', 'round down', 'number'), ceil: M('.ceil', 'round up', 'number'),
  round: M('.round', 'round to nearest', 'number'), sqrt: M('.sqrt', 'square root', 'number'), pow: M('.pow(n)', 'raise to the power n', 'number'),
  clamp: M('.clamp(lo, hi)', 'keep between lo and hi', 'number'),
  min: M('.min / .min(x)', 'smallest item (list) or smaller of two (number)', 'number, list'),
  max: M('.max / .max(x)', 'largest item (list) or larger of two (number)', 'number, list'),
  to_string: M('.to_string', 'turn into text', 'any'),
  last: M('.last', 'last item', 'list'), sort: M('.sort', 'sorted copy', 'list'),
  sortBy: M('.sortBy(fn)', 'sort using a function\'s result as the key', 'list'), sum: M('.sum', 'add all numbers', 'list'),
  remove: M('.remove(x)', 'copy with x removed', 'list'), join: M('.join(sep)', 'join items into text', 'list'),
  r: M('.r', 'red 0-255', 'color'), g: M('.g', 'green 0-255', 'color'), b: M('.b', 'blue 0-255', 'color'), a: M('.a', 'alpha 0-1', 'color'),
  hex: M('.hex', 'color as "#RRGGBB"', 'color'), lighten: M('.lighten(0.2)', 'lighter color', 'color'), darken: M('.darken(0.2)', 'darker color', 'color'),
  mix: M('.mix(other, 0.5)', 'blend with another color', 'color'), invert: M('.invert', 'opposite color', 'color'),
  saturate: M('.saturate(amount)', 'more (or less, if negative) colorful', 'color'),
  position: M('.position', 'x, y, z position', 'scene'), rotation: M('.rotation', 'x, y, z rotation', 'scene'), scale: M('.scale', 'x, y, z scale', 'scene'),
  children: M('.children', 'nodes inside this one', 'scene, gui'), noise: M('.noise', 'noise applied to this mesh', 'scene'),
  bounds: M('.bounds', '.min / .max extents', 'scene'), vertexCount: M('.vertexCount', 'number of vertices', 'scene'), visible: M('.visible', 'shown or hidden', 'scene, gui'),
  x: M('.x', 'first component of a vector / GUI left edge', 'vector, gui'), y: M('.y', 'second component / GUI top edge', 'vector, gui'), z: M('.z', 'third component', 'vector'),
  ready: M('.ready', 'true once a mimic simulation has finished', 'mimic'), mimic_steps: M('.mimic_steps', 'how far the mimic simulated', 'mimic'),
  mimic_collided: M('.mimic_collided', 'true if the mimic hit something', 'mimic'), mimic_state: M('.mimic_state', 'snapshot of the final properties', 'mimic'),
  animating: M('.animating', 'true while a tween is moving this object', 'tween'),
  is_tweening: M('.is_tweening', 'same as .animating', 'tween'),
  collides_with: M('.collides_with(other)', 'true if the two objects overlap (also accepts a list of objects)', 'scene'),
  hover: M('.hover', 'true while the mouse is over this GUI element', 'gui'),
  bit_and: M('.bit_and(x)', 'bitwise AND (same as &)', 'number'), bit_or: M('.bit_or(x)', 'bitwise OR (same as |)', 'number'),
  bit_xor: M('.bit_xor(x)', 'bitwise XOR (same as ^)', 'number'), bit_not: M('.bit_not', 'flip every bit (same as ~)', 'number'),
  shift_left: M('.shift_left(n)', 'shift bits left (same as <<)', 'number'), shift_right: M('.shift_right(n)', 'shift bits right (same as >>)', 'number'),
  bit: M('.bit(i)', 'true if bit number i is 1', 'number'), to_binary: M('.to_binary', 'text like "1101"', 'number'), to_hex: M('.to_hex', 'text like "FF"', 'number'),
  keys: M('.keys', 'list of the keys', 'dictionary'), values: M('.values', 'list of the values', 'dictionary'),
  has: M('.has(key)', 'true if the key exists', 'dictionary'), get: M('.get(key, default)', 'value for a key, or the default', 'dictionary'),
  peek: M('.peek', 'the item pop would give next', 'stack, queue'), empty: M('.empty', 'true when nothing is left', 'stack, queue'),
  pixel: M('.pixel(x, y)', 'color of one pixel (top-left is 0, 0)', 'image'), size: M('.size', '[width, height]', 'image'),
  to_world: M('.to_world(point)', 'screen pixels -> world position', 'camera'), to_screen: M('.to_screen(point)', 'world position -> screen pixels', 'camera'),
  tile_at: M('.tile_at(point)', 'tile number at a world position (-1 = empty)', 'tilemap'),
  angle: M('.angle', 'direction of a 2D vector in degrees', 'vector'), rotate: M('.rotate(degrees)', 'turn a 2D vector', 'vector'),
  repeat: M('.repeat(n)', 'repeat text or a list n times', 'text, list'),
  filter: M('.filter(f)', 'items where f gives true:  nums.filter(n -> n > 10)  (or a dictionary pattern)', 'list'),
  map: M('.map(f)', 'f applied to every item:  nums.map(n -> n * 2)', 'list'),
  find: M('.find(f)', 'the first item where f gives true (database: all matching records)', 'list, database'),
  count: M('.count(x)', 'how many items equal x, or make f give true', 'list'),
  unique: M('.unique', 'the list without repeats', 'list'), index_of: M('.index_of(x)', 'position of x, or -1', 'list'),
  any: M('.any(f)', 'true if f is true for at least one item', 'list'), all: M('.all(f)', 'true if f is true for every item (database: every record)', 'list, database'),
  group_by: M('.group_by(f)', 'a dictionary of lists, grouped by what f gives (or by a field name)', 'list'),
  lines: M('.lines', 'list of lines', 'text'), words: M('.words', 'list of words', 'text'),
  pad_left: M('.pad_left(width, char)', 'fill on the left up to width:  "7".pad_left(3, "0") is "007"', 'text'),
  pad_right: M('.pad_right(width, char)', 'fill on the right up to width', 'text'),
  matches: M('.matches(pattern)', 'true if the WHOLE text fits the pattern (write {{3}} for repeat counts)', 'text'),
  find_all: M('.find_all(pattern)', 'every piece of the text that fits the pattern', 'text'),
  replace_pattern: M('.replace_pattern(pattern, with)', 'replace every piece that fits the pattern', 'text'),
  format: M('.format(x)', 'number: 3.14159.format(2) is "3.14";  date: d.format("DD Month YYYY hh:mm")', 'number, date'),
  commas: M('.commas', '1234567.commas is "1,234,567"', 'number'),
  add_days: M('.add_days(n)', 'a date n days later (also add_seconds/minutes/hours/weeks/months/years)', 'date'),
  days_until: M('.days_until(other)', 'whole days from this date to another', 'date'),
  seconds_until: M('.seconds_until(other)', 'seconds from this date to another', 'date'),
  date_only: M('.date_only', 'the same day at midnight', 'date'),
  add: M('.add(x)', 'list: a copy with x added;  database: saves a record and gives it back with an id', 'list, database'),
  update: M('.update(record, changes)', 'change fields of a saved record (or of every record matching a pattern)', 'database'),
  first: M('.first', 'first item;  database .first(f): the first matching record', 'list, database'),
  clear: M('.clear()', 'remove every record', 'database'),
  magnitude: M('.magnitude', 'length of a vector', 'vector'), normalize: M('.normalize', 'vector of length 1 pointing the same way', 'vector'),
  dot: M('.dot(other)', 'dot product of two vectors', 'vector'), cross: M('.cross(other)', 'cross product of two 3D vectors', 'vector'),
  alive: M('.alive', 'false after the object made by spawn was destroyed', 'spawn'),
  grounded: M('.grounded', 'true while a physics=true entity is standing on something', 'physics'),
  velocity: M('.velocity', 'x, y, z speed of a physics=true entity (units per second)', 'physics'),
  click: M('.click()', 'run a button\'s `then` action', 'gui'),
};

function userSymbols(doc) {
  const out = new Map();
  const re = /^\s*(?:define\s+([A-Za-z_]\w*)|data\s+([A-Za-z_]\w*)|param\s+([A-Za-z_]\w*)|([A-Za-z_]\w*)\s*=(?!=)|each\s+([A-Za-z_]\w*))/gm;
  const text = doc.getText();
  let m;
  while ((m = re.exec(text))) {
    if (m[1]) out.set(m[1], vscode.CompletionItemKind.Function);
    else if (m[2]) out.set(m[2], vscode.CompletionItemKind.Struct);
    else if (m[3]) out.set(m[3], vscode.CompletionItemKind.Constant);
    else if (m[4] && !KEYWORDS[m[4]]) out.set(m[4], vscode.CompletionItemKind.Variable);
    else if (m[5]) out.set(m[5], vscode.CompletionItemKind.Variable);
  }
  return out;
}

function exePath() {
  return vscode.workspace.getConfiguration('eza').get('executablePath') || 'eza';
}

function activate(context) {
  const sel = { language: 'eza' };

  context.subscriptions.push(vscode.languages.registerCompletionItemProvider(sel, {
    provideCompletionItems(doc, pos) {
      const before = doc.lineAt(pos).text.slice(0, pos.character);
      if (/\.\w*$/.test(before)) {
        return Object.entries(METHODS).map(([name, m]) => {
          const it = new vscode.CompletionItem(name, vscode.CompletionItemKind.Method);
          it.detail = `${m.sig}   (${m.group})`;
          it.documentation = new vscode.MarkdownString(m.doc);
          const paren = m.sig.match(/\((.*)\)/);
          if (paren && !m.sig.includes(' / ')) it.insertText = new vscode.SnippetString(`${name}($1)`);
          return it;
        });
      }
      const items = [];
      for (const [k, doc_] of Object.entries(KEYWORDS)) {
        const it = new vscode.CompletionItem(k, vscode.CompletionItemKind.Keyword);
        it.documentation = new vscode.MarkdownString(doc_);
        items.push(it);
      }
      for (const [k, d] of Object.entries(BUILTINS)) {
        const it = new vscode.CompletionItem(k, vscode.CompletionItemKind.Function);
        it.detail = d;
        it.insertText = new vscode.SnippetString(`${k}($1)`);
        items.push(it);
      }
      for (const [k, kind] of userSymbols(doc)) items.push(new vscode.CompletionItem(k, kind));
      return items;
    },
  }, '.'));

  context.subscriptions.push(vscode.languages.registerHoverProvider(sel, {
    provideHover(doc, pos) {
      const range = doc.getWordRangeAtPosition(pos, /[A-Za-z_]\w*/);
      if (!range) return;
      const word = doc.getText(range);
      const afterDot = range.start.character > 0 && doc.lineAt(pos).text[range.start.character - 1] === '.';
      if (afterDot && METHODS[word]) return new vscode.Hover(new vscode.MarkdownString(`**${METHODS[word].sig}** (${METHODS[word].group})\n\n${METHODS[word].doc}`));
      if (KEYWORDS[word]) return new vscode.Hover(new vscode.MarkdownString(`**${word}**\n\n${KEYWORDS[word]}`));
      if (BUILTINS[word]) return new vscode.Hover(new vscode.MarkdownString(BUILTINS[word]));
    },
  }));

  const diags = vscode.languages.createDiagnosticCollection('eza');
  context.subscriptions.push(diags);
  const path = require('path');
  const same = (a, b) => path.resolve(a).toLowerCase() === path.resolve(b).toLowerCase();
  const check = (doc) => {
    if (doc.languageId !== 'eza' || doc.uri.scheme !== 'file') return;
    // `eza check --plain` lists every problem it finds, one per line: file:line[:col:endcol]: message.
    // --stdin checks the editor's text, so problems show up while typing, before saving.
    const child = execFile(exePath(), ['check', '--plain', '--stdin', doc.fileName], { cwd: path.dirname(doc.fileName) }, (_err, _stdout, stderr) => {
      const list = [];
      for (const m of (stderr || '').matchAll(/\[(Syntax|Runtime|Check) (Error|Warning)\] (.*?):(\d+)(?::(\d+):(\d+))?: (.*)/g)) {
        // only problems in this file (eza check also reports included files and modules)
        if (!same(m[3], doc.fileName)) continue;
        const line = Math.min(Math.max(0, parseInt(m[4], 10) - 1), doc.lineCount - 1);
        let range = doc.lineAt(line).range;
        if (m[5]) {
          // the exact spot, when eza knows it
          range = new vscode.Range(line, parseInt(m[5], 10) - 1, line, parseInt(m[6], 10) - 1);
        }
        const severity = m[2] === 'Warning' ? vscode.DiagnosticSeverity.Warning : vscode.DiagnosticSeverity.Error;
        list.push(new vscode.Diagnostic(range, m[7], severity));
      }
      diags.set(doc.uri, list);
    });
    child.stdin.end(doc.getText());
  };
  // while typing: check once the typing pauses for a moment
  const timers = new Map();
  context.subscriptions.push(vscode.workspace.onDidChangeTextDocument((e) => {
    const doc = e.document;
    if (doc.languageId !== 'eza') return;
    clearTimeout(timers.get(doc.uri.toString()));
    timers.set(doc.uri.toString(), setTimeout(() => check(doc), 600));
  }));
  context.subscriptions.push(vscode.workspace.onDidSaveTextDocument(check));
  context.subscriptions.push(vscode.workspace.onDidOpenTextDocument(check));
  vscode.workspace.textDocuments.forEach(check);

  context.subscriptions.push(vscode.commands.registerCommand('eza.test', async () => {
    const ed = vscode.window.activeTextEditor;
    if (!ed) return;
    await ed.document.save();
    let term = vscode.window.terminals.find((t) => t.name === 'Eza');
    if (!term) term = vscode.window.createTerminal('Eza');
    term.show(true);
    term.sendText(`& "${exePath()}" test "${ed.document.fileName}"`);
  }));

  context.subscriptions.push(vscode.commands.registerCommand('eza.run', async () => {
    const ed = vscode.window.activeTextEditor;
    if (!ed) return;
    await ed.document.save();
    let term = vscode.window.terminals.find((t) => t.name === 'Eza');
    if (!term) term = vscode.window.createTerminal('Eza');
    term.show(true);
    term.sendText(`& "${exePath()}" "${ed.document.fileName}"`);
  }));
}

function deactivate() {}

module.exports = { activate, deactivate };

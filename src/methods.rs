use crate::error::{EzaError, R};
use crate::interp::Interp;
use crate::value::*;
use std::io::Write;
use std::rc::Rc;

pub const BUILTINS: &[&str] = &[
    "print", "len", "str", "num", "int", "range", "random", "random_int", "input", "type", "sin", "cos", "tan", "asin",
    "acos", "atan", "atan2", "radians", "degrees", "distance", "lerp", "chr", "ord", "stack", "queue", "exists", "raycast",
    "now", "today", "date", "fetch", "database", "files", "folders", "find_files", "file_info", "is_folder", "make_folder",
    "copy_file", "move_file", "delete_file", "delete_folder", "run", "find_path", "table", "panel", "progress", "clear",
    "quit",
];

/// A number or a list of numbers, as a vector.
fn vec_arg(it: &Interp, args: &[Value], i: usize, method: &str) -> R<Vec<f64>> {
    match arg(it, args, i, method)? {
        Value::Num(n) => Ok(vec![*n]),
        Value::List(l) => l.iter().map(|v| v.as_num(it.line)).collect(),
        v => err(it, format!("{} needs numbers or a vector, got {}", method, v.type_name())),
    }
}

fn err<T>(it: &Interp, msg: impl Into<String>) -> R<T> {
    Err(EzaError::runtime(it.line, msg))
}

fn arg<'a>(it: &Interp, args: &'a [Value], i: usize, method: &str) -> R<&'a Value> {
    match args.get(i) {
        Some(v) => Ok(v),
        None => err(it, format!(".{} needs {} argument(s)", method, i + 1)),
    }
}
fn arg_num(it: &Interp, args: &[Value], i: usize, method: &str) -> R<f64> {
    arg(it, args, i, method)?.as_num(it.line)
}
fn arg_str(it: &Interp, args: &[Value], i: usize, method: &str) -> R<String> {
    Ok(arg(it, args, i, method)?.display())
}

pub fn builtin(it: &mut Interp, name: &str, args: Vec<Value>, named: Vec<(String, Value)>) -> R<Value> {
    if name == "raycast" {
        // raycast(from=a, direction=[1, 0], distance=300), or the same values in order
        let get = |i: usize, key: &str| named.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).or_else(|| args.get(i).cloned());
        let nums = |v: Option<Value>, what: &str| -> R<Vec<f64>> {
            match crate::interp::deref_val(v.unwrap_or(Value::None)) {
                Value::List(l) => l.iter().map(|x| x.as_num(it.line)).collect(),
                other => err(it, format!("raycast needs {} as a vector like [1, 0], got {}", what, other.type_name())),
            }
        };
        let from = nums(get(0, "from"), "from")?;
        let dir = nums(get(1, "direction"), "direction")?;
        let dist = match get(2, "distance") {
            Some(v) => v.as_num(it.line)?,
            None => 1000.0,
        };
        return it.raycast(from, dir, dist);
    }
    if name == "fetch" {
        return crate::tools::fetch(it, &args, &named);
    }
    if name == "run" {
        return crate::tools::run_program(it, &args, &named);
    }
    // nicer terminal output (src/term.rs)
    match name {
        "print" if !named.is_empty() => {
            let text = args.iter().map(|a| a.display()).collect::<Vec<_>>().join(" ");
            println!("{}", crate::term::styled(it.line, &text, &named)?);
            return Ok(Value::None);
        }
        "input" if !named.is_empty() => {
            let prompt = args.first().map(|p| p.display()).unwrap_or_default();
            for (k, _) in &named {
                if k != "choices" && k != "hidden" {
                    return err(it, format!("input doesn't have a setting called '{}' (it has: choices, hidden)", k));
                }
            }
            if let Some((_, c)) = named.iter().find(|(k, _)| k == "choices") {
                let items = match crate::interp::deref_val(c.clone()) {
                    Value::List(l) => (*l).clone(),
                    other => return err(it, format!("choices= needs a list of things to pick from, got {}", other.type_name())),
                };
                return crate::term::choose(it.line, &prompt, &items);
            }
            return Ok(Value::Str(crate::term::hidden(&prompt)));
        }
        "table" => {
            let header = named.iter().find(|(k, _)| k == "header").map(|(_, v)| v.clone());
            if let Some((k, _)) = named.iter().find(|(k, _)| k != "header") {
                return err(it, format!("table doesn't have a setting called '{}' (it has: header)", k));
            }
            return Ok(Value::Str(crate::term::table(it.line, arg(it, &args, 0, name)?, header.as_ref())?));
        }
        "panel" => {
            let title = named.iter().find(|(k, _)| k == "title").map(|(_, v)| v.display()).or_else(|| args.get(1).map(|v| v.display()));
            let text = args.first().map(|v| v.display()).unwrap_or_default();
            return Ok(Value::Str(crate::term::panel(&text, title.as_deref())));
        }
        "progress" => {
            // each file in progress(files, "Copying"): the loop draws a progress bar as it goes
            let items = match crate::interp::deref_val(arg(it, &args, 0, name)?.clone()) {
                Value::List(l) => (*l).clone(),
                Value::Num(n) => (0..n.max(0.0) as i64).map(|i| Value::Num(i as f64)).collect(),
                other => return err(it, format!("progress needs a list (or a number) to go through, got {}", other.type_name())),
            };
            let label = named.iter().find(|(k, _)| k == "label").map(|(_, v)| v.display()).or_else(|| args.get(1).map(|v| v.display()));
            let mut o = Obj::new("progress");
            o.set("items", Value::list(items));
            o.set("label", Value::Str(label.unwrap_or_default()));
            return Ok(Value::obj(o));
        }
        "clear" => {
            crate::term::clear();
            return Ok(Value::None);
        }
        "quit" => {
            // ends the program: the script, the window, the web server or the terminal app
            it.quitting = true;
            return Err(EzaError::switch(it.line));
        }
        _ => {}
    }
    if let Some((k, _)) = named.first() {
        return err(it, format!("{}() doesn't take named arguments like '{}='", name, k));
    }
    Ok(match name {
        "print" => {
            println!("{}", args.iter().map(|a| a.display()).collect::<Vec<_>>().join(" "));
            Value::None
        }
        "len" => match arg(it, &args, 0, name)? {
            Value::Str(s) => Value::Num(s.chars().count() as f64),
            Value::List(l) => Value::Num(l.len() as f64),
            Value::Obj(o) if o.type_name == "dict" => Value::Num(o.fields.len() as f64),
            Value::Obj(o) if o.type_name == "stack" || o.type_name == "queue" => match o.get("items") {
                Some(Value::List(l)) => Value::Num(l.len() as f64),
                _ => Value::Num(0.0),
            },
            v => return err(it, format!("len() doesn't work on {}", v.type_name())),
        },
        "str" => Value::Str(arg(it, &args, 0, name)?.display()),
        "num" | "int" => {
            let n = match arg(it, &args, 0, name)? {
                Value::Num(n) => *n,
                Value::Str(s) => match s.trim().parse::<f64>() {
                    Ok(n) => n,
                    Err(_) => return err(it, format!("\"{}\" isn't a number", s)),
                },
                Value::Bool(b) => *b as i32 as f64,
                v => return err(it, format!("can't turn {} into a number", v.type_name())),
            };
            Value::Num(if name == "int" { n.trunc() } else { n })
        }
        "range" => {
            let (a, b) = if args.len() >= 2 {
                (arg_num(it, &args, 0, name)?, arg_num(it, &args, 1, name)?)
            } else {
                (0.0, arg_num(it, &args, 0, name)?)
            };
            Value::list((a as i64..b as i64).map(|i| Value::Num(i as f64)).collect())
        }
        "random" => Value::Num(it.random()),
        "random_int" => {
            let (a, b) = (arg_num(it, &args, 0, name)?, arg_num(it, &args, 1, name)?);
            Value::Num(a + (it.random() * (b - a + 1.0)).floor())
        }
        "input" => {
            if let Some(p) = args.first() {
                print!("{}", p.display());
                std::io::stdout().flush().ok();
            }
            let mut s = String::new();
            std::io::stdin().read_line(&mut s).ok();
            Value::Str(s.trim_end_matches(['\r', '\n']).to_string())
        }
        "type" => Value::Str(arg(it, &args, 0, name)?.type_name()),
        "stack" | "queue" => {
            let items = match args.first() {
                Some(Value::List(l)) => (**l).clone(),
                Some(v) => return err(it, format!("{}() takes a list of starting items, got {}", name, v.type_name())),
                None => vec![],
            };
            let mut o = Obj::new(name);
            o.set("items", Value::list(items));
            Value::obj(o)
        }
        // find_path(walls, from, to): a tilemap works too, the same as walls.find_path(from, to)
        "find_path" if matches!(args.first(), Some(Value::Obj(o)) if o.type_name == "tilemap") => {
            let map = args[0].clone();
            return call(it, map, "find_path", args[1..].to_vec(), true);
        }
        "find_path" => {
            // find_path(grid, [col, row], [col, row], diagonal): walls are "#" in text rows, or true / 1 in lists
            let grid: Vec<Vec<bool>> = match arg(it, &args, 0, name)? {
                Value::List(rows) => {
                    let mut g = vec![];
                    for r in rows.iter() {
                        g.push(match r {
                            Value::Str(t) => t.chars().map(|c| c == '#').collect(),
                            Value::List(cells) => cells
                                .iter()
                                .map(|c| match c {
                                    Value::Bool(b) => *b,
                                    Value::Num(n) => *n != 0.0,
                                    Value::Str(t) => t == "#",
                                    _ => false,
                                })
                                .collect(),
                            v => return err(it, format!("find_path needs a grid: a list of text rows like \"..#..\", or a list of lists, but a row is {}", v.type_name())),
                        });
                    }
                    g
                }
                v => return err(it, format!("find_path needs a grid (a list of rows) first, got {}", v.type_name())),
            };
            let cell = |i: usize, what: &str| -> R<(i64, i64)> {
                let v = vec_arg(it, &args, i, name)?;
                if v.len() != 2 {
                    return err(it, format!("find_path needs the {} as [column, row]", what));
                }
                Ok((v[0].round() as i64, v[1].round() as i64))
            };
            let (from, to) = (cell(1, "start")?, cell(2, "goal")?);
            let diagonal = args.get(3).map_or(false, |v| v.truthy());
            let h = grid.len();
            let w = grid.iter().map(|r| r.len()).max().unwrap_or(0);
            let blocked = |c: i64, r: i64| grid.get(r as usize).and_then(|row| row.get(c as usize)).copied().unwrap_or(false);
            match astar(w, h, from, to, diagonal, blocked) {
                Some(path) => Value::list(path.into_iter().map(|(c, r)| Value::list(vec![Value::Num(c as f64), Value::Num(r as f64)])).collect()),
                None => Value::None,
            }
        }
        "now" => crate::tools::now(),
        "today" => crate::tools::today(),
        "date" => crate::tools::date(it, &args)?,
        "database" => crate::tools::database(it, &args)?,
        "exists" => Value::Bool(it.resolve_path(&arg_str(it, &args, 0, name)?).exists()),
        "is_folder" => Value::Bool(it.resolve_path(&arg_str(it, &args, 0, name)?).is_dir()),
        "files" | "folders" => crate::tools::list_dir(it, &args, name == "folders")?,
        "find_files" => crate::tools::find_files(it, &args)?,
        "file_info" => crate::tools::file_info(it, &args)?,
        "make_folder" | "copy_file" | "move_file" | "delete_file" | "delete_folder" => crate::tools::file_command(it, name, &args)?,
        "chr" => {
            let n = arg_num(it, &args, 0, name)?;
            match char::from_u32(n as u32).filter(|_| n >= 0.0 && n.fract() == 0.0) {
                Some(c) => Value::Str(c.to_string()),
                None => return err(it, format!("{} isn't a letter code", n)),
            }
        }
        "ord" => {
            let s = arg_str(it, &args, 0, name)?;
            let mut cs = s.chars();
            match (cs.next(), cs.next()) {
                (Some(c), None) => Value::Num(c as u32 as f64),
                _ => return err(it, format!("ord() needs exactly one letter, got \"{}\"", s)),
            }
        }
        "distance" => {
            let (a, b) = (vec_arg(it, &args, 0, name)?, vec_arg(it, &args, 1, name)?);
            if a.len() != b.len() {
                return err(it, "distance needs two vectors of the same size");
            }
            Value::Num(a.iter().zip(&b).map(|(x, y)| (x - y).powi(2)).sum::<f64>().sqrt())
        }
        "lerp" => {
            let t = arg_num(it, &args, 2, name)?;
            crate::interp::lerp(arg(it, &args, 0, name)?, arg(it, &args, 1, name)?, t, it.line)?
        }
        "atan2" => Value::Num(arg_num(it, &args, 0, name)?.atan2(arg_num(it, &args, 1, name)?)),
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "radians" | "degrees" => {
            let x = arg_num(it, &args, 0, name)?;
            Value::Num(match name {
                "sin" => x.sin(),
                "cos" => x.cos(),
                "tan" => x.tan(),
                "asin" => x.asin(),
                "acos" => x.acos(),
                "atan" => x.atan(),
                "radians" => x.to_radians(),
                _ => x.to_degrees(),
            })
        }
        _ => return err(it, format!("unknown built-in '{}'", name)),
    })
}

/// `value.name` or `value.name(args)` when `name` isn't a stored property.
pub fn call(it: &mut Interp, recv: Value, name: &str, args: Vec<Value>, called: bool) -> R<Value> {
    let recv = crate::interp::deref_val(recv);
    if name == "to_string" {
        return Ok(Value::Str(recv.display()));
    }
    if name == "type" {
        return Ok(Value::Str(recv.type_name()));
    }
    // boss.is_a(Enemy): made from Enemy, or from a type built from it
    if name == "is_a" {
        let Some(Value::Data(d)) = args.first() else { return err(it, "is_a needs a data type, like  boss.is_a(Enemy)") };
        return Ok(Value::Bool(match &recv {
            Value::Obj(o) => o.class.as_ref().map_or(o.type_name == d.name, |c| c.is_a(&d.name)),
            _ => false,
        }));
    }
    let r = match &recv {
        Value::Str(s) => match string_method(it, s, name, &args)? {
            Some(v) => Ok(Some(v)),
            None => crate::tools::string_method(it, s, name, &args),
        },
        Value::Num(n) => match number_method(it, *n, name, &args)? {
            Some(v) => Ok(Some(v)),
            None => crate::tools::number_method(it, *n, name, &args),
        },
        Value::List(l) => match list_method(it, l, name, &args)? {
            Some(v) => Ok(Some(v)),
            None => crate::tools::list_method(it, l, name, &args),
        },
        Value::Color(c) => color_method(it, **c, name, &args),
        Value::Image(img) => image_method(it, img, name, &args),
        Value::Obj(o) if o.type_name == "date" => crate::tools::date_method(it, o, name, &args),
        Value::Obj(o) if o.type_name == "database" => crate::tools::db_method(it, o, name, &args),
        Value::Obj(o) => match name {
            "pressed" | "held" | "released" if o.type_name == "Keyboard" => {
                let k = arg_str(it, &args, 0, name)?.to_lowercase();
                let set = match name {
                    "pressed" => &it.keys_pressed,
                    "held" => &it.keys_held,
                    _ => &it.keys_released,
                };
                Ok(Some(Value::Bool(set.contains(&k))))
            }
            // dictionaries
            "keys" if o.type_name == "dict" => Ok(Some(Value::list(o.fields.iter().map(|(k, _)| Value::Str(k.clone())).collect()))),
            "values" if o.type_name == "dict" => Ok(Some(Value::list(o.fields.iter().map(|(_, v)| v.clone()).collect()))),
            "length" if o.type_name == "dict" => Ok(Some(Value::Num(o.fields.len() as f64))),
            "has" if o.type_name == "dict" => {
                let k = key_str(arg(it, &args, 0, name)?);
                Ok(Some(Value::Bool(o.get(&k).is_some())))
            }
            "get" if o.type_name == "dict" => {
                let k = key_str(arg(it, &args, 0, name)?);
                Ok(Some(o.get(&k).cloned().unwrap_or_else(|| args.get(1).cloned().unwrap_or(Value::None))))
            }
            "remove" if o.type_name == "dict" => {
                let k = key_str(arg(it, &args, 0, name)?);
                let mut n = (**o).clone();
                n.fields.retain(|(f, _)| *f != k);
                Ok(Some(Value::obj(n)))
            }
            // stacks and queues
            "peek" | "length" | "empty" if o.type_name == "stack" || o.type_name == "queue" => {
                let items = match o.get("items") {
                    Some(Value::List(l)) => l.clone(),
                    _ => std::rc::Rc::new(vec![]),
                };
                Ok(Some(match name {
                    "length" => Value::Num(items.len() as f64),
                    "empty" => Value::Bool(items.is_empty()),
                    // the item `pop` would give next
                    _ if o.type_name == "queue" => items.first().cloned().unwrap_or(Value::None),
                    _ => items.last().cloned().unwrap_or(Value::None),
                }))
            }
            // 2D camera: screen pixels (top-left origin) <-> world (y up)
            "to_world" | "to_screen" if o.type_name == "camera2d" => {
                let p = vec_arg(it, &args, 0, name)?;
                if p.len() != 2 {
                    return err(it, format!(".{} needs a 2D point like [x, y]", name));
                }
                let cam = crate::two_d::vec2(o.get("position")).unwrap_or([0.0, 0.0]);
                let zoom = match o.get("zoom") {
                    Some(Value::Num(z)) if *z != 0.0 => *z,
                    _ => 1.0,
                };
                let (w, h) = match it.global("screen") {
                    Some(Value::Obj(s)) => (
                        s.get("width").and_then(|v| v.as_num(0).ok()).unwrap_or(1280.0),
                        s.get("height").and_then(|v| v.as_num(0).ok()).unwrap_or(720.0),
                    ),
                    _ => (1280.0, 720.0),
                };
                let out = if name == "to_world" {
                    [cam[0] + (p[0] - w / 2.0) / zoom, cam[1] + (h / 2.0 - p[1]) / zoom]
                } else {
                    [(p[0] - cam[0]) * zoom + w / 2.0, h / 2.0 - (p[1] - cam[1]) * zoom]
                };
                Ok(Some(Value::list(vec![Value::Num(out[0]), Value::Num(out[1])])))
            }
            // walls.find_path(from, to) (the older name path_to still works)
            "find_path" | "path_to" if o.type_name == "tilemap" => {
                // level.path_to(from, to [, diagonal]): world points to walk through, around solid tiles
                let (a, b) = (vec_arg(it, &args, 0, name)?, vec_arg(it, &args, 1, name)?);
                if a.len() != 2 || b.len() != 2 {
                    return err(it, ".path_to needs two 2D points, like  level.path_to(slime.position, hero.position)");
                }
                let diagonal = args.get(2).map_or(false, |v| v.truthy());
                let Some(Value::List(grid)) = o.get("grid") else { return Ok(Value::None) };
                let p = crate::two_d::vec2(o.get("position")).unwrap_or([0.0, 0.0]);
                let ts = match o.get("tile_size") {
                    Some(Value::Num(t)) if *t > 0.0 => *t,
                    _ => 32.0,
                };
                let solid = !matches!(o.get("solid"), Some(Value::Bool(false)));
                let rows: Vec<Vec<bool>> = grid
                    .iter()
                    .map(|r| match r {
                        Value::List(cells) => cells.iter().map(|c| solid && matches!(c, Value::Num(n) if *n >= 0.0)).collect(),
                        _ => vec![],
                    })
                    .collect();
                let to_cell = |q: &[f64]| (((q[0] - p[0]) / ts).floor() as i64, ((p[1] - q[1]) / ts).floor() as i64);
                let (h, w) = (rows.len(), rows.iter().map(|r| r.len()).max().unwrap_or(0));
                let blocked = |c: i64, r: i64| rows.get(r as usize).and_then(|row| row.get(c as usize)).copied().unwrap_or(false);
                Ok(Some(match astar(w, h, to_cell(&a), to_cell(&b), diagonal, blocked) {
                    Some(path) => Value::list(
                        path.into_iter()
                            .map(|(c, r)| Value::list(vec![Value::Num(p[0] + (c as f64 + 0.5) * ts), Value::Num(p[1] - (r as f64 + 0.5) * ts)]))
                            .collect(),
                    ),
                    None => Value::None,
                }))
            }
            "tile_at" if o.type_name == "tilemap" => {
                let p = vec_arg(it, &args, 0, name)?;
                if p.len() != 2 {
                    return err(it, ".tile_at needs a 2D point like [x, y]");
                }
                Ok(Some(Value::Num(crate::two_d::tile_at(o, [p[0], p[1]]) as f64)))
            }
            // .tweening (the older names .animating and .is_tweening still work)
            "tweening" | "animating" | "is_tweening" => Ok(Some(o.get("tweening").cloned().unwrap_or(Value::Bool(false)))),
            "pressed" | "held" | "released" if o.type_name == "Mouse" => {
                let k = format!("mouse:{}", arg_str(it, &args, 0, name)?.to_lowercase());
                let set = match name {
                    "pressed" => &it.keys_pressed,
                    "held" => &it.keys_held,
                    _ => &it.keys_released,
                };
                Ok(Some(Value::Bool(set.contains(&k))))
            }
            "hover" | "pressed" => Ok(Some(o.get(name).cloned().unwrap_or(Value::Bool(false)))),
            // a.touches(b) (the older name collides_with still works)
            "touches" | "collides_with" => {
                let target = arg(it, &args, 0, name)?.clone();
                Ok(Some(Value::Bool(collides(it, o, &target)?)))
            }
            "click" => match o.get("on_click") {
                Some(f) => {
                    let f = f.clone();
                    return it.call_value(f, vec![], vec![], vec![]);
                }
                None => return err(it, format!("{} has no 'then' action to click", o.type_name)),
            },
            _ => Ok(None),
        },
        _ => Ok(None),
    };
    match r? {
        Some(v) => Ok(v),
        None => {
            // "did you mean" among the object's own names
            let hint = match &recv {
                Value::Obj(o) => {
                    let names: Vec<String> = o.fields.iter().map(|(k, _)| k.clone()).collect();
                    crate::suggest::closest(name, &names).map(|c| format!(" - did you mean '{}'?", c)).unwrap_or_default()
                }
                _ => String::new(),
            };
            if matches!(&recv, Value::Obj(o) if o.type_name == "dict") && !called {
                return err(it, format!("there's no key \"{}\"{}", name, hint));
            }
            let what = if called { "method" } else { "property or method" };
            err(it, format!("{} has no {} '.{}'{}", recv.type_name(), what, name, hint))
        }
    }
}

trait OptR {
    fn some(self) -> R<Option<Value>>;
}
impl OptR for Value {
    fn some(self) -> R<Option<Value>> {
        Ok(Some(self))
    }
}

fn string_method(it: &Interp, s: &str, name: &str, args: &[Value]) -> R<Option<Value>> {
    let st = |x: String| Value::Str(x);
    match name {
        "upper" => st(s.to_uppercase()).some(),
        "lower" => st(s.to_lowercase()).some(),
        "length" => Value::Num(s.chars().count() as f64).some(),
        "trim" => st(s.trim().into()).some(),
        "trim_left" | "trimleft" => st(s.trim_start().into()).some(),
        "trim_right" | "trimright" => st(s.trim_end().into()).some(),
        "reverse" => st(s.chars().rev().collect()).some(),
        "capitalize" => {
            let mut c = s.chars();
            st(match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            })
            .some()
        }
        "split" => {
            let sep = if args.is_empty() { " ".to_string() } else { arg_str(it, args, 0, name)? };
            let parts: Vec<Value> = if sep.is_empty() {
                s.chars().map(|c| st(c.to_string())).collect()
            } else {
                s.split(sep.as_str()).map(|p| st(p.to_string())).collect()
            };
            Value::list(parts).some()
        }
        "replace" => st(s.replace(&arg_str(it, args, 0, name)?, &arg_str(it, args, 1, name)?)).some(),
        "contains" => Value::Bool(s.contains(&arg_str(it, args, 0, name)?)).some(),
        "repeat" => st(s.repeat(arg_num(it, args, 0, name)?.max(0.0) as usize)).some(),
        "starts_with" => Value::Bool(s.starts_with(&arg_str(it, args, 0, name)?)).some(),
        "ends_with" => Value::Bool(s.ends_with(&arg_str(it, args, 0, name)?)).some(),
        _ => Ok(None),
    }
}

fn int_of(it: &Interp, n: f64, what: &str) -> R<i64> {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        Ok(n as i64)
    } else {
        err(it, format!(".{} needs whole numbers, got {}", what, fmt_num(n)))
    }
}

fn number_method(it: &Interp, n: f64, name: &str, args: &[Value]) -> R<Option<Value>> {
    // bitwise helpers (the same as the & | ^ << >> ~ operators)
    match name {
        "bit_and" | "bit_or" | "bit_xor" | "shift_left" | "shift_right" => {
            let (a, b) = (int_of(it, n, name)?, int_of(it, arg_num(it, args, 0, name)?, name)?);
            if name.starts_with("shift") && !(0..64).contains(&b) {
                return err(it, "a shift amount must be between 0 and 63");
            }
            let r = match name {
                "bit_and" => a & b,
                "bit_or" => a | b,
                "bit_xor" => a ^ b,
                "shift_left" => a << b,
                _ => a >> b,
            };
            return Value::Num(r as f64).some();
        }
        "bit_not" => return Value::Num(!int_of(it, n, name)? as f64).some(),
        "bit" => {
            let i = int_of(it, arg_num(it, args, 0, name)?, name)?;
            if !(0..64).contains(&i) {
                return err(it, ".bit needs a position between 0 and 63");
            }
            return Value::Bool((int_of(it, n, name)? >> i) & 1 == 1).some();
        }
        "to_binary" => return Value::Str(format!("{:b}", int_of(it, n, name)?)).some(),
        "to_hex" => return Value::Str(format!("{:X}", int_of(it, n, name)?)).some(),
        _ => {}
    }
    let v = match name {
        "abs" => n.abs(),
        "floor" => n.floor(),
        "ceil" => n.ceil(),
        "round" => n.round(),
        "sqrt" => {
            if n < 0.0 {
                return err(it, "can't take the square root of a negative number");
            }
            n.sqrt()
        }
        "pow" => n.powf(arg_num(it, args, 0, name)?),
        "clamp" => {
            let (lo, hi) = (arg_num(it, args, 0, name)?, arg_num(it, args, 1, name)?);
            n.max(lo).min(hi)
        }
        "min" => n.min(arg_num(it, args, 0, name)?),
        "max" => n.max(arg_num(it, args, 0, name)?),
        _ => return Ok(None),
    };
    Value::Num(v).some()
}

fn sort_list(it: &Interp, v: &mut [(Value, Value)]) -> R<()> {
    let mut bad = None;
    v.sort_by(|a, b| {
        cmp_values(&a.0, &b.0).unwrap_or_else(|| {
            bad = Some((a.0.type_name(), b.0.type_name()));
            std::cmp::Ordering::Equal
        })
    });
    match bad {
        Some((a, b)) => err(it, format!("can't sort a list that mixes {} and {}", a, b)),
        None => Ok(()),
    }
}

fn list_method(it: &mut Interp, l: &[Value], name: &str, args: &[Value]) -> R<Option<Value>> {
    let nums = |it: &Interp| -> R<Vec<f64>> { l.iter().map(|v| v.as_num(it.line)).collect() };
    match name {
        "length" => Value::Num(l.len() as f64).some(),
        "first" => l.first().cloned().unwrap_or(Value::None).some(),
        "last" => l.last().cloned().unwrap_or(Value::None).some(),
        "x" | "y" | "z" | "w" => {
            let i = axis(name).unwrap();
            match l.get(i) {
                Some(v) => v.clone().some(),
                None => err(it, format!("this list has no .{} (it only has {} items)", name, l.len())),
            }
        }
        "reverse" => Value::list(l.iter().rev().cloned().collect()).some(),
        "sort" => {
            let mut pairs: Vec<(Value, Value)> = l.iter().map(|v| (v.clone(), v.clone())).collect();
            sort_list(it, &mut pairs)?;
            Value::list(pairs.into_iter().map(|p| p.1).collect()).some()
        }
        "sort_by" | "sortBy" => {
            let f = arg(it, args, 0, name)?.clone();
            let mut pairs = vec![];
            for v in l {
                let key = it.call_value(f.clone(), vec![v.clone()], vec![], vec![])?;
                pairs.push((key, v.clone()));
            }
            sort_list(it, &mut pairs)?;
            Value::list(pairs.into_iter().map(|p| p.1).collect()).some()
        }
        "sum" => Value::Num(nums(it)?.iter().sum()).some(),
        "repeat" => {
            let n = arg_num(it, args, 0, name)?.max(0.0) as usize;
            Value::list(l.iter().cycle().take(l.len() * n).cloned().collect()).some()
        }
        // 2D vectors
        "angle" => {
            let v = nums(it)?;
            if v.len() != 2 {
                return err(it, ".angle needs a 2D vector like [3, 4]");
            }
            Value::Num(v[1].atan2(v[0]).to_degrees()).some()
        }
        "rotate" => {
            let v = nums(it)?;
            if v.len() != 2 {
                return err(it, ".rotate needs a 2D vector like [3, 4]");
            }
            let (s, c) = arg_num(it, args, 0, name)?.to_radians().sin_cos();
            Value::list(vec![Value::Num(v[0] * c - v[1] * s), Value::Num(v[0] * s + v[1] * c)]).some()
        }
        "magnitude" => Value::Num(nums(it)?.iter().map(|x| x * x).sum::<f64>().sqrt()).some(),
        // pos.move_toward(target, step): at most `step` closer, never past it
        "move_toward" => {
            let (a, b) = (nums(it)?, vec_arg(it, args, 0, name)?);
            if a.len() != b.len() {
                return err(it, ".move_toward needs two vectors of the same size");
            }
            let step = arg_num(it, args, 1, name)?;
            let d: Vec<f64> = a.iter().zip(&b).map(|(x, y)| y - x).collect();
            let len = d.iter().map(|x| x * x).sum::<f64>().sqrt();
            let out: Vec<Value> = if len <= step || len == 0.0 {
                b.iter().map(|x| Value::Num(*x)).collect()
            } else {
                a.iter().zip(&d).map(|(x, dx)| Value::Num(x + dx / len * step)).collect()
            };
            Value::list(out).some()
        }
        "normalize" => {
            let v = nums(it)?;
            let m = v.iter().map(|x| x * x).sum::<f64>().sqrt();
            Value::list(v.iter().map(|x| Value::Num(if m == 0.0 { 0.0 } else { x / m })).collect()).some()
        }
        "dot" => {
            let (a, b) = (nums(it)?, vec_arg(it, args, 0, name)?);
            if a.len() != b.len() {
                return err(it, ".dot needs two vectors of the same size");
            }
            Value::Num(a.iter().zip(&b).map(|(x, y)| x * y).sum()).some()
        }
        "cross" => {
            let (a, b) = (nums(it)?, vec_arg(it, args, 0, name)?);
            if a.len() != 3 || b.len() != 3 {
                return err(it, ".cross needs two vectors with 3 numbers");
            }
            Value::list(vec![
                Value::Num(a[1] * b[2] - a[2] * b[1]),
                Value::Num(a[2] * b[0] - a[0] * b[2]),
                Value::Num(a[0] * b[1] - a[1] * b[0]),
            ])
            .some()
        }
        "min" | "max" => {
            if l.is_empty() {
                return Value::None.some();
            }
            let mut best = l[0].clone();
            for v in &l[1..] {
                let o = cmp_values(v, &best)
                    .ok_or_else(|| EzaError::runtime(it.line, format!(".{} needs numbers or text", name)))?;
                if (name == "min" && o.is_lt()) || (name == "max" && o.is_gt()) {
                    best = v.clone();
                }
            }
            best.some()
        }
        "contains" => {
            let x = arg(it, args, 0, name)?;
            Value::Bool(l.iter().any(|v| equals(v, x))).some()
        }
        "add" => {
            let mut v = l.to_vec();
            v.push(arg(it, args, 0, name)?.clone());
            Value::list(v).some()
        }
        "remove" => {
            let x = arg(it, args, 0, name)?;
            let mut v = l.to_vec();
            if let Some(i) = v.iter().position(|y| equals(y, x)) {
                v.remove(i);
            }
            Value::list(v).some()
        }
        "join" => {
            let sep = if args.is_empty() { String::new() } else { arg_str(it, args, 0, name)? };
            Value::Str(l.iter().map(|v| v.display()).collect::<Vec<_>>().join(&sep)).some()
        }
        _ => Ok(None),
    }
}

fn rgb_to_hsl(c: [f64; 4]) -> (f64, f64, f64) {
    let (r, g, b) = (c[0] / 255.0, c[1] / 255.0, c[2] / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    if mx == mn {
        return (0.0, 0.0, l);
    }
    let d = mx - mn;
    let s = if l > 0.5 { d / (2.0 - mx - mn) } else { d / (mx + mn) };
    let h = if mx == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l * 255.0, l * 255.0, l * 255.0);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        255.0
            * if t < 1.0 / 6.0 {
                p + (q - p) * 6.0 * t
            } else if t < 0.5 {
                q
            } else if t < 2.0 / 3.0 {
                p + (q - p) * (2.0 / 3.0 - t) * 6.0
            } else {
                p
            }
    };
    (f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0))
}

fn color_method(it: &Interp, c: [f64; 4], name: &str, args: &[Value]) -> R<Option<Value>> {
    let col = |r: f64, g: f64, b: f64, a: f64| Value::Color(Rc::new([r.clamp(0.0, 255.0), g.clamp(0.0, 255.0), b.clamp(0.0, 255.0), a.clamp(0.0, 1.0)]));
    match name {
        "r" => Value::Num(c[0].round()).some(),
        "g" => Value::Num(c[1].round()).some(),
        "b" => Value::Num(c[2].round()).some(),
        "a" => Value::Num(c[3]).some(),
        "hex" => Value::Str(color_hex(c)).some(),
        "lighten" => {
            let t = arg_num(it, args, 0, name)?;
            col(c[0] + (255.0 - c[0]) * t, c[1] + (255.0 - c[1]) * t, c[2] + (255.0 - c[2]) * t, c[3]).some()
        }
        "darken" => {
            let t = 1.0 - arg_num(it, args, 0, name)?;
            col(c[0] * t, c[1] * t, c[2] * t, c[3]).some()
        }
        "mix" => {
            let o = match arg(it, args, 0, name)? {
                Value::Color(o) => **o,
                v => return err(it, format!(".mix needs a color, got {}", v.type_name())),
            };
            let t = if args.len() > 1 { arg_num(it, args, 1, name)? } else { 0.5 };
            let m = |a: f64, b: f64| a + (b - a) * t;
            col(m(c[0], o[0]), m(c[1], o[1]), m(c[2], o[2]), m(c[3], o[3])).some()
        }
        "invert" => col(255.0 - c[0], 255.0 - c[1], 255.0 - c[2], c[3]).some(),
        "saturate" => {
            let (h, s, l) = rgb_to_hsl(c);
            let (r, g, b) = hsl_to_rgb(h, (s + arg_num(it, args, 0, name)?).clamp(0.0, 1.0), l);
            col(r, g, b, c[3]).some()
        }
        _ => Ok(None),
    }
}

fn vec3_of(v: Option<&Value>) -> Option<[f64; 3]> {
    if let Some(Value::List(l)) = v {
        if let (Some(Value::Num(x)), Some(Value::Num(y)), Some(Value::Num(z))) = (l.first(), l.get(1), l.get(2)) {
            return Some([*x, *y, *z]);
        }
    }
    None
}

/// Axis-aligned box (center, half size) from position, size props and scale.
pub fn aabb(o: &Obj) -> Option<([f64; 3], [f64; 3])> {
    aabb_with(o, false)
}

/// `physical` boxes are what physics collides with (a plane is thin); the default boxes are generous for `collides_with`.
pub fn aabb_with(o: &Obj, physical: bool) -> Option<([f64; 3], [f64; 3])> {
    let pos = vec3_of(o.get("position"))?;
    let scale = vec3_of(o.get("scale")).unwrap_or([1.0; 3]);
    let n = |k: &str, d: f64| match o.get(k) {
        Some(Value::Num(x)) => *x,
        _ => d,
    };
    let (w, h) = (n("width", 1.0), n("height", 1.0));
    let half = if let Some(Value::Num(r)) = o.get("radius") {
        [*r; 3]
    } else {
        match o.type_name.as_str() {
            "plane" => {
                let amp = match o.get("noise") {
                    Some(Value::Obj(nz)) => match nz.get("amp") {
                        Some(Value::Num(a)) => a.abs(),
                        _ => 0.0,
                    },
                    _ => 0.0,
                };
                // a plane's height is its depth; the extra 0.5 means standing on it counts as touching
                [w / 2.0, if physical { 0.05 } else { 0.55 + amp }, h / 2.0]
            }
            "sphere" => [w / 2.0; 3],
            "cube" => [w / 2.0, h / 2.0, n("depth", w) / 2.0],
            "cylinder" => [w / 2.0, h / 2.0, w / 2.0],
            _ => [0.4, 0.9, 0.4], // capsule-sized entity
        }
    };
    Some((pos, [half[0] * scale[0].abs(), half[1] * scale[1].abs(), half[2] * scale[2].abs()]))
}

pub fn collides(it: &Interp, a: &Obj, target: &Value) -> R<bool> {
    match target {
        Value::List(items) => {
            for t in items.iter() {
                if collides(it, a, t)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Value::Entity(h) => {
            let inner = h.var.borrow().get().clone();
            collides(it, a, &inner)
        }
        Value::Obj(b) => {
            if matches!(a.get("destroyed"), Some(Value::Bool(true))) || matches!(b.get("destroyed"), Some(Value::Bool(true))) {
                return Ok(false);
            }
            if crate::two_d::is_2d(a) || crate::two_d::is_2d(b) {
                return Ok(crate::two_d::overlap(a, b));
            }
            let (Some((pa, ha)), Some((pb, hb))) = (aabb(a), aabb(b)) else {
                return err(it, "collides_with needs objects that have a position");
            };
            Ok((0..3).all(|i| (pa[i] - pb[i]).abs() <= ha[i] + hb[i]))
        }
        v => err(it, format!("collides_with needs an object or a list of objects, got {}", v.type_name())),
    }
}

fn image_method(it: &Interp, img: &ImageData, name: &str, args: &[Value]) -> R<Option<Value>> {
    match name {
        "width" => Value::Num(img.width as f64).some(),
        "height" => Value::Num(img.height as f64).some(),
        "size" => Value::list(vec![Value::Num(img.width as f64), Value::Num(img.height as f64)]).some(),
        "path" => Value::Str(img.path.clone()).some(),
        "pixel" | "get_pixel" => {
            let (x, y) = (arg_num(it, args, 0, name)?, arg_num(it, args, 1, name)?);
            if x < 0.0 || y < 0.0 || x >= img.width as f64 || y >= img.height as f64 || x.fract() != 0.0 || y.fract() != 0.0 {
                return err(it, format!("pixel ({}, {}) is outside the {}x{} image", fmt_num(x), fmt_num(y), img.width, img.height));
            }
            let i = ((y as usize) * img.width as usize + x as usize) * 4;
            let p = &img.rgba[i..i + 4];
            Value::Color(Rc::new([p[0] as f64, p[1] as f64, p[2] as f64, p[3] as f64 / 255.0])).some()
        }
        _ => Ok(None),
    }
}

/// A* over a grid of cells (column, row): the cells to walk through after `from`, ending at `to`.
/// None when there's no way through (or `to` is a wall). Diagonal steps never cut a wall's corner.
fn astar(w: usize, h: usize, from: (i64, i64), to: (i64, i64), diagonal: bool, blocked: impl Fn(i64, i64) -> bool) -> Option<Vec<(i64, i64)>> {
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashMap};
    let inside = |c: i64, r: i64| c >= 0 && r >= 0 && (c as usize) < w && (r as usize) < h;
    if !inside(to.0, to.1) || blocked(to.0, to.1) || !inside(from.0, from.1) {
        return None;
    }
    if from == to {
        return Some(vec![]);
    }
    // costs in tenths: 10 straight, 14 diagonal
    let guess = |c: i64, r: i64| {
        let (dx, dy) = ((c - to.0).abs(), (r - to.1).abs());
        if diagonal { 10 * dx.max(dy) + 4 * dx.min(dy) } else { 10 * (dx + dy) }
    };
    let mut open = BinaryHeap::new();
    let mut cost: HashMap<(i64, i64), i64> = HashMap::new();
    let mut came: HashMap<(i64, i64), (i64, i64)> = HashMap::new();
    cost.insert(from, 0);
    open.push(Reverse((guess(from.0, from.1), 0i64, from)));
    let mut steps: Vec<(i64, i64, i64)> = vec![(1, 0, 10), (-1, 0, 10), (0, 1, 10), (0, -1, 10)];
    if diagonal {
        steps.extend([(1, 1, 14), (1, -1, 14), (-1, 1, 14), (-1, -1, 14)]);
    }
    while let Some(Reverse((_, g, cur))) = open.pop() {
        if cur == to {
            let mut path = vec![to];
            let mut at = to;
            while let Some(&prev) = came.get(&at) {
                if prev == from {
                    break;
                }
                path.push(prev);
                at = prev;
            }
            path.reverse();
            return Some(path);
        }
        if g > *cost.get(&cur).unwrap_or(&i64::MAX) {
            continue;
        }
        for &(dx, dy, c) in &steps {
            let (nc, nr) = (cur.0 + dx, cur.1 + dy);
            if !inside(nc, nr) || blocked(nc, nr) {
                continue;
            }
            if dx != 0 && dy != 0 && (blocked(cur.0 + dx, cur.1) || blocked(cur.0, cur.1 + dy)) {
                continue;
            }
            let ng = g + c;
            if ng < *cost.get(&(nc, nr)).unwrap_or(&i64::MAX) {
                cost.insert((nc, nr), ng);
                came.insert((nc, nr), cur);
                open.push(Reverse((ng + guess(nc, nr), ng, (nc, nr))));
            }
        }
    }
    None
}

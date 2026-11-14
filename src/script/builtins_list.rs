//! The built-in library, part three: vectors and the iterator methods.
//!
//! An iterator is just a Vec made all at once, so `v.iter().map(f).sum()`
//! walks `v` twice. Every element passed to a closure is a copy (cheap:
//! big values are shared until one side changes).

use crate::script::ast::Type;
use crate::script::host::{int, want};
use crate::script::interp::{Flow, Interp, Res};
use crate::script::value::*;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

macro_rules! tri {
    ($s:ident, $e:expr) => {
        match $e {
            Ok(x) => x,
            Err(m) => return Some($s.fail(m)),
        }
    };
}

macro_rules! trf {
    ($e:expr) => {
        match $e {
            Ok(x) => x,
            Err(f) => return Some(Err(f)),
        }
    };
}

type Cmp<'c, 'a> = &'c mut dyn FnMut(&mut Interp<'a>, &Value, &Value) -> Result<Ordering, Flow>;

impl<'a> Interp<'a> {
    /// A list argument: a Vec, or a range written out.
    fn list_arg(&mut self, v: &Value) -> Result<Rc<List>, Flow> {
        match v {
            Value::Vec(x) => Ok(x.clone()),
            Value::Range(r) => {
                let (Some(lo), Some(hi)) = (r.lo, r.hi) else { return self.fail("an open range cannot be used as a list") };
                let hi = if r.incl { hi.saturating_add(1) } else { hi };
                let n = if hi > lo { hi.wrapping_sub(lo) as u64 } else { 0 }; // (exact: the difference fits a u64)
                if n > self.limits.max_vec as u64 {
                    return self.fail(format!("a Vec may not hold more than {} elements", self.limits.max_vec));
                }
                self.charge(n as usize + 1)?;
                Ok(Rc::new(List((lo..hi.max(lo)).map(|i| if r.chars { char::from_u32(i as u32).map_or(Value::Int(i), Value::Char) } else { Value::Int(i) }).collect())))
            }
            other => self.fail(format!("expected a list here, found {}", other.type_name())),
        }
    }

    /// Stable merge sort with a comparison that may fail.
    pub(crate) fn merge_sort(&mut self, items: Vec<Value>, cmp: Cmp<'_, 'a>) -> Result<Vec<Value>, Flow> {
        let n = items.len();
        if n < 2 {
            return Ok(items);
        }
        let mut src = items;
        let mut dst: Vec<Value> = Vec::with_capacity(n);
        let mut width = 1;
        while width < n {
            dst.clear();
            let mut i = 0;
            while i < n {
                let mid = (i + width).min(n);
                let hi = (i + 2 * width).min(n);
                let (mut a, mut b) = (i, mid);
                while a < mid && b < hi {
                    if cmp(self, &src[b], &src[a])? == Ordering::Less {
                        dst.push(src[b].clone());
                        b += 1;
                    } else {
                        dst.push(src[a].clone());
                        a += 1;
                    }
                }
                dst.extend_from_slice(&src[a..mid]);
                dst.extend_from_slice(&src[b..hi]);
                i += 2 * width;
            }
            core::mem::swap(&mut src, &mut dst);
            width *= 2;
        }
        Ok(src)
    }

    fn order_of(&self, r: Value) -> Result<Ordering, Flow> {
        match self.deref(r)?.as_ordering() {
            Some(o) => Ok(o),
            None => self.fail("a comparison closure must answer with an Ordering (Less, Equal, Greater)"),
        }
    }

    fn sort_natural(&mut self, items: Vec<Value>) -> Result<Vec<Value>, Flow> {
        self.work(items.len() * 8)?;
        self.merge_sort(items, &mut |it, a, b| match cmp_values(a, b) {
            Some(o) => Ok(o),
            None => it.fail(format!("cannot sort: {} cannot be compared with {}", a.type_name(), b.type_name())),
        })
    }

    fn sort_with(&mut self, items: Vec<Value>, f: Value) -> Result<Vec<Value>, Flow> {
        self.work(items.len() * 8)?;
        self.merge_sort(items, &mut |it, a, b| {
            let r = it.call2(&f, a.clone(), b.clone())?;
            it.order_of(r)
        })
    }

    fn sort_keyed(&mut self, items: Vec<Value>, f: Value) -> Result<Vec<Value>, Flow> {
        self.work(items.len() * 8)?;
        let mut pairs = Vec::with_capacity(items.len());
        for x in items {
            let k = self.call1(&f, x.clone())?;
            pairs.push(Value::tuple(alloc::vec![k, x]));
        }
        let sorted = self.merge_sort(pairs, &mut |it, a, b| match (a, b) {
            (Value::Tuple(p), Value::Tuple(q)) => match cmp_values(&p[0], &q[0]) {
                Some(o) => Ok(o),
                None => it.fail("cannot sort by keys that cannot be compared"),
            },
            _ => it.fail("internal error in sort_by_key"),
        })?;
        Ok(sorted.into_iter().map(|p| if let Value::Tuple(t) = p { t[1].clone() } else { Value::Unit }).collect())
    }

    pub(crate) fn vec_methods(&mut self, items: &Rc<List>, place: Option<&Place>, name: &str, args: &[Value], tf: Option<&Type>) -> Option<Res> {
        if !matches!(name, "len" | "is_empty" | "first" | "last" | "get" | "get_mut" | "first_mut" | "last_mut") {
            trf!(self.work(items.len()));
        }
        let n = items.len();
        let ok =|x: Value| -> Option<Res> { Some(Ok(x)) };
        let elem_ref = |k: usize| -> Value {
            match place {
                Some(p) => {
                    let mut q = p.clone();
                    q.path.push(Elem::Index(k));
                    Value::Ref(Rc::new(q))
                }
                None => items[k].clone(),
            }
        };
        match name {
            "len" | "count" => ok(Value::Int(n as i64)),
            "is_empty" => ok(Value::Bool(n == 0)),
            "first" => ok(Value::option(items.first().cloned())),
            "last" => ok(Value::option(items.last().cloned())),
            "first_mut" => ok(Value::option(if n > 0 { Some(elem_ref(0)) } else { None })),
            "last_mut" => ok(Value::option(if n > 0 { Some(elem_ref(n - 1)) } else { None })),
            "get" | "get_mut" => {
                tri!(self, want(name, args, 1));
                match &args[0] {
                    Value::Int(i) => ok(Value::option(if *i >= 0 && (*i as usize) < n { Some(if name == "get" { items[*i as usize].clone() } else { elem_ref(*i as usize) }) } else { None })),
                    Value::Range(r) => match self.range_bounds(r, n) {
                        Ok((lo, hi)) => ok(Value::some(Value::vec(items[lo..hi].to_vec()))),
                        Err(_) => ok(Value::none()),
                    },
                    other => Some(self.fail(format!("`get` needs a position, found {}", other.type_name()))),
                }
            }
            "iter" | "into_iter" | "peekable" | "by_ref" | "drain_all" => ok(Value::Vec(items.clone())),
            "iter_mut" | "values_mut" => match place {
                Some(_) => ok(Value::vec((0..n).map(elem_ref).collect())),
                None => Some(self.fail("`iter_mut` needs a variable to change")),
            },
            "contains" => {
                tri!(self, want(name, args, 1));
                ok(Value::Bool(items.iter().any(|x| eq_values(x, &args[0]))))
            }
            "join" => {
                tri!(self, want(name, args, 1));
                let sep = match &args[0] {
                    Value::Str(s) => s.clone(),
                    other => return Some(self.fail(format!("`join` needs a string to put between, found {}", other.type_name()))),
                };
                let mut out = String::new();
                for (k, x) in items.iter().enumerate() {
                    if k > 0 {
                        out.push_str(&sep);
                    }
                    if !x.display(&mut out) {
                        return Some(self.fail(format!("`join` cannot show a {}", x.type_name())));
                    }
                    trf!(self.check_string(out.len()));
                }
                let r = Value::string(out);
                self.lift_value(r)
            }
            "concat" => {
                if items.iter().all(|x| matches!(x, Value::Str(_))) {
                    let mut out = String::new();
                    for x in items.iter() {
                        x.display(&mut out);
                        trf!(self.check_string(out.len()));
                    }
                    return self.lift_value(Value::string(out));
                }
                let mut out: Vec<Value> = Vec::new();
                for x in items.iter() {
                    match x {
                        Value::Vec(inner) => out.extend(inner.iter().cloned()),
                        other => return Some(self.fail(format!("`concat` needs lists or strings, found {}", other.type_name()))),
                    }
                    trf!(self.check_vec(out.len()));
                }
                self.lift_value(Value::vec(out))
            }
            "flatten" => {
                let mut out: Vec<Value> = Vec::new();
                for x in items.iter() {
                    match x {
                        Value::Vec(inner) => out.extend(inner.iter().cloned()),
                        Value::Enum(e) if &*e.ty == "Option" => out.extend(e.fields.iter().cloned()),
                        other => out.push(other.clone()),
                    }
                    trf!(self.check_vec(out.len()));
                }
                self.lift_value(Value::vec(out))
            }
            "rev" => {
                let mut out = items.to_vec();
                out.reverse();
                self.lift_value(Value::vec(out))
            }
            "enumerate" => {
                let out: Vec<Value> = items.iter().enumerate().map(|(i, x)| Value::tuple(alloc::vec![Value::Int(i as i64), x.clone()])).collect();
                self.lift_value(Value::vec(out))
            }
            "zip" => {
                tri!(self, want(name, args, 1));
                let other = trf!(self.list_arg(&args[0]));
                let out: Vec<Value> = items.iter().zip(other.iter()).map(|(a, b)| Value::tuple(alloc::vec![a.clone(), b.clone()])).collect();
                self.lift_value(Value::vec(out))
            }
            "chain" => {
                tri!(self, want(name, args, 1));
                let other = trf!(self.list_arg(&args[0]));
                let mut out = items.to_vec();
                out.extend(other.iter().cloned());
                trf!(self.check_vec(out.len()));
                self.lift_value(Value::vec(out))
            }
            "take" | "skip" | "step_by" => {
                tri!(self, want(name, args, 1));
                let k = tri!(self, int(name, args, 0));
                if k < 0 || (name == "step_by" && k == 0) {
                    return Some(self.fail(format!("`{}` needs a {} number", name, if name == "step_by" { "positive" } else { "non-negative" })));
                }
                let k = k as usize;
                let out: Vec<Value> = match name {
                    "take" => items.iter().take(k).cloned().collect(),
                    "skip" => items.iter().skip(k).cloned().collect(),
                    _ => items.iter().step_by(k).cloned().collect(),
                };
                self.lift_value(Value::vec(out))
            }
            "nth" => {
                tri!(self, want(name, args, 1));
                let k = tri!(self, int(name, args, 0));
                ok(Value::option(if k >= 0 { items.get(k as usize).cloned() } else { None }))
            }
            "windows" | "chunks" => {
                tri!(self, want(name, args, 1));
                let k = tri!(self, int(name, args, 0));
                if k <= 0 {
                    return Some(self.fail(format!("`{}` needs a size above 0", name)));
                }
                let k = k as usize;
                let out: Vec<Value> = if name == "windows" { items.windows(k).map(|w| Value::vec(w.to_vec())).collect() } else { items.chunks(k).map(|w| Value::vec(w.to_vec())).collect() };
                trf!(self.check_vec(out.len()));
                self.lift_value(Value::vec(out))
            }
            "split_at" => {
                tri!(self, want(name, args, 1));
                let k = tri!(self, int(name, args, 0));
                if k < 0 || k as usize > n {
                    return Some(self.fail("`split_at` is past the end"));
                }
                let (a, b) = items.split_at(k as usize);
                self.lift_value(Value::tuple(alloc::vec![Value::vec(a.to_vec()), Value::vec(b.to_vec())]))
            }
            "split_first" | "split_last" => {
                if n == 0 {
                    return ok(Value::none());
                }
                let (head, rest) = if name == "split_first" { (items[0].clone(), items[1..].to_vec()) } else { (items[n - 1].clone(), items[..n - 1].to_vec()) };
                self.lift_value(Value::some(Value::tuple(alloc::vec![head, Value::vec(rest)])))
            }
            "unzip" => {
                let (mut a, mut b) = (Vec::new(), Vec::new());
                for x in items.iter() {
                    match x {
                        Value::Tuple(t) if t.len() == 2 => {
                            a.push(t[0].clone());
                            b.push(t[1].clone());
                        }
                        _ => return Some(self.fail("`unzip` needs a list of pairs")),
                    }
                }
                self.lift_value(Value::tuple(alloc::vec![Value::vec(a), Value::vec(b)]))
            }
            "sum" | "product" => {
                let floaty = tf.map_or(false, |t| t.is_float()) || items.iter().any(|x| matches!(x, Value::Float(_)));
                let sum = name == "sum";
                if floaty {
                    let mut acc = if sum { 0.0 } else { 1.0 };
                    for x in items.iter() {
                        let f = match x {
                            Value::Float(f) => *f,
                            Value::Int(i) => *i as f64,
                            other => return Some(self.fail(format!("cannot {} a {}", name, other.type_name()))),
                        };
                        if sum {
                            acc += f;
                        } else {
                            acc *= f;
                        }
                    }
                    ok(Value::Float(acc))
                } else {
                    let mut acc: i64 = if sum { 0 } else { 1 };
                    for x in items.iter() {
                        let Value::Int(i) = x else { return Some(self.fail(format!("cannot {} a {}", name, x.type_name()))) };
                        let r = if sum { acc.checked_add(*i) } else { acc.checked_mul(*i) };
                        match r {
                            Some(r) => acc = r,
                            None => return Some(self.fail(format!("attempt to {} with overflow", if sum { "add" } else { "multiply" }))),
                        }
                    }
                    ok(Value::Int(acc))
                }
            }
            "min" | "max" if args.is_empty() => {
                let mut best: Option<&Value> = None;
                for x in items.iter() {
                    best = match best {
                        None => Some(x),
                        Some(b) => match cmp_values(x, b) {
                            Some(o) => {
                                if (name == "max" && o != Ordering::Less) || (name == "min" && o == Ordering::Less) {
                                    Some(x)
                                } else {
                                    Some(b)
                                }
                            }
                            None => return Some(self.fail(format!("cannot compare {} with {}", x.type_name(), b.type_name()))),
                        },
                    };
                }
                ok(Value::option(best.cloned()))
            }
            "collect" => self.collect_list(items, tf),
            // ---- the ones that call a closure
            "map" | "filter" | "filter_map" | "flat_map" | "for_each" | "any" | "all" | "find" | "find_map" | "position" | "take_while" | "skip_while" | "partition" | "max_by_key" | "min_by_key" | "max_by" | "min_by" | "fold" | "reduce" | "inspect" | "map_while" => self.list_closure_methods(items, name, args),
            _ => None,
        }
    }

    fn lift_value(&mut self, r: Value) -> Option<Res> {
        Some(self.lift(Ok(r)))
    }

    fn collect_list(&mut self, items: &Rc<List>, tf: Option<&Type>) -> Option<Res> {
        let want_ty = tf.map_or("", |t| &*t.name);
        match want_ty {
            "String" => {
                let mut out = String::new();
                for x in items.iter() {
                    if !matches!(x, Value::Char(_) | Value::Str(_)) {
                        return Some(self.fail(format!("cannot collect a {} into a String", x.type_name())));
                    }
                    x.display(&mut out);
                    trf!(self.check_string(out.len()));
                }
                self.lift_value(Value::string(out))
            }
            "Result" | "Option" => {
                let (good, bad) = if want_ty == "Result" { ("Ok", "Err") } else { ("Some", "None") };
                let mut out = Vec::with_capacity(items.len());
                for x in items.iter() {
                    match x {
                        Value::Enum(e) if &*e.variant == good => out.push(e.fields.first().cloned().unwrap_or(Value::Unit)),
                        Value::Enum(e) if &*e.variant == bad => return Some(Ok(x.clone())),
                        other => return Some(self.fail(format!("cannot collect a {} into a {}", other.type_name(), want_ty))),
                    }
                }
                let inner = Value::vec(out);
                self.lift_value(if want_ty == "Result" { Value::ok(inner) } else { Value::some(inner) })
            }
            _ => Some(Ok(Value::Vec(items.clone()))),
        }
    }

    fn list_closure_methods(&mut self, items: &Rc<List>, name: &str, args: &[Value]) -> Option<Res> {
        let need = if matches!(name, "fold") { 2 } else { 1 };
        if args.len() != need {
            return Some(self.fail(format!("`{}` takes {} argument(s) but {} were given", name, need, args.len())));
        }
        let f = args[need - 1].clone();
        let mut out: Vec<Value> = Vec::new();
        match name {
            "map" => {
                for x in items.iter() {
                    out.push(trf!(self.call1(&f, x.clone())));
                }
                self.lift_value(Value::vec(out))
            }
            "inspect" => {
                for x in items.iter() {
                    trf!(self.call1(&f, x.clone()));
                }
                Some(Ok(Value::Vec(items.clone())))
            }
            "for_each" => {
                for x in items.iter() {
                    trf!(self.call1(&f, x.clone()));
                }
                Some(Ok(Value::Unit))
            }
            "filter" | "take_while" | "skip_while" => {
                let mut skipping = true;
                for x in items.iter() {
                    let keep = trf!(self.call1(&f, x.clone()));
                    let keep = trf!(self.truthy(keep));
                    match name {
                        "filter" => {
                            if keep {
                                out.push(x.clone());
                            }
                        }
                        "take_while" => {
                            if !keep {
                                break;
                            }
                            out.push(x.clone());
                        }
                        _ => {
                            if skipping && keep {
                                continue;
                            }
                            skipping = false;
                            out.push(x.clone());
                        }
                    }
                }
                self.lift_value(Value::vec(out))
            }
            "filter_map" | "map_while" => {
                for x in items.iter() {
                    let r = trf!(self.call1(&f, x.clone()));
                    match r {
                        Value::Enum(e) if &*e.ty == "Option" && &*e.variant == "Some" && !e.fields.is_empty() => out.push(e.fields[0].clone()),
                        Value::Enum(e) if &*e.ty == "Option" && &*e.variant == "None" => {
                            if name == "map_while" {
                                break;
                            }
                        }
                        other => return Some(self.fail(format!("`{}` needs a closure that answers with an Option, not {}", name, other.type_name()))),
                    }
                }
                self.lift_value(Value::vec(out))
            }
            "flat_map" => {
                for x in items.iter() {
                    let r = trf!(self.call1(&f, x.clone()));
                    match r {
                        Value::Vec(v) => out.extend(v.iter().cloned()),
                        Value::Enum(e) if &*e.ty == "Option" => out.extend(e.fields.iter().cloned()),
                        other => out.push(other),
                    }
                    trf!(self.check_vec(out.len()));
                }
                self.lift_value(Value::vec(out))
            }
            "any" | "all" => {
                for x in items.iter() {
                    let r = trf!(self.call1(&f, x.clone()));
                    let r = trf!(self.truthy(r));
                    if r == (name == "any") {
                        return Some(Ok(Value::Bool(name == "any")));
                    }
                }
                Some(Ok(Value::Bool(name == "all")))
            }
            "find" | "position" => {
                for (k, x) in items.iter().enumerate() {
                    let r = trf!(self.call1(&f, x.clone()));
                    if trf!(self.truthy(r)) {
                        return Some(Ok(Value::some(if name == "find" { x.clone() } else { Value::Int(k as i64) })));
                    }
                }
                Some(Ok(Value::none()))
            }
            "find_map" => {
                for x in items.iter() {
                    let r = trf!(self.call1(&f, x.clone()));
                    if r.is_variant("Option", "Some") {
                        return Some(Ok(r));
                    }
                }
                Some(Ok(Value::none()))
            }
            "partition" => {
                let (mut yes, mut no) = (Vec::new(), Vec::new());
                for x in items.iter() {
                    let r = trf!(self.call1(&f, x.clone()));
                    if trf!(self.truthy(r)) {
                        yes.push(x.clone());
                    } else {
                        no.push(x.clone());
                    }
                }
                self.lift_value(Value::tuple(alloc::vec![Value::vec(yes), Value::vec(no)]))
            }
            "max_by_key" | "min_by_key" => {
                let mut best: Option<(Value, Value)> = None;
                for x in items.iter() {
                    let k = trf!(self.call1(&f, x.clone()));
                    best = match best {
                        None => Some((k, x.clone())),
                        Some((bk, bx)) => match cmp_values(&k, &bk) {
                            Some(o) => {
                                if (name == "max_by_key" && o != Ordering::Less) || (name == "min_by_key" && o == Ordering::Less) {
                                    Some((k, x.clone()))
                                } else {
                                    Some((bk, bx))
                                }
                            }
                            None => return Some(self.fail("cannot compare these keys")),
                        },
                    };
                }
                Some(Ok(Value::option(best.map(|b| b.1))))
            }
            "max_by" | "min_by" => {
                let mut best: Option<Value> = None;
                for x in items.iter() {
                    best = match best {
                        None => Some(x.clone()),
                        Some(b) => {
                            let r = trf!(self.call2(&f, b.clone(), x.clone()));
                            let o = trf!(self.order_of(r));
                            // Like Rust: max keeps the later of equals, min the earlier.
                            let take_new = if name == "max_by" { o != Ordering::Greater } else { o == Ordering::Greater };
                            Some(if take_new { x.clone() } else { b })
                        }
                    };
                }
                Some(Ok(Value::option(best)))
            }
            "fold" => {
                let mut acc = args[0].clone();
                for x in items.iter() {
                    acc = trf!(self.call2(&f, acc, x.clone()));
                }
                Some(Ok(acc))
            }
            "reduce" => {
                let mut it = items.iter();
                let Some(first) = it.next() else { return Some(Ok(Value::none())) };
                let mut acc = first.clone();
                for x in it {
                    acc = trf!(self.call2(&f, acc, x.clone()));
                }
                Some(Ok(Value::some(acc)))
            }
            _ => None,
        }
    }

    pub(crate) fn vec_mutate(&mut self, target: &mut Value, name: &str, args: Vec<Value>, tf: Option<&Type>) -> Res {
        let _ = tf;
        let Value::Vec(rc) = target else { return self.fail("internal error: not a Vec") };
        let n = rc.len();
        let arg = |k: usize| args.get(k).cloned().unwrap_or(Value::Unit);
        if !matches!(name, "push" | "pop" | "swap_remove" | "swap" | "clear") {
            self.work(n)?;
        }
        match name {
            "push" => {
                if args.len() != 1 {
                    return self.fail("`push` takes one argument");
                }
                self.check_vec(n + 1)?;
                self.charge(1)?;
                Rc::make_mut(rc).push(arg(0));
                Ok(Value::Unit)
            }
            "pop" => Ok(Value::option(Rc::make_mut(rc).pop())),
            "clear" => {
                Rc::make_mut(rc).clear();
                Ok(Value::Unit)
            }
            "insert" => {
                let i = self.want_int(name, &args, 0)?;
                if i < 0 || i as usize > n {
                    return self.fail(format!("insertion index {} is out of bounds (len {})", i, n));
                }
                if args.len() != 2 {
                    return self.fail("`insert` takes a position and a value");
                }
                self.check_vec(n + 1)?;
                self.charge(1)?;
                Rc::make_mut(rc).insert(i as usize, arg(1));
                Ok(Value::Unit)
            }
            "remove" | "swap_remove" => {
                let i = self.want_int(name, &args, 0)?;
                if i < 0 || i as usize >= n {
                    return self.fail(format!("removal index {} is out of bounds (len {})", i, n));
                }
                let v = Rc::make_mut(rc);
                Ok(if name == "remove" { v.remove(i as usize) } else { v.swap_remove(i as usize) })
            }
            "truncate" => {
                let k = self.want_int(name, &args, 0)?;
                if k < 0 {
                    return self.fail("`truncate` needs a length of 0 or more");
                }
                Rc::make_mut(rc).truncate(k as usize);
                Ok(Value::Unit)
            }
            "reverse" => {
                Rc::make_mut(rc).reverse();
                Ok(Value::Unit)
            }
            "swap" => {
                let (i, j) = (self.want_int(name, &args, 0)?, self.want_int(name, &args, 1)?);
                if i < 0 || j < 0 || i as usize >= n || j as usize >= n {
                    return self.fail(format!("swap positions {} and {} are out of bounds (len {})", i, j, n));
                }
                Rc::make_mut(rc).swap(i as usize, j as usize);
                Ok(Value::Unit)
            }
            "extend" | "extend_from_slice" => {
                let Some(other) = args.first() else { return self.fail(format!("`{}` needs a list", name)) };
                let other = self.list_arg(other)?;
                self.check_vec(n + other.len())?;
                self.charge(other.len())?;
                Rc::make_mut(rc).extend(other.iter().cloned());
                Ok(Value::Unit)
            }
            "append" => {
                let Some(Value::Ref(p)) = args.first() else { return self.fail("`append` needs `&mut other_vec`") };
                let p = (**p).clone();
                let other = self.read_place(&p)?;
                let other = self.list_arg(&other)?;
                self.check_vec(n + other.len())?;
                self.write_place(&p, Value::vec(Vec::new()))?;
                Rc::make_mut(rc).extend(other.iter().cloned());
                Ok(Value::Unit)
            }
            "dedup" => {
                Rc::make_mut(rc).dedup_by(|a, b| eq_values(a, b));
                Ok(Value::Unit)
            }
            "resize" => {
                let k = self.want_int(name, &args, 0)?;
                if k < 0 {
                    return self.fail("`resize` needs a length of 0 or more");
                }
                self.check_vec(k as usize)?;
                self.charge(k as usize)?;
                Rc::make_mut(rc).resize(k as usize, arg(1));
                Ok(Value::Unit)
            }
            "fill" => {
                let x = arg(0);
                for e in Rc::make_mut(rc).iter_mut() {
                    *e = x.clone();
                }
                Ok(Value::Unit)
            }
            "rotate_left" | "rotate_right" => {
                let k = self.want_int(name, &args, 0)?;
                if k < 0 || k as usize > n {
                    return self.fail(format!("`{}` needs a number up to the length", name));
                }
                let v = Rc::make_mut(rc);
                if name == "rotate_left" {
                    v.rotate_left(k as usize);
                } else {
                    v.rotate_right(k as usize);
                }
                Ok(Value::Unit)
            }
            "drain" => {
                let Some(Value::Range(r)) = args.first() else { return self.fail("`drain` needs a range, like `drain(..)` or `drain(0..2)`") };
                let (lo, hi) = match self.range_bounds(r, n) {
                    Ok(b) => b,
                    Err(m) => return self.fail(m),
                };
                let removed: Vec<Value> = Rc::make_mut(rc).drain(lo..hi).collect();
                self.lift(Ok(Value::vec(removed)))
            }
            "sort" | "sort_unstable" => {
                let items = core::mem::take(&mut Rc::make_mut(rc).0);
                let sorted = self.sort_natural(items)?;
                Rc::make_mut(rc).0 = sorted;
                Ok(Value::Unit)
            }
            "sort_by" | "sort_unstable_by" | "sort_by_key" | "sort_unstable_by_key" | "retain" => {
                let Some(f) = args.first().cloned() else { return self.fail(format!("`{}` needs a closure", name)) };
                let items = core::mem::take(&mut Rc::make_mut(rc).0);
                let result = match name {
                    "sort_by" | "sort_unstable_by" => self.sort_with(items.clone(), f),
                    "retain" => {
                        let mut kept = Vec::new();
                        let mut err = None;
                        for x in items.iter() {
                            match self.call1(&f, x.clone()).and_then(|r| self.truthy(r)) {
                                Ok(true) => kept.push(x.clone()),
                                Ok(false) => {}
                                Err(e) => {
                                    err = Some(e);
                                    break;
                                }
                            }
                        }
                        match err {
                            Some(e) => Err(e),
                            None => Ok(kept),
                        }
                    }
                    _ => self.sort_keyed(items.clone(), f),
                };
                match result {
                    Ok(v) => {
                        Rc::make_mut(rc).0 = v;
                        Ok(Value::Unit)
                    }
                    Err(e) => {
                        // Leave the list as it was.
                        Rc::make_mut(rc).0 = items;
                        Err(e)
                    }
                }
            }
            _ => self.fail(format!("no method `{}` for a Vec", name)),
        }
    }

    fn want_int(&self, name: &str, args: &[Value], i: usize) -> Result<i64, Flow> {
        match int(name, args, i) {
            Ok(x) => Ok(x),
            Err(m) => self.fail(m),
        }
    }
}

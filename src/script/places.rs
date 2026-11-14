//! Reaching into variables (places) and matching patterns.
//!
//! A place is a stack position plus a path (`items[2].count`). It is what
//! `&mut x` hands around and what assignment writes to. Reading follows
//! references met on the way; writing goes straight in, copying any storage
//! that is shared with another value first (copy on write).

use crate::script::ast::*;
use crate::script::interp::{Flow, Interp};
use crate::script::value::*;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;

fn field_of<'v>(cur: &'v Value, name: &Name) -> Option<&'v Value> {
    match cur {
        Value::Struct(s) => s.fields.iter().find(|f| f.0 == *name).map(|f| &f.1),
        Value::Enum(e) => {
            let k = e.names.as_ref().and_then(|ns| ns.iter().position(|x| x == name))?;
            e.fields.get(k)
        }
        _ => None,
    }
}

fn step_ref<'v>(cur: &'v Value, el: &Elem) -> Result<&'v Value, String> {
    let found = match el {
        Elem::Index(i) => match cur {
            Value::Vec(v) | Value::Tuple(v) => v.get(*i),
            Value::Struct(s) => s.fields.get(*i).map(|f| &f.1),
            Value::Enum(e) => e.fields.get(*i),
            _ => return Err(format!("cannot index into a {}", cur.type_name())),
        },
        Elem::Field(n) => field_of(cur, n),
    };
    found.ok_or_else(|| match el {
        Elem::Index(i) => format!("index {} is out of range", i),
        Elem::Field(n) => format!("no field `{}`", n),
    })
}

fn step_mut<'v>(cur: &'v mut Value, el: &Elem) -> Result<&'v mut Value, String> {
    let found: Option<&'v mut Value> = match (el, cur) {
        (Elem::Index(i), Value::Vec(rc)) | (Elem::Index(i), Value::Tuple(rc)) => Rc::make_mut(rc).get_mut(*i),
        (Elem::Index(i), Value::Struct(rc)) => Rc::make_mut(rc).fields.get_mut(*i).map(|f| &mut f.1),
        (Elem::Index(i), Value::Enum(rc)) => Rc::make_mut(rc).fields.get_mut(*i),
        (Elem::Field(n), Value::Struct(rc)) => Rc::make_mut(rc).fields.iter_mut().find(|f| f.0 == *n).map(|f| &mut f.1),
        (Elem::Field(n), Value::Enum(rc)) => {
            let e = Rc::make_mut(rc);
            let k = e.names.as_ref().and_then(|ns| ns.iter().position(|x| x == n));
            match k {
                Some(k) => e.fields.get_mut(k),
                None => None,
            }
        }
        (_, Value::Ref(_)) => return Err(String::from("cannot change something through a reference inside another value")),
        (_, other) => return Err(format!("cannot reach into a {}", other.type_name())),
    };
    found.ok_or_else(|| match el {
        Elem::Index(i) => format!("index {} is out of range", i),
        Elem::Field(n) => format!("no field `{}`", n),
    })
}

impl<'a> Interp<'a> {
    /// The value at a place, following references met on the way but not
    /// the one it may itself be.
    pub(crate) fn place_ref_raw(&self, p: &Place) -> Result<&Value, String> {
        let mut cur: &Value = match self.stack.get(p.slot) {
            Some(s) => &s.1,
            None => return Err(String::from("a reference outlived the variable it pointed at")),
        };
        for el in &p.path {
            let mut n = 0;
            while let Value::Ref(r) = cur {
                n += 1;
                if n > 16 {
                    return Err(String::from("a reference points at a reference too many times"));
                }
                cur = self.place_ref_raw(r)?;
            }
            cur = step_ref(cur, el)?;
        }
        Ok(cur)
    }

    pub(crate) fn place_mut(&mut self, p: &Place) -> Result<&mut Value, String> {
        let mut cur: &mut Value = match self.stack.get_mut(p.slot) {
            Some(s) => &mut s.1,
            None => return Err(String::from("a reference outlived the variable it pointed at")),
        };
        for el in &p.path {
            cur = step_mut(cur, el)?;
        }
        Ok(cur)
    }

    pub(crate) fn read_place(&self, p: &Place) -> Result<Value, Flow> {
        match self.place_ref_raw(p) {
            Ok(v) => Ok(v.clone()),
            Err(m) => self.fail(m),
        }
    }

    pub(crate) fn write_place(&mut self, p: &Place, v: Value) -> Result<(), Flow> {
        match self.place_mut(p) {
            Ok(slot) => {
                *slot = v;
                Ok(())
            }
            Err(m) => self.fail(m),
        }
    }

    /// If the value at `p` is a reference, the place it points at (and so on).
    fn follow(&self, mut p: Place) -> Place {
        for _ in 0..16 {
            let next = match self.place_ref_raw(&p) {
                Ok(Value::Ref(r)) => (**r).clone(),
                _ => break,
            };
            p = next;
        }
        p
    }

    /// The place an expression names, if it names one: a variable, a field,
    /// an element, or what a reference points at. With `follow_last`, a
    /// variable that holds a reference stands for what it points at (as
    /// for a method call); without, for itself (as for `x = ...`).
    pub(crate) fn place_of(&mut self, e: &Expr, follow_last: bool) -> Result<Option<Place>, Flow> {
        let p = match &e.kind {
            ExprKind::Path(segs) if segs.len() == 1 => match self.lookup(&segs[0]) {
                Some(slot) => Place { slot, path: Vec::new() },
                None => return Ok(None),
            },
            ExprKind::Field(base, name) => {
                let Some(mut p) = self.place_of(base, true)? else { return Ok(None) };
                p.path.push(Elem::Field(name.clone()));
                p
            }
            ExprKind::TupleField(base, i) => {
                let Some(mut p) = self.place_of(base, true)? else { return Ok(None) };
                p.path.push(Elem::Index(*i));
                p
            }
            ExprKind::Index(base, idx) => {
                let Some(mut p) = self.place_of(base, true)? else { return Ok(None) };
                match self.eval_val(idx)? {
                    Value::Int(i) if i >= 0 => p.path.push(Elem::Index(i as usize)),
                    Value::Int(i) => return self.fail_at(e.line, format!("index out of bounds: the index is {}", i)),
                    // A slice (`v[1..3]`) is a value, not a place.
                    _ => return Ok(None),
                }
                p
            }
            ExprKind::Unary(UnOp::Deref, inner) => {
                let target = match self.place_of(inner, false)? {
                    Some(p0) => match self.place_ref_raw(&p0) {
                        Ok(Value::Ref(r)) => Some((**r).clone()),
                        _ => Some(p0),
                    },
                    None => match self.eval(inner)? {
                        Value::Ref(r) => Some((*r).clone()),
                        _ => None,
                    },
                };
                match target {
                    Some(t) => t,
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        Ok(Some(if follow_last { self.follow(p) } else { p }))
    }

    // ---- patterns

    /// Does `v` fit `pat`? On a match the names it binds are added to `binds`.
    /// When `v` is a reference (`match &mut x`), names bind to references
    /// into it, so that `*n += 1` changes the original.
    pub(crate) fn match_value(&mut self, pat: &Pat, v: &Value, binds: &mut Vec<(Name, Value)>) -> Result<bool, Flow> {
        let (inner, place) = match v {
            Value::Ref(p) if !Self::is_plain_bind(pat) => (self.deref(v.clone())?, Some((**p).clone())),
            _ => (v.clone(), None),
        };
        self.match_at(pat, &inner, place.as_ref(), binds)
    }

    /// A pattern that just names the whole thing (or ignores it).
    fn is_plain_bind(pat: &Pat) -> bool {
        match pat {
            Pat::Wild => true,
            Pat::Bind(n, None) => !n.chars().next().map_or(false, |c| c.is_uppercase()),
            _ => false,
        }
    }

    fn sub_place(place: Option<&Place>, el: Elem) -> Option<Place> {
        place.map(|p| {
            let mut q = p.clone();
            q.path.push(el);
            q
        })
    }

    /// The pieces a tuple or slice pattern with a `..` in it matches, as
    /// (patterns before, patterns after) when `n` items are there.
    fn split_rest(pats: &[Pat], n: usize) -> Option<(usize, usize)> {
        match pats.iter().position(|p| matches!(p, Pat::Rest)) {
            Some(r) => {
                let (before, after) = (r, pats.len() - r - 1);
                if before + after <= n {
                    Some((before, after))
                } else {
                    None
                }
            }
            None => {
                if pats.len() == n {
                    Some((n, 0))
                } else {
                    None
                }
            }
        }
    }

    fn match_seq(&mut self, pats: &[Pat], items: &[Value], place: Option<&Place>, binds: &mut Vec<(Name, Value)>) -> Result<bool, Flow> {
        let Some((before, after)) = Self::split_rest(pats, items.len()) else { return Ok(false) };
        for (k, p) in pats[..before].iter().enumerate() {
            let sub = Self::sub_place(place, Elem::Index(k));
            if !self.match_sub(p, &items[k], sub, binds)? {
                return Ok(false);
            }
        }
        for k in 0..after {
            let (pi, ii) = (pats.len() - after + k, items.len() - after + k);
            let sub = Self::sub_place(place, Elem::Index(ii));
            if !self.match_sub(&pats[pi], &items[ii], sub, binds)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// A part of a bigger value: a reference found there is looked through.
    fn match_sub(&mut self, pat: &Pat, v: &Value, place: Option<Place>, binds: &mut Vec<(Name, Value)>) -> Result<bool, Flow> {
        if let Value::Ref(_) = v {
            return self.match_value(pat, v, binds);
        }
        self.match_at(pat, v, place.as_ref(), binds)
    }

    fn match_at(&mut self, pat: &Pat, v: &Value, place: Option<&Place>, binds: &mut Vec<(Name, Value)>) -> Result<bool, Flow> {
        match pat {
            Pat::Wild | Pat::Rest => Ok(true),
            Pat::Bind(name, sub) => {
                // `None` and constants are names that are not bindings.
                if name.chars().next().map_or(false, |c| c.is_uppercase()) {
                    if &**name == "None" {
                        return Ok(v.is_variant("Option", "None"));
                    }
                    let found = self.consts.iter().find(|c| c.0 == *name).map(|c| c.1.clone());
                    if let Some(c) = found {
                        return Ok(eq_values(&c, v));
                    }
                }
                if let Some(s) = sub {
                    if !self.match_at(s, v, place, binds)? {
                        return Ok(false);
                    }
                }
                let bound = match place {
                    Some(p) => Value::Ref(Rc::new(p.clone())),
                    None => v.clone(),
                };
                binds.push((name.clone(), bound));
                Ok(true)
            }
            Pat::Lit(l) => Ok(eq_values(l, v)),
            Pat::Range(lo, hi, incl) => {
                if let Some(lo) = lo {
                    match cmp_values(v, lo) {
                        Some(o) if o != core::cmp::Ordering::Less => {}
                        _ => return Ok(false),
                    }
                }
                if let Some(hi) = hi {
                    match cmp_values(v, hi) {
                        Some(core::cmp::Ordering::Less) => {}
                        Some(core::cmp::Ordering::Equal) if *incl => {}
                        _ => return Ok(false),
                    }
                }
                Ok(true)
            }
            Pat::Tuple(pats) => match v {
                Value::Tuple(items) => {
                    let items = items.clone();
                    self.match_seq(pats, &items, place, binds)
                }
                Value::Unit if pats.is_empty() => Ok(true),
                _ => Ok(false),
            },
            Pat::Slice(pats) => match v {
                Value::Vec(items) => {
                    let items = items.clone();
                    self.match_seq(pats, &items, place, binds)
                }
                _ => Ok(false),
            },
            Pat::Path(segs) => {
                let name = &segs[segs.len() - 1];
                match v {
                    Value::Enum(e) if e.variant == *name && e.fields.is_empty() => Ok(self.path_type_fits(segs, &e.ty)),
                    _ => {
                        let key = self.path_key(segs);
                        let found = self.consts.iter().find(|c| *c.0 == *key).map(|c| c.1.clone());
                        Ok(found.map_or(false, |c| eq_values(&c, v)))
                    }
                }
            }
            Pat::TupleStruct(segs, pats) => {
                let name = &segs[segs.len() - 1];
                match v {
                    Value::Enum(e) if e.variant == *name && self.path_type_fits(segs, &e.ty) => {
                        let fields = e.fields.clone();
                        self.match_seq(pats, &fields, place, binds)
                    }
                    Value::Struct(s) if s.name == *name => {
                        let fields: Vec<Value> = s.fields.iter().map(|f| f.1.clone()).collect();
                        self.match_seq(pats, &fields, place, binds)
                    }
                    _ => Ok(false),
                }
            }
            Pat::Struct(segs, fpats) => {
                let name = &segs[segs.len() - 1];
                match v {
                    Value::Struct(s) if s.name == *name => {
                        for (fname, fp) in fpats {
                            let Some(f) = s.fields.iter().find(|f| f.0 == *fname) else { return Ok(false) };
                            let sub = Self::sub_place(place, Elem::Field(fname.clone()));
                            if !self.match_sub(fp, &f.1, sub, binds)? {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    }
                    Value::Enum(e) if e.variant == *name && self.path_type_fits(segs, &e.ty) => {
                        let Some(names) = e.names.clone() else { return Ok(false) };
                        for (fname, fp) in fpats {
                            let Some(k) = names.iter().position(|n| n == fname) else { return Ok(false) };
                            let sub = Self::sub_place(place, Elem::Index(k));
                            if !self.match_sub(fp, &e.fields[k], sub, binds)? {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            Pat::Or(alts) => {
                for a in alts {
                    let mut trial = Vec::new();
                    if self.match_at(a, v, place, &mut trial)? {
                        binds.extend(trial);
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    /// `Shape::Circle` fits an enum called `Shape` (`Self::Circle` fits the
    /// enum being worked in; a bare `Circle` fits any).
    fn path_type_fits(&self, segs: &[Name], ty: &Name) -> bool {
        if segs.len() < 2 {
            return true;
        }
        let named = &segs[segs.len() - 2];
        if &**named == "Self" {
            return self.self_ty.as_ref().map_or(true, |t| t == ty);
        }
        named == ty
    }

    fn path_key(&self, segs: &[Name]) -> String {
        if segs.len() < 2 {
            return String::from(&*segs[0]);
        }
        let ty = &segs[segs.len() - 2];
        let ty: &str = if &**ty == "Self" { self.self_ty.as_deref().unwrap_or("Self") } else { ty };
        format!("{}::{}", ty, segs[segs.len() - 1])
    }
}

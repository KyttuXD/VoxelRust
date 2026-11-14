//! The interpreter: walks the tree of a [`Program`] and runs it.
//!
//! One [`Interp`] is made for each call into a script (a hook, a command)
//! and thrown away afterwards; what outlives a call is the program, its
//! constants and the mod's saved state (see `script/mod.rs`). Everything a
//! call may spend is limited (see `limits.rs`): evaluation steps, how deep
//! functions call one another, how much it may allocate. Running out is an
//! error, never a hang or a panic.
//!
//! Variables live on one stack of (name, value); a function's variables are
//! the part above its `base`. A `&mut x` is a [`Place`]: the stack position
//! of `x` and the way into it, so it stays valid however deep the calls go.

use crate::script::ast::*;
use crate::script::error::ScriptError;
use crate::script::host::Host;
use crate::script::limits::Limits;
use crate::script::ops;
use crate::script::state::ScriptState;
use crate::script::value::*;
use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;

/// Why evaluation left the normal path.
#[derive(Debug)]
pub enum Flow {
    Break(Option<Name>, Value),
    Continue(Option<Name>),
    Return(Value),
    Error(ScriptError),
}

pub type Res = Result<Value, Flow>;

pub struct Interp<'a> {
    pub(crate) prog: &'a Program,
    /// Constants by name (`Type::NAME` for the ones in `impl` blocks).
    pub(crate) consts: &'a mut Vec<(Name, Value)>,
    pub(crate) host: &'a mut dyn Host,
    pub(crate) state: &'a mut ScriptState,
    pub(crate) mod_name: &'a str,
    pub(crate) limits: Limits,
    pub(crate) steps: u64,
    pub(crate) cells: usize,
    pub(crate) depth: usize,
    pub(crate) nest: usize,
    pub(crate) stack: Vec<(Name, Value)>,
    /// Where the running function's variables start on the stack.
    pub(crate) base: usize,
    /// The type of the `impl` block being run (what `Self` means).
    pub(crate) self_ty: Option<Name>,
    /// The line being run, for errors raised from deep inside.
    pub(crate) line: u32,
}

enum Act {
    Next,
    Stop(Value),
    Out(Flow),
}

fn is_upper(name: &str) -> bool {
    name.chars().next().map_or(false, |c| c.is_uppercase())
}

impl<'a> Interp<'a> {
    pub fn new(prog: &'a Program, consts: &'a mut Vec<(Name, Value)>, host: &'a mut dyn Host, state: &'a mut ScriptState, mod_name: &'a str, limits: Limits) -> Interp<'a> {
        Interp { prog, consts, host, state, mod_name, limits, steps: 0, cells: 0, depth: 0, nest: 0, stack: Vec::new(), base: 0, self_ty: None, line: 0 }
    }

    // ---- errors and budgets

    pub(crate) fn fail<T>(&self, msg: impl Into<String>) -> Result<T, Flow> {
        Err(Flow::Error(ScriptError::new(&self.prog.file, self.line, 0, msg)))
    }

    pub(crate) fn fail_at<T>(&mut self, line: u32, msg: impl Into<String>) -> Result<T, Flow> {
        self.line = line;
        self.fail(msg)
    }

    /// The error a flow stands for at the top of a call.
    pub fn flow_error(&self, f: Flow) -> ScriptError {
        match f {
            Flow::Error(e) => e,
            Flow::Break(..) | Flow::Continue(_) => ScriptError::new(&self.prog.file, self.line, 0, "`break` or `continue` outside of a loop"),
            Flow::Return(_) => ScriptError::new(&self.prog.file, self.line, 0, "`return` outside of a function"),
        }
    }

    pub fn steps_used(&self) -> u64 {
        self.steps
    }

    pub(crate) fn step(&mut self) -> Result<(), Flow> {
        self.steps += 1;
        if self.steps > self.limits.max_steps {
            return self.fail(format!("the script used up its budget of {} steps in one call (an endless loop?)", self.limits.max_steps));
        }
        Ok(())
    }

    /// Charge for a job that walks `n` things.
    pub(crate) fn work(&mut self, n: usize) -> Result<(), Flow> {
        self.steps = self.steps.saturating_add((n / 4) as u64);
        if self.steps > self.limits.max_steps {
            return self.fail(format!("the script used up its budget of {} steps in one call (an endless loop?)", self.limits.max_steps));
        }
        Ok(())
    }

    pub(crate) fn charge(&mut self, n: usize) -> Result<(), Flow> {
        self.cells = self.cells.saturating_add(n);
        if self.cells > self.limits.max_cells {
            return self.fail(format!("the script made too many values in one call (limit {})", self.limits.max_cells));
        }
        Ok(())
    }

    pub(crate) fn check_vec(&self, n: usize) -> Result<(), Flow> {
        if n > self.limits.max_vec {
            return self.fail(format!("a Vec may not hold more than {} elements", self.limits.max_vec));
        }
        Ok(())
    }

    pub(crate) fn check_string(&self, n: usize) -> Result<(), Flow> {
        if n > self.limits.max_string {
            return self.fail(format!("a string may not be longer than {} bytes", self.limits.max_string));
        }
        Ok(())
    }

    // ---- entry points

    /// Work out the program's constants, in order.
    pub fn init_consts(&mut self) -> Result<(), ScriptError> {
        let prog = self.prog;
        for c in &prog.consts {
            self.line = c.init.line;
            self.self_ty = c.impl_ty.clone();
            let v = match self.eval_val(&c.init) {
                Ok(v) => v,
                Err(f) => return Err(self.flow_error(f)),
            };
            let v = match &c.ty {
                Some(t) => ops::coerce(t, v),
                None => v,
            };
            let key = match &c.impl_ty {
                Some(t) => Name::from(format!("{}::{}", t, c.name)),
                None => c.name.clone(),
            };
            self.consts.push((key, v));
        }
        self.self_ty = None;
        Ok(())
    }

    /// Call the script's function `name` if it has one. The arguments the
    /// function does not take are dropped (a hook need not name them all).
    pub fn call_named(&mut self, name: &str, mut args: Vec<Value>) -> Result<Option<Value>, ScriptError> {
        let prog = self.prog;
        let Some(def) = prog.fns.get(name) else { return Ok(None) };
        args.truncate(def.params.len());
        self.line = def.line;
        match self.call_fn(def, None, args) {
            Ok(v) => Ok(Some(v)),
            Err(f) => Err(self.flow_error(f)),
        }
    }

    // ---- calls

    pub(crate) fn call_fn(&mut self, def: &FnDef, self_val: Option<Value>, args: Vec<Value>) -> Res {
        if self.depth >= self.limits.max_depth {
            return self.fail(format!("functions are nested too deeply (more than {} calls: endless recursion?)", self.limits.max_depth));
        }
        if args.len() != def.params.len() {
            return self.fail(format!("`{}` takes {} argument(s) but {} were given", def.name, def.params.len(), args.len()));
        }
        self.depth += 1;
        let saved_base = self.base;
        let saved_self = self.self_ty.take();
        self.base = self.stack.len();
        self.self_ty = def.impl_ty.clone();
        let r = self.run_fn(def, self_val, args);
        self.stack.truncate(self.base);
        self.base = saved_base;
        self.self_ty = saved_self;
        self.depth -= 1;
        match r {
            Ok(v) | Err(Flow::Return(v)) => Ok(match &def.ret {
                Some(t) => ops::coerce(t, v),
                None => v,
            }),
            Err(Flow::Break(..)) | Err(Flow::Continue(_)) => self.fail("`break` or `continue` outside of a loop"),
            Err(e) => Err(e),
        }
    }

    fn run_fn(&mut self, def: &FnDef, self_val: Option<Value>, args: Vec<Value>) -> Res {
        if let Some(s) = self_val {
            self.stack.push((Name::from("self"), s));
        }
        for (p, a) in def.params.iter().zip(args) {
            let a = ops::coerce(&p.ty, a);
            self.bind_irrefutable(&p.pat, a)?;
        }
        self.eval_block(&def.body)
    }

    pub(crate) fn call_closure(&mut self, c: &ClosureVal, args: Vec<Value>) -> Res {
        if self.depth >= self.limits.max_depth {
            return self.fail(format!("functions are nested too deeply (more than {} calls: endless recursion?)", self.limits.max_depth));
        }
        if args.len() != c.def.params.len() {
            return self.fail(format!("this closure takes {} argument(s) but {} were given", c.def.params.len(), args.len()));
        }
        self.depth += 1;
        let saved_base = self.base;
        let saved_self = self.self_ty.take();
        self.base = self.stack.len();
        self.self_ty = c.self_ty.clone();
        for (n, v) in &c.env {
            self.stack.push((n.clone(), v.clone()));
        }
        let mut r = Ok(Value::Unit);
        for (p, a) in c.def.params.iter().zip(args) {
            if let Err(e) = self.bind_irrefutable(p, a) {
                r = Err(e);
                break;
            }
        }
        if r.is_ok() {
            r = self.eval(&c.def.body);
        }
        self.stack.truncate(self.base);
        self.base = saved_base;
        self.self_ty = saved_self;
        self.depth -= 1;
        match r {
            Ok(v) | Err(Flow::Return(v)) => Ok(v),
            Err(e) => Err(e),
        }
    }

    /// Call a closure or a function value.
    pub(crate) fn call_value(&mut self, f: &Value, args: Vec<Value>) -> Res {
        match f {
            Value::Closure(c) => {
                let c = c.clone();
                self.call_closure(&c, args)
            }
            Value::Func(name) => {
                let name = name.clone();
                self.call_func_name(&name, args)
            }
            Value::Ref(_) => {
                let v = self.deref(f.clone())?;
                self.call_value(&v, args)
            }
            other => self.fail(format!("a {} cannot be called", other.type_name())),
        }
    }

    /// `name` or `Type::name`, with the arguments already evaluated.
    fn call_func_name(&mut self, name: &str, args: Vec<Value>) -> Res {
        let prog = self.prog;
        match name.rsplit_once("::") {
            Some((ty, f)) => {
                if let Some(def) = prog.methods.get(&(Name::from(ty), Name::from(f))) {
                    return self.call_user_assoc(def, args);
                }
                if let Some(r) = self.call_variant_ctor(ty, f, &args) {
                    return r;
                }
                match self.call_assoc_builtin(ty, f, args) {
                    Some(r) => r,
                    None => self.fail(format!("cannot find `{}`", name)),
                }
            }
            None => {
                if let Some(def) = prog.fns.get(name) {
                    return self.call_fn(def, None, args);
                }
                if let Some(r) = self.call_variant_ctor("", name, &args) {
                    return r;
                }
                match self.call_builtin_fn(name, args) {
                    Some(r) => r,
                    None => self.fail(format!("cannot find the function `{}`", name)),
                }
            }
        }
    }

    /// `Type::f(args)` for a function in an `impl` block: the first
    /// argument is `self` when the function takes one.
    fn call_user_assoc(&mut self, def: &FnDef, mut args: Vec<Value>) -> Res {
        if def.self_kind.is_some() {
            if args.is_empty() {
                return self.fail(format!("`{}` needs a value to call it on", def.name));
            }
            let first = args.remove(0);
            return self.call_fn(def, Some(first), args);
        }
        self.call_fn(def, None, args)
    }

    /// `Some(x)`, `Ok(x)`, `Err(e)`, `Shape::Circle(r)`, `Point(1, 2)`.
    fn call_variant_ctor(&mut self, ty: &str, name: &str, args: &[Value]) -> Option<Res> {
        let prog = self.prog;
        if ty.is_empty() || ty == "Option" || ty == "Result" {
            let one = |args: &[Value]| args.first().cloned().unwrap_or(Value::Unit);
            match name {
                "Some" if args.len() == 1 => return Some(Ok(Value::some(one(args)))),
                "Ok" if args.len() == 1 => return Some(Ok(Value::ok(one(args)))),
                "Err" if args.len() == 1 => return Some(Ok(Value::err(one(args)))),
                _ => {}
            }
        }
        if ty.is_empty() {
            if let Some(s) = prog.structs.get(name) {
                if s.tuple {
                    if s.fields.len() != args.len() {
                        return Some(self.fail(format!("`{}` has {} fields but {} were given", name, s.fields.len(), args.len())));
                    }
                    let fields = s.fields.iter().cloned().zip(args.iter().cloned()).collect();
                    return Some(self.charge(1 + args.len()).map(|_| Value::Struct(Rc::new(StructVal { name: s.name.clone(), fields }))));
                }
            }
        }
        let ty = if ty == "Self" { self.self_ty.as_deref().unwrap_or("") } else { ty };
        if let Some(e) = prog.enums.get(ty) {
            if let Some(v) = e.variants.iter().find(|v| &*v.name == name) {
                if let VariantKind::Tuple(n) = v.kind {
                    if n != args.len() {
                        return Some(self.fail(format!("`{}::{}` has {} field(s) but {} were given", ty, name, n, args.len())));
                    }
                    let val = Value::Enum(Rc::new(EnumVal { ty: e.name.clone(), variant: v.name.clone(), fields: args.to_vec(), names: None }));
                    return Some(self.charge(1 + args.len()).map(|_| val));
                }
            }
        }
        None
    }

    // ---- values and variables

    pub(crate) fn lookup(&self, name: &str) -> Option<usize> {
        (self.base..self.stack.len()).rev().find(|&i| &*self.stack[i].0 == name)
    }

    /// Follow references down to the value they point at.
    pub(crate) fn deref(&self, v: Value) -> Res {
        let mut v = v;
        let mut n = 0;
        while let Value::Ref(p) = &v {
            n += 1;
            if n > 16 {
                return self.fail("a reference points at a reference too many times");
            }
            let next = self.read_place(p)?;
            v = next;
        }
        Ok(v)
    }

    pub(crate) fn eval_val(&mut self, e: &Expr) -> Res {
        let v = self.eval(e)?;
        self.line = e.line;
        self.deref(v)
    }

    fn eval_cond(&mut self, e: &Expr) -> Result<bool, Flow> {
        match self.eval_val(e)? {
            Value::Bool(b) => Ok(b),
            other => self.fail_at(e.line, format!("expected a bool here, found {}", other.type_name())),
        }
    }

    fn eval_int(&mut self, e: &Expr, what: &str) -> Result<i64, Flow> {
        match self.eval_val(e)? {
            Value::Int(i) => Ok(i),
            other => self.fail_at(e.line, format!("{} must be a whole number, found {}", what, other.type_name())),
        }
    }

    // ---- evaluation

    pub fn eval(&mut self, e: &Expr) -> Res {
        self.step()?;
        self.nest += 1;
        if self.nest > self.limits.max_nesting {
            self.nest -= 1;
            return self.fail_at(e.line, "this expression is nested too deeply");
        }
        self.line = e.line;
        let r = self.eval_kind(e);
        self.nest -= 1;
        r
    }

    fn eval_kind(&mut self, e: &Expr) -> Res {
        match &e.kind {
            ExprKind::Lit(v) => Ok(v.clone()),
            ExprKind::Path(segs) => self.eval_path(segs, e.line),
            ExprKind::Unary(op, x) => self.eval_unary(*op, x, e.line),
            ExprKind::Ref(is_mut, x) => {
                if *is_mut {
                    if let Some(p) = self.place_of(x, true)? {
                        return Ok(Value::Ref(Rc::new(p)));
                    }
                }
                self.eval(x)
            }
            ExprKind::Binary(op, a, b) => self.eval_binary(*op, a, b, e.line),
            ExprKind::Assign(l, r) => {
                let v = self.eval(r)?;
                self.line = e.line;
                match self.place_of(l, false)? {
                    Some(p) => self.write_place(&p, v)?,
                    None => return self.fail_at(e.line, "this cannot be assigned to"),
                }
                Ok(Value::Unit)
            }
            ExprKind::AssignOp(op, l, r) => {
                let rv = self.eval_val(r)?;
                self.line = e.line;
                let Some(p) = self.place_of(l, true)? else { return self.fail_at(e.line, "this cannot be assigned to") };
                // The value is taken out while it is worked on, so that a string
                // being added to is not shared (and so copied) every time.
                let cur = match self.place_mut(&p) {
                    Ok(slot) => core::mem::replace(slot, Value::Unit),
                    Err(m) => return self.fail_at(e.line, m),
                };
                self.line = e.line;
                let cur = match cur {
                    Value::Ref(_) => self.deref(cur)?,
                    other => other,
                };
                let new = self.apply(*op, cur, rv)?;
                self.write_place(&p, new)?;
                Ok(Value::Unit)
            }
            ExprKind::Call(f, args) => self.eval_call(f, args, e.line),
            ExprKind::MethodCall { recv, name, turbofish, args } => self.eval_method(recv, name, turbofish.as_ref(), args, e.line),
            ExprKind::Field(base, name) => {
                let b = self.eval_val(base)?;
                self.line = e.line;
                match &b {
                    Value::Struct(s) => match s.fields.iter().find(|f| f.0 == *name) {
                        Some(f) => Ok(f.1.clone()),
                        None => self.fail(format!("`{}` has no field `{}`", s.name, name)),
                    },
                    Value::Enum(en) => match en.names.as_ref().and_then(|n| n.iter().position(|x| x == name)) {
                        Some(k) => Ok(en.fields[k].clone()),
                        None => self.fail(format!("`{}` has no field `{}`", en.variant, name)),
                    },
                    other => self.fail(format!("a {} has no field `{}`", other.type_name(), name)),
                }
            }
            ExprKind::TupleField(base, i) => {
                let b = self.eval_val(base)?;
                self.line = e.line;
                let got = match &b {
                    Value::Tuple(t) => t.get(*i).cloned(),
                    Value::Struct(s) => s.fields.get(*i).map(|f| f.1.clone()),
                    _ => None,
                };
                match got {
                    Some(v) => Ok(v),
                    None => self.fail(format!("a {} has no field `{}`", b.type_name(), i)),
                }
            }
            ExprKind::Index(base, idx) => {
                let b = self.eval_val(base)?;
                let i = self.eval_val(idx)?;
                self.line = e.line;
                self.index_value(&b, &i)
            }
            ExprKind::Range(lo, hi, incl) => {
                let mut chars = false;
                let mut end = |p: &mut Self, x: &Option<Box<Expr>>| -> Result<Option<i64>, Flow> {
                    match x {
                        None => Ok(None),
                        Some(x) => match p.eval_val(x)? {
                            Value::Int(i) => Ok(Some(i)),
                            Value::Char(c) => {
                                chars = true;
                                Ok(Some(c as u32 as i64))
                            }
                            other => p.fail_at(x.line, format!("a range needs whole numbers, found {}", other.type_name())),
                        },
                    }
                };
                let lo = end(self, lo)?;
                let hi = end(self, hi)?;
                Ok(Value::Range(Rc::new(RangeVal { lo, hi, incl: *incl, chars })))
            }
            ExprKind::Cast(x, ty) => {
                let v = self.eval_val(x)?;
                self.line = e.line;
                // A unit variant as a number: its discriminant.
                if let Value::Enum(en) = &v {
                    if let Some(d) = self.prog.enums.get(&en.ty).and_then(|d| d.variants.iter().find(|x| x.name == en.variant)) {
                        return match ops::cast(&Value::Int(d.disc), &ty.name) {
                            Ok(v) => Ok(v),
                            Err(m) => self.fail(m),
                        };
                    }
                }
                match ops::cast(&v, &ty.name) {
                    Ok(v) => Ok(v),
                    Err(m) => self.fail(m),
                }
            }
            ExprKind::Try(x) => {
                let v = self.eval_val(x)?;
                self.line = e.line;
                match &v {
                    Value::Enum(en) if &*en.ty == "Option" => {
                        if &*en.variant == "Some" {
                            Ok(en.fields[0].clone())
                        } else {
                            Err(Flow::Return(Value::none()))
                        }
                    }
                    Value::Enum(en) if &*en.ty == "Result" => {
                        if &*en.variant == "Ok" {
                            Ok(en.fields[0].clone())
                        } else {
                            Err(Flow::Return(v.clone()))
                        }
                    }
                    other => self.fail(format!("`?` needs an Option or a Result, found {}", other.type_name())),
                }
            }
            ExprKind::Block(b) => self.eval_block(b),
            ExprKind::Let(pat, x) => {
                let v = self.eval(x)?;
                self.line = e.line;
                let mut binds = Vec::new();
                if self.match_value(pat, &v, &mut binds)? {
                    self.stack.extend(binds);
                    Ok(Value::Bool(true))
                } else {
                    Ok(Value::Bool(false))
                }
            }
            ExprKind::If { cond, then, els } => {
                let mark = self.stack.len();
                let c = self.eval_cond(cond)?;
                let r = if c {
                    self.eval_block(then)
                } else {
                    self.stack.truncate(mark);
                    match els {
                        Some(x) => self.eval(x),
                        None => Ok(Value::Unit),
                    }
                };
                self.stack.truncate(mark);
                r
            }
            ExprKind::Match { scrut, arms } => self.eval_match(scrut, arms, e.line),
            ExprKind::While { label, cond, body } => {
                loop {
                    self.step()?;
                    let mark = self.stack.len();
                    let c = self.eval_cond(cond)?;
                    if !c {
                        self.stack.truncate(mark);
                        return Ok(Value::Unit);
                    }
                    let r = self.eval_block(body);
                    self.stack.truncate(mark);
                    match Self::act(label, r) {
                        Act::Next => {}
                        Act::Stop(_) => return Ok(Value::Unit),
                        Act::Out(f) => return Err(f),
                    }
                }
            }
            ExprKind::Loop { label, body } => loop {
                self.step()?;
                let mark = self.stack.len();
                let r = self.eval_block(body);
                self.stack.truncate(mark);
                match Self::act(label, r) {
                    Act::Next => {}
                    Act::Stop(v) => return Ok(v),
                    Act::Out(f) => return Err(f),
                }
            },
            ExprKind::For { label, pat, iter, body } => self.eval_for(label, pat, iter, body, e.line),
            ExprKind::Break(label, v) => {
                let v = match v {
                    Some(x) => self.eval(x)?,
                    None => Value::Unit,
                };
                Err(Flow::Break(label.clone(), v))
            }
            ExprKind::Continue(label) => Err(Flow::Continue(label.clone())),
            ExprKind::Return(v) => {
                let v = match v {
                    Some(x) => self.eval(x)?,
                    None => Value::Unit,
                };
                Err(Flow::Return(v))
            }
            ExprKind::Closure(c) => {
                // The closure sees a copy of every variable of this function.
                let mut env: Vec<(Name, Value)> = Vec::new();
                for (n, v) in self.stack[self.base..].iter().rev() {
                    if !env.iter().any(|(k, _)| k == n) {
                        env.push((n.clone(), v.clone()));
                    }
                }
                env.reverse();
                self.charge(1 + env.len())?;
                Ok(Value::Closure(Rc::new(ClosureVal { def: c.clone(), env, self_ty: self.self_ty.clone() })))
            }
            ExprKind::Tuple(items) => {
                let mut out = Vec::with_capacity(items.len());
                for x in items {
                    out.push(self.eval(x)?);
                }
                self.charge(1 + out.len())?;
                Ok(Value::tuple(out))
            }
            ExprKind::Array(items) => {
                self.check_vec(items.len())?;
                let mut out = Vec::with_capacity(items.len());
                for x in items {
                    out.push(self.eval(x)?);
                }
                self.charge(1 + out.len())?;
                Ok(Value::vec(out))
            }
            ExprKind::Repeat(x, n) => {
                let v = self.eval_val(x)?;
                let n = self.eval_int(n, "the length of `[x; n]`")?;
                self.line = e.line;
                if n < 0 {
                    return self.fail("the length of `[x; n]` cannot be negative");
                }
                self.check_vec(n as usize)?;
                self.charge(1 + n as usize)?;
                self.work(n as usize)?;
                Ok(Value::vec(alloc::vec![v; n as usize]))
            }
            ExprKind::StructLit { path, fields, base } => self.eval_struct_lit(path, fields, base.as_deref(), e.line),
            ExprKind::Format { mac, pieces, args } => self.eval_format(*mac, pieces, args, e.line),
            ExprKind::Assert(cond, msg) => {
                if self.eval_cond(cond)? {
                    return Ok(Value::Unit);
                }
                let text = match msg {
                    Some(m) => self.format_message(m)?,
                    None => String::from("assertion failed"),
                };
                self.fail_at(e.line, text)
            }
            ExprKind::AssertEq(equal, a, b, msg) => {
                let (x, y) = (self.eval_val(a)?, self.eval_val(b)?);
                if eq_values(&x, &y) == *equal {
                    return Ok(Value::Unit);
                }
                let extra = match msg {
                    Some(m) => format!(": {}", self.format_message(m)?),
                    None => String::new(),
                };
                self.fail_at(e.line, format!("assertion failed: left {} right{}\n  left: {}\n right: {}", if *equal { "==" } else { "!=" }, extra, x.debug_string(), y.debug_string()))
            }
            ExprKind::Matches(x, pat, guard) => {
                let v = self.eval(x)?;
                let mut binds = Vec::new();
                let mark = self.stack.len();
                let mut hit = self.match_value(pat, &v, &mut binds)?;
                if hit {
                    self.stack.extend(binds);
                    if let Some(g) = guard {
                        hit = self.eval_cond(g)?;
                    }
                    self.stack.truncate(mark);
                }
                Ok(Value::Bool(hit))
            }
        }
    }

    fn act(label: &Option<Name>, r: Res) -> Act {
        match r {
            Ok(_) => Act::Next,
            Err(Flow::Break(l, v)) if l.is_none() || l == *label => Act::Stop(v),
            Err(Flow::Continue(l)) if l.is_none() || l == *label => Act::Next,
            Err(f) => Act::Out(f),
        }
    }

    pub(crate) fn apply(&mut self, op: BinOp, a: Value, b: Value) -> Res {
        if op == BinOp::Add {
            if let (Value::Str(x), Value::Str(y)) = (&a, &b) {
                // Joining onto a string nobody else holds adds only the new
                // piece; onto a shared one it copies the lot.
                if Rc::strong_count(x) > 1 {
                    self.work(x.len())?;
                    self.charge(1 + (x.len() + y.len()) / 16)?;
                } else {
                    self.charge(1 + y.len() / 16)?;
                }
            }
        }
        match ops::binary(op, a, b, self.limits.max_string) {
            Ok(v) => Ok(v),
            Err(m) => self.fail(m),
        }
    }

    fn eval_unary(&mut self, op: UnOp, x: &Expr, line: u32) -> Res {
        let v = self.eval(x)?;
        self.line = line;
        if op == UnOp::Deref {
            // Through a reference; a plain value is its own pointee.
            return match v {
                Value::Ref(_) => self.deref(v),
                other => Ok(other),
            };
        }
        let v = self.deref(v)?;
        match (op, &v) {
            (UnOp::Neg, Value::Int(i)) => match i.checked_neg() {
                Some(n) => Ok(Value::Int(n)),
                None => self.fail("attempt to negate with overflow"),
            },
            (UnOp::Neg, Value::Float(f)) => Ok(Value::Float(-*f)),
            (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!*b)),
            (UnOp::Not, Value::Int(i)) => Ok(Value::Int(!*i)),
            (UnOp::Neg, other) => self.fail(format!("cannot negate a {}", other.type_name())),
            (_, other) => self.fail(format!("cannot use `!` on a {}", other.type_name())),
        }
    }

    fn eval_binary(&mut self, op: BinOp, a: &Expr, b: &Expr, line: u32) -> Res {
        if op == BinOp::And || op == BinOp::Or {
            let left = self.eval_cond(a)?;
            if (op == BinOp::And && !left) || (op == BinOp::Or && left) {
                return Ok(Value::Bool(left));
            }
            let right = self.eval_cond(b)?;
            return Ok(Value::Bool(right));
        }
        let x = self.eval_val(a)?;
        let y = self.eval_val(b)?;
        self.line = line;
        self.apply(op, x, y)
    }

    fn index_value(&mut self, b: &Value, i: &Value) -> Res {
        match (b, i) {
            (Value::Vec(v), Value::Int(n)) | (Value::Tuple(v), Value::Int(n)) => {
                if *n < 0 || *n as usize >= v.len() {
                    return self.fail(format!("index out of bounds: the length is {} but the index is {}", v.len(), n));
                }
                Ok(v[*n as usize].clone())
            }
            (Value::Vec(v), Value::Range(r)) => {
                let (lo, hi) = match self.range_bounds(r, v.len()) {
                    Ok(x) => x,
                    Err(m) => return self.fail(m),
                };
                self.charge(1 + hi - lo)?;
                Ok(Value::vec(v[lo..hi].to_vec()))
            }
            (Value::Str(s), Value::Range(r)) => {
                let (lo, hi) = match self.range_bounds(r, s.len()) {
                    Ok(x) => x,
                    Err(m) => return self.fail(m),
                };
                match s.get(lo..hi) {
                    Some(t) => Ok(Value::str(t)),
                    None => self.fail("a string slice must start and end on character boundaries"),
                }
            }
            (Value::Str(_), Value::Int(_)) => self.fail("a string cannot be indexed by a number (use `.chars().nth(i)`)"),
            (b, i) => self.fail(format!("cannot index a {} with a {}", b.type_name(), i.type_name())),
        }
    }

    /// The (start, end) a range means in something of length `len`.
    pub(crate) fn range_bounds(&self, r: &RangeVal, len: usize) -> Result<(usize, usize), String> {
        let lo = r.lo.unwrap_or(0);
        let hi = match r.hi {
            Some(h) => {
                if r.incl {
                    h.saturating_add(1)
                } else {
                    h
                }
            }
            None => len as i64,
        };
        if lo < 0 || hi < lo {
            return Err(format!("slice index starts at {} but ends at {}", lo, hi));
        }
        if hi as usize > len {
            return Err(format!("range end index {} out of range for length {}", hi, len));
        }
        Ok((lo as usize, hi as usize))
    }

    fn eval_path(&mut self, segs: &[Name], line: u32) -> Res {
        if segs.len() == 1 {
            let name = &segs[0];
            if let Some(i) = self.lookup(name) {
                return Ok(self.stack[i].1.clone());
            }
            if let Some((_, v)) = self.consts.iter().find(|c| c.0 == *name) {
                return Ok(v.clone());
            }
            let prog = self.prog;
            if prog.fns.contains_key(name) {
                return Ok(Value::Func(name.clone()));
            }
            if let Some(s) = prog.structs.get(name) {
                if s.fields.is_empty() {
                    return Ok(Value::Struct(Rc::new(StructVal { name: s.name.clone(), fields: Vec::new() })));
                }
                if s.tuple {
                    return Ok(Value::Func(name.clone()));
                }
            }
            return match &**name {
                "None" => Ok(Value::none()),
                "Some" | "Ok" | "Err" => Ok(Value::Func(name.clone())),
                "self" => self.fail_at(line, "`self` is not available here"),
                _ => self.fail_at(line, format!("cannot find the name `{}` in this scope", name)),
            };
        }
        // `Type::NAME`: skip a leading `std::`, `core::` or module name.
        let ty = segs[segs.len() - 2].clone();
        let name = segs[segs.len() - 1].clone();
        let ty: Name = if &*ty == "Self" {
            match &self.self_ty {
                Some(t) => t.clone(),
                None => return self.fail_at(line, "`Self` is only available inside an `impl` block"),
            }
        } else {
            ty
        };
        let prog = self.prog;
        if let Some(e) = prog.enums.get(&ty) {
            if let Some(v) = e.variants.iter().find(|v| v.name == name) {
                return match &v.kind {
                    VariantKind::Unit => Ok(Value::Enum(Rc::new(EnumVal { ty: e.name.clone(), variant: v.name.clone(), fields: Vec::new(), names: None }))),
                    _ => Ok(Value::Func(Name::from(format!("{}::{}", ty, name)))),
                };
            }
        }
        let key = format!("{}::{}", ty, name);
        if let Some((_, v)) = self.consts.iter().find(|c| &*c.0 == key.as_str()) {
            return Ok(v.clone());
        }
        if prog.methods.contains_key(&(ty.clone(), name.clone())) {
            return Ok(Value::Func(Name::from(key)));
        }
        if let Some(v) = self.builtin_const(&ty, &name) {
            return Ok(v);
        }
        self.fail_at(line, format!("cannot find `{}` in this scope", key))
    }

    fn eval_args(&mut self, args: &[Expr]) -> Result<Vec<Value>, Flow> {
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            out.push(self.eval(a)?);
        }
        Ok(out)
    }

    fn eval_call(&mut self, f: &Expr, args: &[Expr], line: u32) -> Res {
        if let ExprKind::Path(segs) = &f.kind {
            let prog = self.prog;
            if segs.len() == 1 {
                let name = &segs[0];
                if self.lookup(name).is_none() {
                    let argv = self.eval_args(args)?;
                    self.line = line;
                    if let Some(def) = prog.fns.get(name) {
                        return self.call_fn(def, None, argv);
                    }
                    if let Some(r) = self.call_variant_ctor("", name, &argv) {
                        return r;
                    }
                    return match self.call_builtin_fn(name, argv) {
                        Some(r) => r,
                        None => self.fail(format!("cannot find the function `{}`", name)),
                    };
                }
            } else {
                let ty = &segs[segs.len() - 2];
                let name = &segs[segs.len() - 1];
                let ty: Name = if &**ty == "Self" {
                    match &self.self_ty {
                        Some(t) => t.clone(),
                        None => return self.fail_at(line, "`Self` is only available inside an `impl` block"),
                    }
                } else {
                    ty.clone()
                };
                let argv = self.eval_args(args)?;
                self.line = line;
                if let Some(def) = prog.methods.get(&(ty.clone(), name.clone())) {
                    return self.call_user_assoc(def, argv);
                }
                if let Some(r) = self.call_variant_ctor(&ty, name, &argv) {
                    return r;
                }
                // A function stored in a constant (`Type::HANDLER(x)`) is not supported; builtins:
                return match self.call_assoc_builtin(&ty, name, argv) {
                    Some(r) => r,
                    None => self.fail(format!("cannot find the function `{}::{}`", ty, name)),
                };
            }
        }
        let fv = self.eval(f)?;
        let argv = self.eval_args(args)?;
        self.line = line;
        self.call_value(&fv, argv)
    }

    fn eval_method(&mut self, recv: &Expr, name: &Name, tf: Option<&Type>, args: &[Expr], line: u32) -> Res {
        // The receiver: a variable (or part of one) we can reach again, or a
        // temporary.
        let mut place = self.place_of(recv, true)?;
        let mut temp = Value::Unit;
        if place.is_none() {
            let v = self.eval(recv)?;
            match v {
                Value::Ref(p) => place = Some((*p).clone()),
                other => temp = other,
            }
        }
        let argv = self.eval_args(args)?;
        self.line = line;
        // What kind of thing is it? (Looked at, not copied: a copy would
        // make a later change copy the whole Vec.)
        let (user_ty, mutating) = {
            let v: &Value = match &place {
                Some(p) => match self.place_ref_raw(p) {
                    Ok(v) => v,
                    Err(m) => return self.fail(m),
                },
                None => &temp,
            };
            let user_ty = match v {
                Value::Struct(s) => Some(s.name.clone()),
                Value::Enum(e) if &*e.ty != "Option" && &*e.ty != "Result" && &*e.ty != "Ordering" => Some(e.ty.clone()),
                _ => None,
            };
            (user_ty, crate::script::builtins::is_mutating(v, name))
        };
        let prog = self.prog;
        if let Some(ty) = user_ty {
            if let Some(def) = prog.methods.get(&(ty.clone(), name.clone())) {
                let kind = def.self_kind;
                let val = match (&place, kind) {
                    (Some(p), Some(SelfKind::Ref)) | (Some(p), Some(SelfKind::RefMut)) => Value::Ref(Rc::new(p.clone())),
                    (Some(p), _) => self.read_place(p)?,
                    (None, _) => temp,
                };
                if kind.is_none() {
                    return self.fail(format!("`{}::{}` is an associated function, call it as `{}::{}(..)`", ty, name, ty, name));
                }
                return self.call_fn(def, Some(val), argv);
            }
        }
        // Built-in methods. Ones that change the receiver work on it taken
        // out of its place, so that it is not shared (and so copied).
        if mutating {
            if let Some(p) = &place {
                let taken = match self.place_mut(p) {
                    Ok(slot) => core::mem::replace(slot, Value::Unit),
                    Err(m) => return self.fail(m),
                };
                let mut v = taken;
                let r = self.call_builtin_method_mut(&mut v, name, argv, tf);
                if let Ok(slot) = self.place_mut(p) {
                    *slot = v;
                }
                return r;
            }
            let mut v = temp;
            return self.call_builtin_method_mut(&mut v, name, argv, tf);
        }
        let val = match &place {
            Some(p) => self.read_place(p)?,
            None => temp,
        };
        self.call_builtin_method(val, place.as_ref(), name, argv, tf)
    }

    fn eval_match(&mut self, scrut: &Expr, arms: &[Arm], line: u32) -> Res {
        let v = self.eval(scrut)?;
        self.line = line;
        for arm in arms {
            let mark = self.stack.len();
            let mut binds = Vec::new();
            if !self.match_value(&arm.pat, &v, &mut binds)? {
                continue;
            }
            self.stack.extend(binds);
            if let Some(g) = &arm.guard {
                if !self.eval_cond(g)? {
                    self.stack.truncate(mark);
                    continue;
                }
            }
            let r = self.eval(&arm.body);
            self.stack.truncate(mark);
            return r;
        }
        let shown = match self.deref(v) {
            Ok(v) => v.debug_string(),
            Err(_) => String::from("?"),
        };
        self.fail_at(line, format!("no arm of this `match` fits {}", shown))
    }

    fn eval_for(&mut self, label: &Option<Name>, pat: &Pat, iter: &Expr, body: &Block, line: u32) -> Res {
        let it = self.eval(iter)?;
        self.line = line;
        // What is walked: a range (lazily), a Vec (by value), or a Vec
        // reached through `&mut` (each element as a reference).
        enum Src {
            Range(i64, i64, bool),
            List(Rc<List>),
            Refs(Place, usize),
        }
        let src = match it {
            Value::Range(r) => {
                let Some(lo) = r.lo else { return self.fail("a `for` over a range needs a start") };
                let hi = match r.hi {
                    Some(h) => {
                        if r.incl {
                            h.saturating_add(1)
                        } else {
                            h
                        }
                    }
                    None => i64::MAX,
                };
                Src::Range(lo, hi, r.chars)
            }
            Value::Vec(v) => Src::List(v),
            Value::Ref(p) => match self.deref(Value::Ref(p.clone()))? {
                Value::Vec(v) => Src::Refs((*p).clone(), v.len()),
                Value::Range(r) => {
                    let (lo, hi) = (r.lo.unwrap_or(0), r.hi.map_or(i64::MAX, |h| if r.incl { h.saturating_add(1) } else { h }));
                    Src::Range(lo, hi, r.chars)
                }
                other => return self.fail(format!("cannot loop over a {}", other.type_name())),
            },
            other => return self.fail(format!("cannot loop over a {} (use `.chars()` for a string)", other.type_name())),
        };
        let mut k = 0usize;
        let mut cur = match &src {
            Src::Range(lo, _, _) => *lo,
            _ => 0,
        };
        loop {
            self.step()?;
            let item = match &src {
                Src::Range(_, hi, chars) => {
                    if cur >= *hi {
                        break;
                    }
                    let v = if *chars {
                        match char::from_u32(cur as u32) {
                            Some(c) => Value::Char(c),
                            None => break,
                        }
                    } else {
                        Value::Int(cur)
                    };
                    cur += 1;
                    v
                }
                Src::List(v) => {
                    if k >= v.len() {
                        break;
                    }
                    k += 1;
                    v[k - 1].clone()
                }
                Src::Refs(p, n) => {
                    if k >= *n {
                        break;
                    }
                    let mut q = p.clone();
                    q.path.push(Elem::Index(k));
                    k += 1;
                    Value::Ref(Rc::new(q))
                }
            };
            let mark = self.stack.len();
            let mut binds = Vec::new();
            if !self.match_value(pat, &item, &mut binds)? {
                return self.fail_at(line, "the pattern of this `for` does not fit its items");
            }
            self.stack.extend(binds);
            let r = self.eval_block(body);
            self.stack.truncate(mark);
            match Self::act(label, r) {
                Act::Next => {}
                Act::Stop(_) => break,
                Act::Out(f) => return Err(f),
            }
        }
        Ok(Value::Unit)
    }

    pub(crate) fn eval_block(&mut self, b: &Block) -> Res {
        let mark = self.stack.len();
        let r = self.eval_block_inner(b);
        self.stack.truncate(mark);
        r
    }

    fn eval_block_inner(&mut self, b: &Block) -> Res {
        for s in &b.stmts {
            match s {
                Stmt::Expr(e) => {
                    self.eval(e)?;
                }
                Stmt::Let { pat, ty, init, els, line } => {
                    self.line = *line;
                    let Some(init) = init else {
                        // `let x;` — declared, given a value later.
                        if let Pat::Bind(n, _) = pat {
                            self.stack.push((n.clone(), Value::Unit));
                        }
                        continue;
                    };
                    let mut v = self.eval(init)?;
                    self.line = *line;
                    if let Some(t) = ty {
                        v = ops::coerce(t, v);
                    }
                    match els {
                        None => self.bind_irrefutable(pat, v)?,
                        Some(block) => {
                            let mut binds = Vec::new();
                            if self.match_value(pat, &v, &mut binds)? {
                                self.stack.extend(binds);
                            } else {
                                self.eval_block(block)?;
                                return self.fail_at(*line, "the `else` of a `let ... else` must leave (return, break, continue or panic)");
                            }
                        }
                    }
                }
            }
        }
        match &b.tail {
            Some(t) => self.eval(t),
            None => Ok(Value::Unit),
        }
    }

    pub(crate) fn bind_irrefutable(&mut self, pat: &Pat, v: Value) -> Result<(), Flow> {
        // The common case: a plain name.
        if let Pat::Bind(n, None) = pat {
            if !is_upper(n) {
                self.stack.push((n.clone(), v));
                return Ok(());
            }
        }
        if let Pat::Wild = pat {
            return Ok(());
        }
        let mut binds = Vec::new();
        if self.match_value(pat, &v, &mut binds)? {
            self.stack.extend(binds);
            Ok(())
        } else {
            self.fail("this value does not fit the pattern it is given to")
        }
    }

    fn eval_struct_lit(&mut self, path: &[Name], fields: &[(Name, Expr)], base: Option<&Expr>, line: u32) -> Res {
        let prog = self.prog;
        let last = path[path.len() - 1].clone();
        let first = if path.len() >= 2 { path[path.len() - 2].clone() } else { Name::from("") };
        let resolve_self = |n: &Name, me: &Option<Name>| -> Name {
            if &**n == "Self" {
                me.clone().unwrap_or_else(|| n.clone())
            } else {
                n.clone()
            }
        };
        let last = resolve_self(&last, &self.self_ty);
        let first = resolve_self(&first, &self.self_ty);
        let mut given: Vec<(Name, Value)> = Vec::with_capacity(fields.len());
        for (n, x) in fields {
            let v = self.eval_val(x)?;
            given.push((n.clone(), v));
        }
        self.line = line;
        let base_val = match base {
            Some(b) => Some(self.eval_val(b)?),
            None => None,
        };
        // An enum variant with braces?
        if path.len() >= 2 {
            if let Some(en) = prog.enums.get(&first) {
                if let Some(v) = en.variants.iter().find(|v| v.name == last) {
                    if let VariantKind::Struct(names) = &v.kind {
                        let mut out = Vec::with_capacity(names.len());
                        for n in names.iter() {
                            match given.iter().position(|g| g.0 == *n) {
                                Some(k) => out.push(given.swap_remove(k).1),
                                None => return self.fail(format!("`{}::{}` needs the field `{}`", first, last, n)),
                            }
                        }
                        if let Some(extra) = given.first() {
                            return self.fail(format!("`{}::{}` has no field `{}`", first, last, extra.0));
                        }
                        self.charge(1 + out.len())?;
                        return Ok(Value::Enum(Rc::new(EnumVal { ty: en.name.clone(), variant: v.name.clone(), fields: out, names: Some(names.clone()) })));
                    }
                }
            }
        }
        let Some(def) = prog.structs.get(&last) else { return self.fail(format!("cannot find the struct `{}`", last)) };
        let mut out: Vec<(Name, Value)> = Vec::with_capacity(def.fields.len());
        for n in &def.fields {
            if let Some(k) = given.iter().position(|g| g.0 == *n) {
                out.push(given.swap_remove(k));
            } else if let Some(Value::Struct(b)) = &base_val {
                match b.fields.iter().find(|f| f.0 == *n) {
                    Some(f) => out.push(f.clone()),
                    None => return self.fail(format!("the base value has no field `{}`", n)),
                }
            } else {
                return self.fail(format!("`{}` needs the field `{}`", last, n));
            }
        }
        if let Some(extra) = given.first() {
            return self.fail(format!("`{}` has no field `{}`", last, extra.0));
        }
        self.charge(1 + out.len())?;
        Ok(Value::Struct(Rc::new(StructVal { name: def.name.clone(), fields: out })))
    }

    // ---- format!

    fn format_pieces(&mut self, pieces: &[Piece], args: &[Expr]) -> Result<String, Flow> {
        let mut vals = Vec::with_capacity(args.len());
        for a in args {
            vals.push(self.eval_val(a)?);
        }
        let mut out = String::new();
        for p in pieces {
            match p {
                Piece::Lit(s) => out.push_str(s),
                Piece::Arg { arg, spec } => {
                    let Some(v) = vals.get(*arg) else { return self.fail("the format string asks for an argument that is not there") };
                    match crate::script::fmt::format_value(v, spec) {
                        Ok(s) => out.push_str(&s),
                        Err(m) => return self.fail(m),
                    }
                }
            }
            self.check_string(out.len())?;
        }
        self.charge(1 + out.len() / 16)?;
        Ok(out)
    }

    /// The text of a `Format` expression (used for assert messages).
    fn format_message(&mut self, e: &Expr) -> Result<String, Flow> {
        match &e.kind {
            ExprKind::Format { pieces, args, .. } => self.format_pieces(pieces, args),
            _ => Ok(String::new()),
        }
    }

    fn eval_format(&mut self, mac: FmtMac, pieces: &[Piece], args: &[Expr], line: u32) -> Res {
        let text = self.format_pieces(pieces, args)?;
        self.line = line;
        match mac {
            FmtMac::Format => Ok(Value::string(text)),
            FmtMac::Print => {
                self.host.log(&text);
                Ok(Value::Unit)
            }
            FmtMac::Panic => self.fail(if text.is_empty() { String::from("the script panicked") } else { format!("the script panicked: {}", text) }),
        }
    }
}

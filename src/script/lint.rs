//! A static look at a script for mistakes the language cannot see because it
//! is not typed: a call to a function that does not exist, a host function
//! called with the wrong number of arguments. `--script-check` prints these
//! and the game's test of every mod's scripts fails on them, so they are
//! found before anybody plays.

use crate::script::ast::{Block, Expr, ExprKind, Name, Pat, Program, Stmt};
use crate::script::ext::{all_functions, arity_of, name_of};
use crate::script::host::HOST_FUNCTIONS;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Names the interpreter itself answers when called like a function.
const BUILTIN_CALLS: &[&str] = &["Some", "Ok", "Err", "min", "max", "drop", "swap", "replace", "take"];

/// The number of parameters of the host function `name`, if it is one.
pub fn host_arity(name: &str) -> Option<usize> {
    HOST_FUNCTIONS.iter().chain(crate::script::host_ui::UI_FUNCTIONS).map(|f| f.0).chain(all_functions().map(|f| f.0)).find(|sig| name_of(sig) == name).map(arity_of)
}

struct Walk<'a> {
    prog: &'a Program,
    /// Every name bound anywhere in the file (a closure held in a variable can be called).
    bound: BTreeSet<Name>,
    problems: Vec<String>,
    /// Collecting the names that are bound (false) or checking calls (true).
    checking: bool,
}

/// What is wrong with the calls in a program: one message per mistake, with its line.
pub fn lint(prog: &Program) -> Vec<String> {
    let mut w = Walk { prog, bound: BTreeSet::new(), problems: Vec::new(), checking: false };
    for def in prog.fns.values().chain(prog.methods.values()) {
        for p in &def.params {
            w.pat(&p.pat);
        }
    }
    for def in prog.fns.values().chain(prog.methods.values()) {
        w.block(&def.body);
    }
    for c in &prog.consts {
        w.expr(&c.init);
    }
    // Second pass: now every binding is known, look at the calls.
    let mut w2 = Walk { prog, bound: w.bound, problems: Vec::new(), checking: true };
    for def in prog.fns.values().chain(prog.methods.values()) {
        w2.block(&def.body);
    }
    for c in &prog.consts {
        w2.expr(&c.init);
    }
    w2.problems
}

impl Walk<'_> {
    fn pat(&mut self, p: &Pat) {
        match p {
            Pat::Bind(n, inner) => {
                self.bound.insert(n.clone());
                if let Some(i) = inner {
                    self.pat(i);
                }
            }
            Pat::Tuple(v) | Pat::Or(v) | Pat::Slice(v) | Pat::TupleStruct(_, v) => v.iter().for_each(|p| self.pat(p)),
            Pat::Struct(_, fields) => fields.iter().for_each(|f| self.pat(&f.1)),
            _ => {}
        }
    }

    fn block(&mut self, b: &Block) {
        for s in &b.stmts {
            match s {
                Stmt::Let { pat, init, els, .. } => {
                    self.pat(pat);
                    if let Some(e) = init {
                        self.expr(e);
                    }
                    if let Some(b) = els {
                        self.block(b);
                    }
                }
                Stmt::Expr(e) => self.expr(e),
            }
        }
        if let Some(t) = &b.tail {
            self.expr(t);
        }
    }

    fn known_call(&self, name: &str) -> bool {
        self.prog.fns.contains_key(name)
            || self.prog.structs.contains_key(name)
            || self.bound.contains(name)
            || BUILTIN_CALLS.contains(&name)
            || host_arity(name).is_some()
            || self.prog.enums.values().any(|e| e.variants.iter().any(|v| &*v.name == name))
    }

    fn call(&mut self, f: &Expr, args: &[Expr], line: u32) {
        if let ExprKind::Path(segs) = &f.kind {
            if segs.len() == 1 && self.checking {
                let name: &str = &segs[0];
                if !self.known_call(name) {
                    self.problems.push(format!("{}:{}: `{}` is not a function of this file, nor a host function or part of the library", self.prog.file, line, name));
                } else if !self.prog.fns.contains_key(name) && !self.bound.contains(name) {
                    if let Some(n) = host_arity(name) {
                        if n != args.len() {
                            self.problems.push(format!("{}:{}: `{}` takes {} argument(s) but {} were given", self.prog.file, line, name, n, args.len()));
                        }
                    }
                } else if let Some(def) = self.prog.fns.get(name) {
                    if def.params.len() != args.len() {
                        self.problems.push(format!("{}:{}: `{}` takes {} argument(s) but {} were given", self.prog.file, line, name, def.params.len(), args.len()));
                    }
                }
            }
        }
    }

    fn expr(&mut self, e: &Expr) {
        use ExprKind::*;
        match &e.kind {
            Lit(_) | Path(_) | Continue(_) => {}
            Unary(_, a) | Ref(_, a) | Field(a, _) | TupleField(a, _) | Cast(a, _) | Try(a) => self.expr(a),
            Binary(_, a, b) | Assign(a, b) | AssignOp(_, a, b) | Index(a, b) | Repeat(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            Call(f, args) => {
                self.call(f, args, e.line);
                self.expr(f);
                args.iter().for_each(|a| self.expr(a));
            }
            MethodCall { recv, args, .. } => {
                self.expr(recv);
                args.iter().for_each(|a| self.expr(a));
            }
            Range(a, b, _) => {
                if let Some(a) = a {
                    self.expr(a);
                }
                if let Some(b) = b {
                    self.expr(b);
                }
            }
            Block(b) => self.block(b),
            Let(p, a) => {
                self.pat(p);
                self.expr(a);
            }
            If { cond, then, els } => {
                self.expr(cond);
                self.block(then);
                if let Some(x) = els {
                    self.expr(x);
                }
            }
            Match { scrut, arms } => {
                self.expr(scrut);
                for a in arms {
                    self.pat(&a.pat);
                    if let Some(g) = &a.guard {
                        self.expr(g);
                    }
                    self.expr(&a.body);
                }
            }
            While { cond, body, .. } => {
                self.expr(cond);
                self.block(body);
            }
            Loop { body, .. } => self.block(body),
            For { pat, iter, body, .. } => {
                self.pat(pat);
                self.expr(iter);
                self.block(body);
            }
            Break(_, v) | Return(v) => {
                if let Some(v) = v {
                    self.expr(v);
                }
            }
            Closure(c) => {
                c.params.iter().for_each(|p| self.pat(p));
                self.expr(&c.body);
            }
            Tuple(v) | Array(v) => v.iter().for_each(|x| self.expr(x)),
            StructLit { fields, base, .. } => {
                fields.iter().for_each(|f| self.expr(&f.1));
                if let Some(b) = base {
                    self.expr(b);
                }
            }
            Format { args, .. } => args.iter().for_each(|a| self.expr(a)),
            Assert(a, m) => {
                self.expr(a);
                if let Some(m) = m {
                    self.expr(m);
                }
            }
            AssertEq(_, a, b, m) => {
                self.expr(a);
                self.expr(b);
                if let Some(m) = m {
                    self.expr(m);
                }
            }
            Matches(a, p, g) => {
                self.expr(a);
                self.pat(p);
                if let Some(g) = g {
                    self.expr(g);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint_src(src: &str) -> Vec<String> {
        let prog = crate::script::parser::parse("t.rs", src).unwrap();
        lint(&prog)
    }

    #[test]
    fn a_good_file_has_no_mistakes() {
        let src = "struct P(i64);\nfn helper(a: i64) -> i64 { a + 1 }\nfn on_tick(dt: f32) { let f = |x: i64| x * 2; let p = P(f(helper(3))); say(&format!(\"{}\", p.0)); let (x, y, z) = player_pos(); give(\"a\", 1); is_loaded(x as i64, z as i64); match Some(1) { Some(n) => { min(n, 2); } None => {} } }";
        assert_eq!(lint_src(src), Vec::<String>::new());
    }

    #[test]
    fn a_call_to_nothing_is_found_with_its_line() {
        let m = lint_src("fn on_tick(dt: f32) {\n    sayy(\"x\");\n}");
        assert_eq!(m.len(), 1, "{:?}", m);
        assert!(m[0].starts_with("t.rs:2:") && m[0].contains("sayy"), "{:?}", m);
    }

    #[test]
    fn the_wrong_number_of_arguments_is_found() {
        let m = lint_src("fn helper(a: i64) {}\nfn on_tick(dt: f32) { give(\"a\"); helper(1, 2); is_loaded(1, 2, 3); spawn_mob(\"a\", 1.0, 2.0, 3.0); }");
        assert_eq!(m.len(), 3, "{:?}", m);
    }
}

//! Reading a script: tokens in, a [`Program`] out.
//!
//! The language is a small subset of Rust (see docs/SCRIPTING.md). Things
//! this does not know are refused with a message saying so and where.

use crate::script::ast::*;
use crate::script::error::ScriptError;
use crate::script::fmt::parse_format;
use crate::script::lexer::{lex, Tok, Token};
use crate::script::limits::{MAX_PARSE_DEPTH, MAX_SOURCE};
use crate::script::value::Value;
use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;

type R<T> = Result<T, ScriptError>;

const PRIMITIVES: [&str; 17] = ["i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64", "bool", "char", "str"];

/// Read a whole file. `file` is only used to word the errors.
pub fn parse(file: &str, src: &str) -> Result<Program, ScriptError> {
    if src.len() > MAX_SOURCE {
        return Err(ScriptError::new(file, 1, 0, format!("this file is {} bytes; the most allowed is {}", src.len(), MAX_SOURCE)));
    }
    let toks = lex(src).map_err(|e| ScriptError::new(file, e.line, e.col, e.msg))?;
    let mut p = Parser { file, toks, pos: 0, depth: 0, no_struct: false, prog: Program { file: String::from(file), ..Program::default() } };
    while !matches!(p.peek(), Tok::Eof) {
        p.parse_item()?;
    }
    Ok(p.prog)
}

struct Parser<'f> {
    file: &'f str,
    toks: Vec<Token>,
    pos: usize,
    depth: usize,
    /// In a condition (`if x {`): `Name {` is not a struct literal.
    no_struct: bool,
    prog: Program,
}

fn ex(kind: ExprKind, line: u32) -> Expr {
    Expr { kind, line }
}

fn is_upper(name: &str) -> bool {
    name.chars().next().map_or(false, |c| c.is_uppercase())
}

impl<'f> Parser<'f> {
    // ---- tokens

    fn peek(&self) -> &Tok {
        &self.toks[self.pos.min(self.toks.len() - 1)].tok
    }

    fn peek_at(&self, k: usize) -> &Tok {
        &self.toks[(self.pos + k).min(self.toks.len() - 1)].tok
    }

    fn line(&self) -> u32 {
        self.toks[self.pos.min(self.toks.len() - 1)].line
    }

    fn describe(&self) -> String {
        match self.peek() {
            Tok::Ident(s) => format!("`{}`", s),
            Tok::Int(_) | Tok::Float(_) => String::from("a number"),
            Tok::Str(_) => String::from("a string"),
            Tok::Char(_) => String::from("a character"),
            Tok::Life(s) => format!("`'{}`", s),
            Tok::Sym(s) => format!("`{}`", s),
            Tok::Eof => String::from("the end of the file"),
        }
    }

    fn err<T>(&self, msg: impl Into<String>) -> R<T> {
        let t = &self.toks[self.pos.min(self.toks.len() - 1)];
        Err(ScriptError::new(self.file, t.line, t.col, msg))
    }

    fn unexpected<T>(&self, wanted: &str) -> R<T> {
        self.err(format!("expected {}, found {}", wanted, self.describe()))
    }

    fn is_sym(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Sym(x) if *x == s)
    }

    fn is_sym_at(&self, k: usize, s: &str) -> bool {
        matches!(self.peek_at(k), Tok::Sym(x) if *x == s)
    }

    fn eat_sym(&mut self, s: &str) -> bool {
        if self.is_sym(s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_sym(&mut self, s: &str) -> R<()> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            self.unexpected(&format!("`{}`", s))
        }
    }

    fn is_kw(&self, s: &str) -> bool {
        matches!(self.peek(), Tok::Ident(x) if x == s)
    }

    fn is_kw_at(&self, k: usize, s: &str) -> bool {
        matches!(self.peek_at(k), Tok::Ident(x) if x == s)
    }

    fn eat_kw(&mut self, s: &str) -> bool {
        if self.is_kw(s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_kw(&mut self, s: &str) -> R<()> {
        if self.eat_kw(s) {
            Ok(())
        } else {
            self.unexpected(&format!("`{}`", s))
        }
    }

    fn ident(&mut self) -> R<Name> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.pos += 1;
                Ok(Name::from(s))
            }
            _ => self.unexpected("a name"),
        }
    }

    fn enter(&mut self) -> R<()> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            return self.err("this is nested too deeply");
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// Inside brackets a `Name {` may be a struct literal again.
    fn allow_struct<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        let saved = core::mem::replace(&mut self.no_struct, false);
        let r = f(self);
        self.no_struct = saved;
        r
    }

    /// In an `if` or `while` condition, a `match` or `for` head.
    fn without_struct<T>(&mut self, f: impl FnOnce(&mut Self) -> R<T>) -> R<T> {
        let saved = core::mem::replace(&mut self.no_struct, true);
        let r = f(self);
        self.no_struct = saved;
        r
    }

    /// `>` ending a generic list, where the lexer may have glued it to what follows.
    fn eat_gt(&mut self) -> R<()> {
        let i = self.pos.min(self.toks.len() - 1);
        match self.toks[i].tok {
            Tok::Sym(">") => {
                self.pos += 1;
                Ok(())
            }
            Tok::Sym(">>") => {
                self.toks[i].tok = Tok::Sym(">");
                Ok(())
            }
            Tok::Sym(">=") => {
                self.toks[i].tok = Tok::Sym("=");
                Ok(())
            }
            Tok::Sym(">>=") => {
                self.toks[i].tok = Tok::Sym(">=");
                Ok(())
            }
            _ => self.unexpected("`>`"),
        }
    }

    // ---- items

    /// Skip `#[...]` and `#![...]`.
    fn skip_attrs(&mut self) -> R<()> {
        while self.is_sym("#") {
            self.pos += 1;
            self.eat_sym("!");
            if !self.is_sym("[") {
                return self.unexpected("`[` after `#`");
            }
            self.skip_balanced("[", "]")?;
        }
        Ok(())
    }

    /// Skip a bracketed group, `open` being the current token.
    fn skip_balanced(&mut self, open: &str, close: &str) -> R<()> {
        let mut depth = 0usize;
        loop {
            if matches!(self.peek(), Tok::Eof) {
                return self.err(format!("`{}` is never closed", open));
            }
            if self.is_sym(open) {
                depth += 1;
            } else if self.is_sym(close) {
                depth -= 1;
                if depth == 0 {
                    self.pos += 1;
                    return Ok(());
                }
            }
            self.pos += 1;
        }
    }

    /// Skip `<...>` of generic parameters (declarations).
    fn skip_generics(&mut self) -> R<()> {
        if !self.is_sym("<") {
            return Ok(());
        }
        let mut depth = 0i32;
        loop {
            match self.peek() {
                Tok::Eof => return self.err("`<` is never closed"),
                Tok::Sym("<") => depth += 1,
                Tok::Sym(">") => depth -= 1,
                Tok::Sym(">>") => depth -= 2,
                _ => {}
            }
            self.pos += 1;
            if depth <= 0 {
                return Ok(());
            }
        }
    }

    fn skip_where(&mut self) {
        if self.eat_kw("where") {
            while !self.is_sym("{") && !self.is_sym(";") && !matches!(self.peek(), Tok::Eof) {
                self.pos += 1;
            }
        }
    }

    fn skip_vis(&mut self) {
        if self.eat_kw("pub") && self.is_sym("(") {
            let _ = self.skip_balanced("(", ")");
        }
    }

    fn is_item_start(&self) -> bool {
        match self.peek() {
            Tok::Ident(s) => match s.as_str() {
                "fn" | "struct" | "enum" | "impl" | "use" | "pub" | "static" | "trait" | "mod" | "type" => true,
                // `const` starts an item, but `const { }` blocks are not supported anyway.
                "const" => true,
                "unsafe" => self.is_kw_at(1, "fn") || self.is_kw_at(1, "impl"),
                _ => false,
            },
            _ => false,
        }
    }

    fn parse_item(&mut self) -> R<()> {
        self.skip_attrs()?;
        self.skip_vis();
        let line = self.line();
        if self.is_kw("unsafe") {
            self.pos += 1;
        }
        let word = match self.peek() {
            Tok::Ident(s) => s.clone(),
            _ => return self.unexpected("an item (`fn`, `struct`, `enum`, `impl`, `const` or `use`)"),
        };
        match word.as_str() {
            "fn" => {
                let f = self.parse_fn(None)?;
                self.add_fn(f)
            }
            "const" if self.is_kw_at(1, "fn") => {
                self.pos += 1;
                let f = self.parse_fn(None)?;
                self.add_fn(f)
            }
            "const" | "static" => {
                self.pos += 1;
                self.eat_kw("mut");
                let c = self.parse_const(None)?;
                self.prog.consts.push(c);
                Ok(())
            }
            "struct" => self.parse_struct(),
            "enum" => self.parse_enum(),
            "impl" => self.parse_impl(),
            "use" => {
                while !self.is_sym(";") {
                    if matches!(self.peek(), Tok::Eof) {
                        return self.err("this `use` line has no `;`");
                    }
                    self.pos += 1;
                }
                self.pos += 1;
                Ok(())
            }
            "type" => {
                while !self.is_sym(";") && !matches!(self.peek(), Tok::Eof) {
                    self.pos += 1;
                }
                self.eat_sym(";");
                Ok(())
            }
            "trait" => Err(ScriptError::new(self.file, line, 0, "traits are not part of the script language (use plain functions and `impl` blocks)")),
            "mod" => Err(ScriptError::new(self.file, line, 0, "modules are not part of the script language (one file is one module)")),
            other => self.err(format!("expected an item (`fn`, `struct`, `enum`, `impl`, `const` or `use`), found `{}`", other)),
        }
    }

    fn add_fn(&mut self, f: Rc<FnDef>) -> R<()> {
        if self.prog.fns.contains_key(&f.name) {
            return Err(ScriptError::new(self.file, f.line, 0, format!("the function `{}` is defined twice", f.name)));
        }
        self.prog.fns.insert(f.name.clone(), f);
        Ok(())
    }

    fn parse_const(&mut self, impl_ty: Option<Name>) -> R<ConstDef> {
        let name = self.ident()?;
        let ty = if self.eat_sym(":") { Some(self.parse_type()?) } else { None };
        self.expect_sym("=")?;
        let mut init = self.parse_expr()?;
        if let Some(t) = &ty {
            hint(&mut init, t);
        }
        self.expect_sym(";")?;
        Ok(ConstDef { name, ty, init, impl_ty })
    }

    /// Is the next thing a `self` parameter? How many tokens, and what kind.
    fn self_param(&self) -> Option<(SelfKind, usize)> {
        let mut k = 0;
        let mut by_ref = false;
        if self.is_sym_at(k, "&") {
            by_ref = true;
            k += 1;
            if matches!(self.peek_at(k), Tok::Life(_)) {
                k += 1;
            }
        }
        let mut is_mut = false;
        if self.is_kw_at(k, "mut") {
            is_mut = true;
            k += 1;
        }
        if self.is_kw_at(k, "self") {
            k += 1;
            let kind = if by_ref {
                if is_mut {
                    SelfKind::RefMut
                } else {
                    SelfKind::Ref
                }
            } else {
                SelfKind::Value
            };
            Some((kind, k))
        } else {
            None
        }
    }

    fn parse_fn(&mut self, impl_ty: Option<Name>) -> R<Rc<FnDef>> {
        let line = self.line();
        self.expect_kw("fn")?;
        let name = self.ident()?;
        self.skip_generics()?;
        self.expect_sym("(")?;
        let mut self_kind = None;
        let mut params = Vec::new();
        if let Some((kind, n)) = self.self_param() {
            self.pos += n;
            self_kind = Some(kind);
            if self.eat_sym(":") {
                self.parse_type()?;
            }
            if !self.is_sym(")") {
                self.expect_sym(",")?;
            }
        }
        while !self.is_sym(")") {
            self.skip_attrs()?;
            let pat = self.parse_pattern_no_or()?;
            self.expect_sym(":")?;
            let ty = self.parse_type()?;
            params.push(Param { pat, ty });
            if !self.is_sym(")") {
                self.expect_sym(",")?;
            }
        }
        self.expect_sym(")")?;
        let ret = if self.eat_sym("->") { Some(self.parse_type()?) } else { None };
        self.skip_where();
        if self.is_sym(";") {
            return self.err("a function without a body (a trait method) is not supported");
        }
        let body = self.parse_block()?;
        Ok(Rc::new(FnDef { name, self_kind, params, ret, body, impl_ty, line }))
    }

    fn parse_struct(&mut self) -> R<()> {
        self.expect_kw("struct")?;
        let name = self.ident()?;
        self.skip_generics()?;
        self.skip_where();
        let mut fields = Vec::new();
        let mut tuple = false;
        if self.eat_sym(";") {
            // A unit struct.
        } else if self.is_sym("(") {
            tuple = true;
            self.pos += 1;
            let mut n = 0;
            while !self.is_sym(")") {
                self.skip_attrs()?;
                self.skip_vis();
                self.parse_type()?;
                fields.push(Name::from(format!("{}", n)));
                n += 1;
                if !self.is_sym(")") {
                    self.expect_sym(",")?;
                }
            }
            self.pos += 1;
            self.skip_where();
            self.expect_sym(";")?;
        } else {
            self.expect_sym("{")?;
            while !self.is_sym("}") {
                self.skip_attrs()?;
                self.skip_vis();
                fields.push(self.ident()?);
                self.expect_sym(":")?;
                self.parse_type()?;
                if !self.is_sym("}") {
                    self.expect_sym(",")?;
                }
            }
            self.pos += 1;
        }
        self.prog.structs.insert(name.clone(), StructDef { name, fields, tuple });
        Ok(())
    }

    fn parse_enum(&mut self) -> R<()> {
        self.expect_kw("enum")?;
        let name = self.ident()?;
        self.skip_generics()?;
        self.skip_where();
        self.expect_sym("{")?;
        let mut variants: Vec<VariantDef> = Vec::new();
        let mut next_disc = 0i64;
        while !self.is_sym("}") {
            self.skip_attrs()?;
            let vname = self.ident()?;
            let kind = if self.is_sym("(") {
                self.pos += 1;
                let mut n = 0;
                while !self.is_sym(")") {
                    self.parse_type()?;
                    n += 1;
                    if !self.is_sym(")") {
                        self.expect_sym(",")?;
                    }
                }
                self.pos += 1;
                VariantKind::Tuple(n)
            } else if self.is_sym("{") {
                self.pos += 1;
                let mut names = Vec::new();
                while !self.is_sym("}") {
                    self.skip_attrs()?;
                    names.push(self.ident()?);
                    self.expect_sym(":")?;
                    self.parse_type()?;
                    if !self.is_sym("}") {
                        self.expect_sym(",")?;
                    }
                }
                self.pos += 1;
                VariantKind::Struct(Rc::new(names))
            } else {
                VariantKind::Unit
            };
            if self.eat_sym("=") {
                let neg = self.eat_sym("-");
                match self.peek().clone() {
                    Tok::Int(n) => {
                        self.pos += 1;
                        next_disc = if neg { -n } else { n };
                    }
                    _ => return self.unexpected("a whole number after `=`"),
                }
            }
            if variants.iter().any(|v| v.name == vname) {
                return self.err(format!("the variant `{}` is defined twice", vname));
            }
            variants.push(VariantDef { name: vname, kind, disc: next_disc });
            next_disc += 1;
            if !self.is_sym("}") {
                self.expect_sym(",")?;
            }
        }
        self.pos += 1;
        self.prog.enums.insert(name.clone(), EnumDef { name, variants });
        Ok(())
    }

    fn parse_impl(&mut self) -> R<()> {
        self.expect_kw("impl")?;
        self.skip_generics()?;
        let mut ty = self.parse_type()?;
        if self.eat_kw("for") {
            // `impl Trait for Type`: the trait is only a name here.
            ty = self.parse_type()?;
        }
        self.skip_where();
        self.expect_sym("{")?;
        let ty_name = ty.name.clone();
        while !self.is_sym("}") {
            self.skip_attrs()?;
            self.skip_vis();
            if matches!(self.peek(), Tok::Eof) {
                return self.err("this `impl` block is never closed");
            }
            if self.is_kw("fn") || (self.is_kw("const") && self.is_kw_at(1, "fn")) || (self.is_kw("unsafe") && self.is_kw_at(1, "fn")) {
                if !self.is_kw("fn") {
                    self.pos += 1;
                }
                let f = self.parse_fn(Some(ty_name.clone()))?;
                let key = (ty_name.clone(), f.name.clone());
                if self.prog.methods.contains_key(&key) {
                    return Err(ScriptError::new(self.file, f.line, 0, format!("`{}::{}` is defined twice", ty_name, f.name)));
                }
                self.prog.methods.insert(key, f);
            } else if self.eat_kw("const") {
                let c = self.parse_const(Some(ty_name.clone()))?;
                self.prog.consts.push(c);
            } else if self.is_kw("type") {
                while !self.is_sym(";") && !matches!(self.peek(), Tok::Eof) {
                    self.pos += 1;
                }
                self.eat_sym(";");
            } else {
                return self.unexpected("`fn` or `const` in an `impl` block");
            }
        }
        self.pos += 1;
        Ok(())
    }

    // ---- types

    fn parse_generic_args(&mut self) -> R<Vec<Type>> {
        // The `<` is the current token.
        self.expect_sym("<")?;
        let mut args = Vec::new();
        loop {
            if matches!(self.peek(), Tok::Sym(">") | Tok::Sym(">>") | Tok::Sym(">=") | Tok::Sym(">>=")) {
                break;
            }
            if matches!(self.peek(), Tok::Life(_)) {
                self.pos += 1;
            } else if matches!(self.peek(), Tok::Ident(_)) && self.is_sym_at(1, "=") {
                // `Item = u32`
                self.pos += 2;
                args.push(self.parse_type()?);
            } else {
                args.push(self.parse_type()?);
            }
            if !self.eat_sym(",") {
                break;
            }
        }
        self.eat_gt()?;
        Ok(args)
    }

    fn parse_type(&mut self) -> R<Type> {
        self.enter()?;
        let r = self.parse_type_inner(true);
        self.leave();
        r
    }

    fn simple(name: &str) -> Type {
        Type { name: Name::from(name), args: Vec::new() }
    }

    fn parse_type_inner(&mut self, generics: bool) -> R<Type> {
        match self.peek().clone() {
            Tok::Sym("&") | Tok::Sym("&&") => {
                self.pos += 1;
                if matches!(self.peek(), Tok::Life(_)) {
                    self.pos += 1;
                }
                self.eat_kw("mut");
                self.parse_type_inner(true)
            }
            Tok::Sym("*") => {
                self.pos += 1;
                if !self.eat_kw("const") {
                    self.eat_kw("mut");
                }
                self.parse_type_inner(true)
            }
            Tok::Sym("(") => {
                self.pos += 1;
                let mut args = Vec::new();
                while !self.is_sym(")") {
                    args.push(self.parse_type()?);
                    if !self.is_sym(")") {
                        self.expect_sym(",")?;
                    }
                }
                self.pos += 1;
                Ok(Type { name: Name::from("()"), args })
            }
            Tok::Sym("[") => {
                self.pos += 1;
                let elem = self.parse_type()?;
                if self.eat_sym(";") {
                    self.parse_expr()?;
                }
                self.expect_sym("]")?;
                Ok(Type { name: Name::from("[]"), args: alloc::vec![elem] })
            }
            Tok::Sym("!") => {
                self.pos += 1;
                Ok(Self::simple("!"))
            }
            Tok::Sym("_") => {
                self.pos += 1;
                Ok(Self::simple("_"))
            }
            Tok::Ident(w) if w == "dyn" || w == "impl" => {
                self.pos += 1;
                let first = self.parse_type()?;
                while self.eat_sym("+") {
                    if matches!(self.peek(), Tok::Life(_)) {
                        self.pos += 1;
                    } else {
                        self.parse_type()?;
                    }
                }
                Ok(first)
            }
            Tok::Ident(w) if w == "fn" => {
                self.pos += 1;
                if !self.is_sym("(") {
                    return self.unexpected("`(`");
                }
                self.skip_balanced("(", ")")?;
                if self.eat_sym("->") {
                    self.parse_type()?;
                }
                Ok(Self::simple("_"))
            }
            Tok::Ident(_) => {
                let mut last = self.ident()?;
                let mut args = Vec::new();
                loop {
                    if generics && self.is_sym("<") {
                        args = self.parse_generic_args()?;
                    } else if self.is_sym("(") && matches!(&*last, "Fn" | "FnMut" | "FnOnce") {
                        self.skip_balanced("(", ")")?;
                        if self.eat_sym("->") {
                            self.parse_type()?;
                        }
                        return Ok(Self::simple("_"));
                    }
                    if self.is_sym("::") && matches!(self.peek_at(1), Tok::Ident(_)) {
                        self.pos += 1;
                        last = self.ident()?;
                        args = Vec::new();
                    } else {
                        break;
                    }
                }
                Ok(Type { name: last, args })
            }
            _ => self.unexpected("a type"),
        }
    }

    // ---- blocks and statements

    fn parse_block(&mut self) -> R<Block> {
        self.enter()?;
        let r = self.parse_block_inner();
        self.leave();
        r
    }

    fn starts_blocklike(&self) -> bool {
        match self.peek() {
            Tok::Ident(s) => matches!(s.as_str(), "if" | "match" | "while" | "loop" | "for") || (s == "unsafe" && self.is_sym_at(1, "{")),
            Tok::Sym("{") => true,
            Tok::Life(_) => self.is_sym_at(1, ":"),
            _ => false,
        }
    }

    fn parse_block_inner(&mut self) -> R<Block> {
        self.expect_sym("{")?;
        let saved = core::mem::replace(&mut self.no_struct, false);
        let mut stmts = Vec::new();
        let mut tail = None;
        loop {
            if self.eat_sym(";") {
                continue;
            }
            if self.eat_sym("}") {
                break;
            }
            if matches!(self.peek(), Tok::Eof) {
                return self.err("this block is never closed: expected `}`");
            }
            self.skip_attrs()?;
            if self.is_item_start() {
                self.parse_item()?;
                continue;
            }
            if self.is_kw("let") {
                stmts.push(self.parse_let()?);
                continue;
            }
            let mut e;
            if self.starts_blocklike() {
                e = self.parse_primary()?;
                if !(self.is_sym(".") || self.is_sym("?")) {
                    if self.eat_sym("}") {
                        tail = Some(Box::new(e));
                        break;
                    }
                    self.eat_sym(";");
                    stmts.push(Stmt::Expr(e));
                    continue;
                }
                // `match x {..}.method()`: carry on as a plain expression.
                e = self.parse_postfix(e)?;
            } else {
                e = self.parse_expr()?;
            }
            if self.eat_sym(";") {
                stmts.push(Stmt::Expr(e));
            } else if self.eat_sym("}") {
                tail = Some(Box::new(e));
                break;
            } else {
                return self.unexpected("`;` or `}`");
            }
        }
        self.no_struct = saved;
        Ok(Block { stmts, tail })
    }

    fn parse_let(&mut self) -> R<Stmt> {
        let line = self.line();
        self.expect_kw("let")?;
        let pat = self.parse_pattern_no_or()?;
        let ty = if self.eat_sym(":") { Some(self.parse_type()?) } else { None };
        let mut init = None;
        let mut els = None;
        if self.eat_sym("=") {
            let mut e = self.parse_expr()?;
            if let Some(t) = &ty {
                hint(&mut e, t);
            }
            init = Some(e);
            if self.eat_kw("else") {
                els = Some(self.parse_block()?);
            }
        }
        self.expect_sym(";")?;
        Ok(Stmt::Let { pat, ty, init, els, line })
    }

    // ---- expressions

    fn parse_expr(&mut self) -> R<Expr> {
        self.enter()?;
        let r = self.parse_assign();
        self.leave();
        r
    }

    fn parse_assign(&mut self) -> R<Expr> {
        let lhs = self.parse_range()?;
        let line = self.line();
        let op = match self.peek() {
            Tok::Sym("=") => None,
            Tok::Sym("+=") => Some(BinOp::Add),
            Tok::Sym("-=") => Some(BinOp::Sub),
            Tok::Sym("*=") => Some(BinOp::Mul),
            Tok::Sym("/=") => Some(BinOp::Div),
            Tok::Sym("%=") => Some(BinOp::Rem),
            Tok::Sym("&=") => Some(BinOp::BitAnd),
            Tok::Sym("|=") => Some(BinOp::BitOr),
            Tok::Sym("^=") => Some(BinOp::BitXor),
            Tok::Sym("<<=") => Some(BinOp::Shl),
            Tok::Sym(">>=") => Some(BinOp::Shr),
            _ => return Ok(lhs),
        };
        self.pos += 1;
        let rhs = self.parse_assign()?;
        Ok(match op {
            None => ex(ExprKind::Assign(Box::new(lhs), Box::new(rhs)), line),
            Some(op) => ex(ExprKind::AssignOp(op, Box::new(lhs), Box::new(rhs)), line),
        })
    }

    /// Whether the next token can begin an expression (for the end of `a..`
    /// and the value of `return` / `break`).
    fn can_start_expr(&self) -> bool {
        match self.peek() {
            Tok::Eof => false,
            Tok::Sym(s) => match *s {
                ")" | "]" | "}" | "," | ";" | "=>" | "=" => false,
                "{" => !self.no_struct,
                _ => true,
            },
            Tok::Ident(s) => !matches!(s.as_str(), "else" | "as"),
            _ => true,
        }
    }

    fn parse_range(&mut self) -> R<Expr> {
        let line = self.line();
        if self.is_sym("..") || self.is_sym("..=") {
            let incl = self.is_sym("..=");
            self.pos += 1;
            let hi = if self.can_start_expr() { Some(Box::new(self.parse_bin(1)?)) } else { None };
            return Ok(ex(ExprKind::Range(None, hi, incl), line));
        }
        let lo = self.parse_bin(1)?;
        if self.is_sym("..") || self.is_sym("..=") {
            let incl = self.is_sym("..=");
            self.pos += 1;
            let hi = if self.can_start_expr() { Some(Box::new(self.parse_bin(1)?)) } else { None };
            return Ok(ex(ExprKind::Range(Some(Box::new(lo)), hi, incl), line));
        }
        Ok(lo)
    }

    fn binop(&self) -> Option<(BinOp, u8)> {
        Some(match self.peek() {
            Tok::Sym("||") => (BinOp::Or, 1),
            Tok::Sym("&&") => (BinOp::And, 2),
            Tok::Sym("==") => (BinOp::Eq, 3),
            Tok::Sym("!=") => (BinOp::Ne, 3),
            Tok::Sym("<") => (BinOp::Lt, 3),
            Tok::Sym(">") => (BinOp::Gt, 3),
            Tok::Sym("<=") => (BinOp::Le, 3),
            Tok::Sym(">=") => (BinOp::Ge, 3),
            Tok::Sym("|") => (BinOp::BitOr, 4),
            Tok::Sym("^") => (BinOp::BitXor, 5),
            Tok::Sym("&") => (BinOp::BitAnd, 6),
            Tok::Sym("<<") => (BinOp::Shl, 7),
            Tok::Sym(">>") => (BinOp::Shr, 7),
            Tok::Sym("+") => (BinOp::Add, 8),
            Tok::Sym("-") => (BinOp::Sub, 8),
            Tok::Sym("*") => (BinOp::Mul, 9),
            Tok::Sym("/") => (BinOp::Div, 9),
            Tok::Sym("%") => (BinOp::Rem, 9),
            _ => return None,
        })
    }

    fn parse_bin(&mut self, min: u8) -> R<Expr> {
        let mut lhs = self.parse_cast()?;
        while let Some((op, prec)) = self.binop() {
            if prec < min {
                break;
            }
            let line = self.line();
            self.pos += 1;
            let rhs = self.parse_bin(prec + 1)?;
            lhs = ex(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), line);
        }
        Ok(lhs)
    }

    fn parse_cast(&mut self) -> R<Expr> {
        let mut e = self.parse_unary()?;
        while self.is_kw("as") {
            let line = self.line();
            self.pos += 1;
            let generics = !matches!(self.peek(), Tok::Ident(n) if PRIMITIVES.contains(&n.as_str()));
            let ty = self.parse_type_inner(generics)?;
            e = ex(ExprKind::Cast(Box::new(e), ty), line);
        }
        Ok(e)
    }

    fn parse_unary(&mut self) -> R<Expr> {
        let line = self.line();
        match self.peek() {
            Tok::Sym("-") => {
                self.pos += 1;
                self.enter()?;
                let operand = self.parse_unary();
                self.leave();
                let operand = operand?;
                Ok(match operand.kind {
                    ExprKind::Lit(Value::Int(n)) => ex(ExprKind::Lit(Value::Int(n.wrapping_neg())), line),
                    ExprKind::Lit(Value::Float(f)) => ex(ExprKind::Lit(Value::Float(-f)), line),
                    _ => ex(ExprKind::Unary(UnOp::Neg, Box::new(operand)), line),
                })
            }
            Tok::Sym("!") | Tok::Sym("*") => {
                let op = if self.is_sym("!") { UnOp::Not } else { UnOp::Deref };
                self.pos += 1;
                self.enter()?;
                let operand = self.parse_unary();
                self.leave();
                Ok(ex(ExprKind::Unary(op, Box::new(operand?)), line))
            }
            Tok::Sym("&") | Tok::Sym("&&") => {
                let twice = self.is_sym("&&");
                self.pos += 1;
                let is_mut = self.eat_kw("mut");
                self.enter()?;
                let operand = self.parse_unary();
                self.leave();
                let mut e = ex(ExprKind::Ref(is_mut, Box::new(operand?)), line);
                if twice {
                    e = ex(ExprKind::Ref(false, Box::new(e)), line);
                }
                Ok(e)
            }
            _ => {
                let p = self.parse_primary()?;
                self.parse_postfix(p)
            }
        }
    }

    fn parse_args(&mut self, close: &str) -> R<Vec<Expr>> {
        let mut args = Vec::new();
        self.allow_struct(|p| {
            while !p.is_sym(close) {
                args.push(p.parse_expr()?);
                if !p.is_sym(close) {
                    p.expect_sym(",")?;
                }
            }
            p.pos += 1;
            Ok(())
        })?;
        Ok(args)
    }

    fn parse_postfix(&mut self, mut e: Expr) -> R<Expr> {
        loop {
            let line = self.line();
            match self.peek().clone() {
                Tok::Sym("?") => {
                    self.pos += 1;
                    e = ex(ExprKind::Try(Box::new(e)), line);
                }
                Tok::Sym("(") => {
                    self.pos += 1;
                    let args = self.parse_args(")")?;
                    e = ex(ExprKind::Call(Box::new(e), args), line);
                }
                Tok::Sym("[") => {
                    self.pos += 1;
                    let idx = self.allow_struct(|p| p.parse_expr())?;
                    self.expect_sym("]")?;
                    e = ex(ExprKind::Index(Box::new(e), Box::new(idx)), line);
                }
                Tok::Sym(".") => {
                    self.pos += 1;
                    match self.peek().clone() {
                        Tok::Int(n) => {
                            self.pos += 1;
                            e = ex(ExprKind::TupleField(Box::new(e), n.max(0) as usize), line);
                        }
                        Tok::Ident(name) => {
                            if name == "await" {
                                return self.err("`.await` is not part of the script language");
                            }
                            let line = self.line();
                            self.pos += 1;
                            let mut turbofish = None;
                            if self.is_sym("::") && self.is_sym_at(1, "<") {
                                self.pos += 1;
                                let mut args = self.parse_generic_args()?;
                                if !args.is_empty() {
                                    turbofish = Some(args.remove(0));
                                }
                            }
                            if self.is_sym("(") {
                                self.pos += 1;
                                let args = self.parse_args(")")?;
                                e = ex(ExprKind::MethodCall { recv: Box::new(e), name: Name::from(name), turbofish, args }, line);
                            } else {
                                e = ex(ExprKind::Field(Box::new(e), Name::from(name)), line);
                            }
                        }
                        _ => return self.unexpected("a field or method name after `.`"),
                    }
                }
                _ => return Ok(e),
            }
        }
    }

    fn parse_cond(&mut self) -> R<Expr> {
        self.without_struct(|p| p.parse_expr())
    }

    fn parse_if(&mut self) -> R<Expr> {
        let line = self.line();
        self.expect_kw("if")?;
        let cond = self.parse_cond()?;
        let then = self.parse_block()?;
        let els = if self.eat_kw("else") {
            if self.is_kw("if") {
                Some(Box::new(self.parse_if()?))
            } else {
                let l = self.line();
                Some(Box::new(ex(ExprKind::Block(self.parse_block()?), l)))
            }
        } else {
            None
        };
        Ok(ex(ExprKind::If { cond: Box::new(cond), then, els }, line))
    }

    fn parse_loop_kind(&mut self, label: Option<Name>) -> R<Expr> {
        let line = self.line();
        if self.eat_kw("while") {
            let cond = self.parse_cond()?;
            let body = self.parse_block()?;
            Ok(ex(ExprKind::While { label, cond: Box::new(cond), body }, line))
        } else if self.eat_kw("loop") {
            let body = self.parse_block()?;
            Ok(ex(ExprKind::Loop { label, body }, line))
        } else if self.eat_kw("for") {
            let pat = self.parse_pattern_no_or()?;
            self.expect_kw("in")?;
            let iter = self.parse_cond()?;
            let body = self.parse_block()?;
            Ok(ex(ExprKind::For { label, pat, iter: Box::new(iter), body }, line))
        } else if self.is_sym("{") {
            // A labelled block: `'a: { ... break 'a value; }` is a loop that runs once.
            let body = self.parse_block()?;
            let brk = ex(ExprKind::Break(label.clone(), Some(Box::new(ex(ExprKind::Block(body), line)))), line);
            let once = Block { stmts: Vec::new(), tail: Some(Box::new(brk)) };
            Ok(ex(ExprKind::Loop { label, body: once }, line))
        } else {
            self.unexpected("`loop`, `while` or `for` after a label")
        }
    }

    fn parse_match(&mut self) -> R<Expr> {
        let line = self.line();
        self.expect_kw("match")?;
        let scrut = self.parse_cond()?;
        self.expect_sym("{")?;
        let mut arms = Vec::new();
        let saved = core::mem::replace(&mut self.no_struct, false);
        while !self.is_sym("}") {
            if matches!(self.peek(), Tok::Eof) {
                return self.err("this `match` is never closed");
            }
            self.skip_attrs()?;
            self.eat_sym("|");
            let pat = self.parse_pattern()?;
            let guard = if self.eat_kw("if") { Some(self.parse_expr()?) } else { None };
            self.expect_sym("=>")?;
            let body = if self.is_sym("{") {
                let l = self.line();
                let b = self.parse_block()?;
                ex(ExprKind::Block(b), l)
            } else {
                self.parse_expr()?
            };
            arms.push(Arm { pat, guard, body });
            if !self.eat_sym(",") && !self.is_sym("}") && !matches!(arms.last().map(|a| &a.body.kind), Some(ExprKind::Block(_))) {
                return self.unexpected("`,` or `}` after a match arm");
            }
        }
        self.pos += 1;
        self.no_struct = saved;
        Ok(ex(ExprKind::Match { scrut: Box::new(scrut), arms }, line))
    }

    fn parse_closure(&mut self) -> R<Expr> {
        let line = self.line();
        self.eat_kw("move");
        let mut params = Vec::new();
        if self.eat_sym("||") {
            // No parameters.
        } else {
            self.expect_sym("|")?;
            while !self.is_sym("|") {
                params.push(self.parse_pattern_no_or()?);
                if self.eat_sym(":") {
                    self.parse_type()?;
                }
                if !self.is_sym("|") {
                    self.expect_sym(",")?;
                }
            }
            self.pos += 1;
        }
        let body = if self.eat_sym("->") {
            self.parse_type()?;
            let l = self.line();
            let b = self.parse_block()?;
            ex(ExprKind::Block(b), l)
        } else {
            self.parse_expr()?
        };
        Ok(ex(ExprKind::Closure(Rc::new(Closure { params, body })), line))
    }

    fn parse_primary(&mut self) -> R<Expr> {
        self.enter()?;
        let r = self.parse_primary_inner();
        self.leave();
        r
    }

    fn parse_primary_inner(&mut self) -> R<Expr> {
        let line = self.line();
        match self.peek().clone() {
            Tok::Int(n) => {
                self.pos += 1;
                Ok(ex(ExprKind::Lit(Value::Int(n)), line))
            }
            Tok::Float(f) => {
                self.pos += 1;
                Ok(ex(ExprKind::Lit(Value::Float(f)), line))
            }
            Tok::Str(s) => {
                self.pos += 1;
                Ok(ex(ExprKind::Lit(Value::string(s)), line))
            }
            Tok::Char(c) => {
                self.pos += 1;
                Ok(ex(ExprKind::Lit(Value::Char(c)), line))
            }
            Tok::Life(l) => {
                if !self.is_sym_at(1, ":") {
                    return self.err("a lifetime is not an expression");
                }
                self.pos += 2;
                self.parse_loop_kind(Some(Name::from(l)))
            }
            Tok::Sym("(") => {
                self.pos += 1;
                self.allow_struct(|p| {
                    if p.eat_sym(")") {
                        return Ok(ex(ExprKind::Lit(Value::Unit), line));
                    }
                    let first = p.parse_expr()?;
                    if p.eat_sym(")") {
                        return Ok(first);
                    }
                    let mut items = alloc::vec![first];
                    while p.eat_sym(",") {
                        if p.is_sym(")") {
                            break;
                        }
                        items.push(p.parse_expr()?);
                    }
                    p.expect_sym(")")?;
                    Ok(ex(ExprKind::Tuple(items), line))
                })
            }
            Tok::Sym("[") => {
                self.pos += 1;
                self.parse_array(line, "]")
            }
            Tok::Sym("{") => {
                let b = self.parse_block()?;
                Ok(ex(ExprKind::Block(b), line))
            }
            Tok::Sym("|") | Tok::Sym("||") => self.parse_closure(),
            Tok::Ident(word) => match word.as_str() {
                "true" => {
                    self.pos += 1;
                    Ok(ex(ExprKind::Lit(Value::Bool(true)), line))
                }
                "false" => {
                    self.pos += 1;
                    Ok(ex(ExprKind::Lit(Value::Bool(false)), line))
                }
                "if" => self.parse_if(),
                "match" => self.parse_match(),
                "while" | "loop" | "for" => self.parse_loop_kind(None),
                "unsafe" if self.is_sym_at(1, "{") => {
                    self.pos += 1;
                    let b = self.parse_block()?;
                    Ok(ex(ExprKind::Block(b), line))
                }
                "move" => self.parse_closure(),
                "let" => {
                    self.pos += 1;
                    let pat = self.parse_pattern()?;
                    self.expect_sym("=")?;
                    let scrut = self.parse_bin(3)?;
                    Ok(ex(ExprKind::Let(pat, Box::new(scrut)), line))
                }
                "return" => {
                    self.pos += 1;
                    let v = if self.can_start_expr() { Some(Box::new(self.parse_expr()?)) } else { None };
                    Ok(ex(ExprKind::Return(v), line))
                }
                "break" => {
                    self.pos += 1;
                    let label = match self.peek().clone() {
                        Tok::Life(l) => {
                            self.pos += 1;
                            Some(Name::from(l))
                        }
                        _ => None,
                    };
                    let v = if self.can_start_expr() { Some(Box::new(self.parse_expr()?)) } else { None };
                    Ok(ex(ExprKind::Break(label, v), line))
                }
                "continue" => {
                    self.pos += 1;
                    let label = match self.peek().clone() {
                        Tok::Life(l) => {
                            self.pos += 1;
                            Some(Name::from(l))
                        }
                        _ => None,
                    };
                    Ok(ex(ExprKind::Continue(label), line))
                }
                "async" | "await" | "yield" | "trait" | "mod" | "fn" | "struct" | "enum" | "impl" => self.err(format!("`{}` cannot be used here", word)),
                _ => {
                    if self.is_sym_at(1, "!") && (self.is_sym_at(2, "(") || self.is_sym_at(2, "[") || self.is_sym_at(2, "{")) {
                        return self.parse_macro();
                    }
                    self.parse_path_expr(line)
                }
            },
            _ => self.unexpected("an expression"),
        }
    }

    fn parse_array(&mut self, line: u32, close: &str) -> R<Expr> {
        self.allow_struct(|p| {
            if p.eat_sym(close) {
                return Ok(ex(ExprKind::Array(Vec::new()), line));
            }
            let first = p.parse_expr()?;
            if p.eat_sym(";") {
                let n = p.parse_expr()?;
                p.expect_sym(close)?;
                return Ok(ex(ExprKind::Repeat(Box::new(first), Box::new(n)), line));
            }
            let mut items = alloc::vec![first];
            while p.eat_sym(",") {
                if p.is_sym(close) {
                    break;
                }
                items.push(p.parse_expr()?);
            }
            p.expect_sym(close)?;
            Ok(ex(ExprKind::Array(items), line))
        })
    }

    fn parse_path_expr(&mut self, line: u32) -> R<Expr> {
        let mut segs = alloc::vec![self.ident()?];
        while self.is_sym("::") {
            self.pos += 1;
            if self.is_sym("<") {
                // `Vec::<i32>::new()`: the generics mean nothing here.
                self.parse_generic_args()?;
                continue;
            }
            segs.push(self.ident()?);
        }
        let last_upper = segs.last().map_or(false, |s| is_upper(s));
        if self.is_sym("{") && !self.no_struct && last_upper && self.looks_like_struct_literal() {
            return self.parse_struct_lit(segs, line);
        }
        Ok(ex(ExprKind::Path(segs), line))
    }

    /// At `{` after a capitalised path: `{ }`, `{ name:`, `{ name,`, `{ name }`, `{ ..`.
    fn looks_like_struct_literal(&self) -> bool {
        if self.is_sym_at(1, "}") || self.is_sym_at(1, "..") {
            return true;
        }
        matches!(self.peek_at(1), Tok::Ident(_) | Tok::Int(_)) && (self.is_sym_at(2, ":") || self.is_sym_at(2, ",") || self.is_sym_at(2, "}"))
    }

    fn parse_struct_lit(&mut self, path: Vec<Name>, line: u32) -> R<Expr> {
        self.expect_sym("{")?;
        let mut fields = Vec::new();
        let mut base = None;
        self.allow_struct(|p| {
            while !p.is_sym("}") {
                if p.eat_sym("..") {
                    base = Some(Box::new(p.parse_expr()?));
                    break;
                }
                let fline = p.line();
                let fname = match p.peek().clone() {
                    Tok::Int(n) => {
                        p.pos += 1;
                        Name::from(format!("{}", n))
                    }
                    _ => p.ident()?,
                };
                let value = if p.eat_sym(":") { p.parse_expr()? } else { ex(ExprKind::Path(alloc::vec![fname.clone()]), fline) };
                fields.push((fname, value));
                if !p.is_sym("}") {
                    p.expect_sym(",")?;
                }
            }
            Ok(())
        })?;
        self.expect_sym("}")?;
        Ok(ex(ExprKind::StructLit { path, fields, base }, line))
    }

    // ---- macros

    fn parse_macro(&mut self) -> R<Expr> {
        let line = self.line();
        let name = self.ident()?;
        self.expect_sym("!")?;
        let close = match self.peek() {
            Tok::Sym("(") => ")",
            Tok::Sym("[") => "]",
            _ => "}",
        };
        self.pos += 1;
        match &*name {
            "vec" => self.parse_array(line, close),
            "format" => self.parse_format_macro(FmtMac::Format, close, line),
            "print" | "println" | "eprint" | "eprintln" => self.parse_format_macro(FmtMac::Print, close, line),
            "panic" | "unreachable" | "todo" | "unimplemented" => self.parse_format_macro(FmtMac::Panic, close, line),
            "assert" | "debug_assert" => {
                let cond = self.allow_struct(|p| p.parse_expr())?;
                let msg = if self.eat_sym(",") && !self.is_sym(close) { Some(Box::new(self.parse_format_macro(FmtMac::Panic, close, line)?)) } else { self.expect_sym(close)?; None };
                Ok(ex(ExprKind::Assert(Box::new(cond), msg), line))
            }
            "assert_eq" | "assert_ne" | "debug_assert_eq" | "debug_assert_ne" => {
                let (a, b) = self.allow_struct(|p| {
                    let a = p.parse_expr()?;
                    p.expect_sym(",")?;
                    let b = p.parse_expr()?;
                    Ok((a, b))
                })?;
                let msg = if self.eat_sym(",") && !self.is_sym(close) { Some(Box::new(self.parse_format_macro(FmtMac::Panic, close, line)?)) } else { self.expect_sym(close)?; None };
                Ok(ex(ExprKind::AssertEq(name.ends_with("eq"), Box::new(a), Box::new(b), msg), line))
            }
            "matches" => {
                let (e, pat, guard) = self.allow_struct(|p| {
                    let e = p.parse_expr()?;
                    p.expect_sym(",")?;
                    let pat = p.parse_pattern()?;
                    let guard = if p.eat_kw("if") { Some(Box::new(p.parse_expr()?)) } else { None };
                    p.eat_sym(",");
                    Ok((e, pat, guard))
                })?;
                self.expect_sym(close)?;
                Ok(ex(ExprKind::Matches(Box::new(e), pat, guard), line))
            }
            other => Err(ScriptError::new(self.file, line, 0, format!("the macro `{}!` is not part of the script language (known: format!, println!, panic!, assert!, assert_eq!, vec!, matches!)", other))),
        }
    }

    /// The inside of `format!(...)` up to and including the closing bracket.
    fn parse_format_macro(&mut self, mac: FmtMac, close: &str, line: u32) -> R<Expr> {
        if self.eat_sym(close) {
            return Ok(ex(ExprKind::Format { mac, pieces: Vec::new(), args: Vec::new() }, line));
        }
        let text = match self.peek().clone() {
            Tok::Str(s) => {
                self.pos += 1;
                s
            }
            _ => return self.unexpected("a format string"),
        };
        let mut positional: Vec<Expr> = Vec::new();
        let mut named: Vec<(Name, Expr)> = Vec::new();
        self.allow_struct(|p| {
            while p.eat_sym(",") {
                if p.is_sym(close) {
                    break;
                }
                if matches!(p.peek(), Tok::Ident(_)) && p.is_sym_at(1, "=") {
                    let n = p.ident()?;
                    p.pos += 1;
                    named.push((n, p.parse_expr()?));
                } else if !named.is_empty() {
                    return p.err("positional arguments cannot follow named ones");
                } else {
                    positional.push(p.parse_expr()?);
                }
            }
            Ok(())
        })?;
        self.expect_sym(close)?;
        let names: Vec<Name> = named.iter().map(|n| n.0.clone()).collect();
        let (pieces, extras) = match parse_format(&text, positional.len(), &names) {
            Ok(r) => r,
            Err(m) => return Err(ScriptError::new(self.file, line, 0, m)),
        };
        let mut args = positional;
        args.extend(named.into_iter().map(|n| n.1));
        for x in extras {
            args.push(ex(ExprKind::Path(alloc::vec![x]), line));
        }
        Ok(ex(ExprKind::Format { mac, pieces, args }, line))
    }

    // ---- patterns

    fn parse_pattern(&mut self) -> R<Pat> {
        self.enter()?;
        let r = self.parse_pattern_or();
        self.leave();
        r
    }

    fn parse_pattern_or(&mut self) -> R<Pat> {
        let first = self.parse_pat_single()?;
        if !self.is_sym("|") {
            return Ok(first);
        }
        let mut alts = alloc::vec![first];
        while self.eat_sym("|") {
            alts.push(self.parse_pat_single()?);
        }
        Ok(Pat::Or(alts))
    }

    /// A pattern where `|` ends it (closure parameters, `let`, `for`).
    fn parse_pattern_no_or(&mut self) -> R<Pat> {
        self.enter()?;
        let r = self.parse_pat_single();
        self.leave();
        r
    }

    fn lit_value(&mut self) -> R<Option<Value>> {
        let neg = self.is_sym("-");
        let at = if neg { 1 } else { 0 };
        let v = match self.peek_at(at).clone() {
            Tok::Int(n) => Value::Int(if neg { -n } else { n }),
            Tok::Float(f) => Value::Float(if neg { -f } else { f }),
            Tok::Char(c) if !neg => Value::Char(c),
            Tok::Str(s) if !neg => Value::string(s),
            _ => return Ok(None),
        };
        self.pos += at + 1;
        Ok(Some(v))
    }

    fn parse_pat_single(&mut self) -> R<Pat> {
        // A literal, or the start of a range.
        if matches!(self.peek(), Tok::Int(_) | Tok::Float(_) | Tok::Char(_) | Tok::Str(_)) || (self.is_sym("-") && matches!(self.peek_at(1), Tok::Int(_) | Tok::Float(_))) {
            let lo = self.lit_value()?.unwrap_or(Value::Unit);
            if self.is_sym("..=") || self.is_sym("...") || self.is_sym("..") {
                let incl = !self.is_sym("..");
                self.pos += 1;
                let hi = self.lit_value()?;
                return Ok(Pat::Range(Some(lo), hi, incl));
            }
            return Ok(Pat::Lit(lo));
        }
        match self.peek().clone() {
            Tok::Sym("_") => {
                self.pos += 1;
                Ok(Pat::Wild)
            }
            Tok::Sym("..") => {
                self.pos += 1;
                Ok(Pat::Rest)
            }
            Tok::Sym("..=") => {
                self.pos += 1;
                match self.lit_value()? {
                    Some(hi) => Ok(Pat::Range(None, Some(hi), true)),
                    None => self.unexpected("a number or character after `..=`"),
                }
            }
            Tok::Sym("&") | Tok::Sym("&&") => {
                self.pos += 1;
                self.eat_kw("mut");
                self.parse_pat_single()
            }
            Tok::Sym("(") => {
                self.pos += 1;
                let mut items = Vec::new();
                let mut comma = false;
                while !self.is_sym(")") {
                    items.push(self.parse_pattern()?);
                    comma = false;
                    if !self.is_sym(")") {
                        self.expect_sym(",")?;
                        comma = true;
                    }
                }
                self.pos += 1;
                if items.len() == 1 && !comma && !matches!(items[0], Pat::Rest) {
                    Ok(items.remove(0))
                } else {
                    Ok(Pat::Tuple(items))
                }
            }
            Tok::Sym("[") => {
                self.pos += 1;
                let mut items = Vec::new();
                while !self.is_sym("]") {
                    items.push(self.parse_pattern()?);
                    if !self.is_sym("]") {
                        self.expect_sym(",")?;
                    }
                }
                self.pos += 1;
                Ok(Pat::Slice(items))
            }
            Tok::Ident(w) if w == "true" || w == "false" => {
                self.pos += 1;
                Ok(Pat::Lit(Value::Bool(w == "true")))
            }
            Tok::Ident(w) if w == "mut" || w == "ref" => {
                self.pos += 1;
                if w == "ref" {
                    self.eat_kw("mut");
                }
                let name = self.ident()?;
                let sub = if self.eat_sym("@") { Some(Box::new(self.parse_pat_single()?)) } else { None };
                Ok(Pat::Bind(name, sub))
            }
            Tok::Ident(_) => {
                let mut segs = alloc::vec![self.ident()?];
                while self.is_sym("::") {
                    self.pos += 1;
                    segs.push(self.ident()?);
                }
                if self.is_sym("(") {
                    self.pos += 1;
                    let mut items = Vec::new();
                    while !self.is_sym(")") {
                        items.push(self.parse_pattern()?);
                        if !self.is_sym(")") {
                            self.expect_sym(",")?;
                        }
                    }
                    self.pos += 1;
                    return Ok(Pat::TupleStruct(segs, items));
                }
                if self.is_sym("{") {
                    self.pos += 1;
                    let mut fields = Vec::new();
                    while !self.is_sym("}") {
                        if self.eat_sym("..") {
                            break;
                        }
                        self.eat_kw("ref");
                        self.eat_kw("mut");
                        let fname = match self.peek().clone() {
                            Tok::Int(n) => {
                                self.pos += 1;
                                Name::from(format!("{}", n))
                            }
                            _ => self.ident()?,
                        };
                        let pat = if self.eat_sym(":") { self.parse_pattern()? } else { Pat::Bind(fname.clone(), None) };
                        fields.push((fname, pat));
                        if !self.is_sym("}") {
                            self.expect_sym(",")?;
                        }
                    }
                    self.expect_sym("}")?;
                    return Ok(Pat::Struct(segs, fields));
                }
                if segs.len() == 1 {
                    let name = segs.remove(0);
                    let sub = if self.eat_sym("@") { Some(Box::new(self.parse_pat_single()?)) } else { None };
                    Ok(Pat::Bind(name, sub))
                } else {
                    Ok(Pat::Path(segs))
                }
            }
            _ => self.unexpected("a pattern"),
        }
    }
}

/// A `let x: T = <call>` tells the call what it should make: `collect`,
/// `parse`, `sum` and `product` have no other way to know.
fn hint(e: &mut Expr, ty: &Type) {
    match &mut e.kind {
        ExprKind::MethodCall { recv, name, turbofish, .. } => match &**name {
            "collect" | "parse" | "sum" | "product" => {
                if turbofish.is_none() {
                    *turbofish = Some(ty.clone());
                }
            }
            "unwrap" | "expect" | "unwrap_or" | "unwrap_or_default" | "unwrap_or_else" => hint(recv, ty),
            _ => {}
        },
        ExprKind::Try(inner) => hint(inner, ty),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> Program {
        match parse("t.rs", src) {
            Ok(p) => p,
            Err(e) => panic!("{}", e),
        }
    }

    fn fails(src: &str) -> ScriptError {
        parse("t.rs", src).unwrap_err()
    }

    #[test]
    fn functions_and_items() {
        let p = ok("use a::b;\n#[derive(Debug)]\nstruct P { x: i32, y: i32 }\nenum E { A, B(i32), C { n: i32 } = 5 }\nconst N: usize = 3;\nfn f(a: i32, b: &str) -> i32 { a }\nimpl P { fn new() -> P { P { x: 1, y: 2 } } fn sum(&self) -> i32 { self.x + self.y } }");
        assert!(p.fns.contains_key("f"));
        assert_eq!(p.structs["P"].fields.len(), 2);
        assert_eq!(p.enums["E"].variants.len(), 3);
        assert_eq!(p.enums["E"].variants[2].disc, 5);
        assert_eq!(p.consts.len(), 1);
        assert!(p.methods.contains_key(&(Name::from("P"), Name::from("sum"))));
        assert_eq!(p.methods[&(Name::from("P"), Name::from("sum"))].self_kind, Some(SelfKind::Ref));
    }

    #[test]
    fn generics_in_types_and_lets() {
        ok("fn f(v: Vec<Vec<i32>>, o: Option<(i32, String)>) -> Result<(), String> { let x: Vec<Vec<i32>> = Vec::new(); Ok(()) }");
        ok("fn g<T: Clone + Copy>(a: &mut Vec<T>) where T: Clone { }");
        ok("fn h(f: impl Fn(i32) -> i32, g: &dyn Fn(i32)) {}");
    }

    #[test]
    fn precedence() {
        let p = ok("fn f() -> i32 { 1 + 2 * 3 }");
        let body = &p.fns["f"].body;
        match &body.tail.as_ref().unwrap().kind {
            ExprKind::Binary(BinOp::Add, _, r) => assert!(matches!(r.kind, ExprKind::Binary(BinOp::Mul, _, _))),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn negative_literals_fold() {
        let p = ok("fn f() { let x = -5; let y = -2.5; }");
        match &p.fns["f"].body.stmts[0] {
            Stmt::Let { init: Some(e), .. } => assert!(matches!(e.kind, ExprKind::Lit(Value::Int(-5)))),
            _ => panic!(),
        }
    }

    #[test]
    fn control_flow_forms() {
        ok("fn f(x: i32) -> i32 { if x > 1 { 1 } else if x < 0 { 2 } else { 3 } }");
        ok("fn f(v: Vec<i32>) { for (i, x) in v.iter().enumerate() { if *x == 0 { continue; } } while let Some(y) = v.pop() { } 'a: loop { break 'a; } }");
        ok("fn f(x: i32) -> &str { match x { 0 => \"a\", 1 | 2 => \"b\", 3..=9 => \"c\", n if n < 0 => \"d\", _ => \"e\" } }");
        ok("fn f(o: Option<i32>) -> i32 { let Some(v) = o else { return 0 }; if let Some(w) = o { w } else { v } }");
    }

    #[test]
    fn struct_literal_vs_block() {
        ok("struct P { x: i32 } fn f(p: P) -> i32 { if p.x == 1 { 1 } else { 2 } }");
        ok("struct P { x: i32 } fn f() -> P { let x = 1; P { x } }");
        ok("struct P { x: i32 } const Z: i32 = 0; fn f(a: i32) { while a < Z { } for i in 0..Z { } match a { _ => {} } }");
    }

    #[test]
    fn ranges_and_casts() {
        ok("fn f(n: i32) { for i in 0..n { } for i in 0..=n { } let s = &v[1..]; let t = &v[..n]; let x = n as f64 * 2.0; let c = (n as u8 + 1) as char; }");
    }

    #[test]
    fn macros() {
        ok("fn f(a: i32) { let s = format!(\"{} {a} {:?}\", a, a); println!(\"hi\"); println!(); let v = vec![1, 2, 3]; let w = vec![0; 4]; assert!(a > 0, \"bad {}\", a); assert_eq!(a, 1); if matches!(a, 1 | 2) {} panic!(\"no\"); }");
    }

    #[test]
    fn closures() {
        ok("fn f(v: Vec<i32>) { let a = v.iter().map(|x| x * 2).filter(|x| *x > 2).collect::<Vec<i32>>(); let b = |x: i32, y: i32| -> i32 { x + y }; let c = || 5; v.sort_by(|a, b| b.cmp(a)); }");
    }

    #[test]
    fn nested_items_are_hoisted() {
        let p = ok("fn outer() { fn inner() -> i32 { 1 } const K: i32 = 2; inner(); }");
        assert!(p.fns.contains_key("inner"));
        assert_eq!(p.consts.len(), 1);
    }

    #[test]
    fn tuple_index_chain() {
        ok("fn f(t: ((i32, i32), i32)) -> i32 { t.0.1 + t.1 }");
    }

    #[test]
    fn errors_have_lines() {
        let e = fails("fn f() {\n  let x = ;\n}");
        assert_eq!(e.line, 2);
        assert!(e.msg.contains("expected an expression"), "{}", e);
        let e = fails("fn f() {\n  let x = 1\n  let y = 2;\n}");
        assert_eq!(e.line, 3);
        let e = fails("fn f() {");
        assert!(e.msg.contains("never closed"), "{}", e);
        let e = fails("trait T {}");
        assert!(e.msg.contains("traits"));
        let e = fails("fn f() { foo!(1); }");
        assert!(e.msg.contains("macro"));
        assert!(fails("fn f() {}\nfn f() {}").msg.contains("twice"));
        assert_eq!(format!("{}", fails("fn f() { 1 + }")), "t.rs:1:14: expected an expression, found `}`");
    }

    #[test]
    fn deep_nesting_is_refused_not_crashed() {
        let mut s = String::from("fn f() { let x = ");
        for _ in 0..500 {
            s.push('(');
        }
        s.push('1');
        for _ in 0..500 {
            s.push(')');
        }
        s.push_str("; }");
        assert!(fails(&s).msg.contains("nested too deeply"));
    }
}

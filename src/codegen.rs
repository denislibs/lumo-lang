use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::ast::{
    BinOp, Expr, ExprKind, ExternalDecl, FnDecl, Item, Pattern, PatternKind, Stmt, TypeDecl, UnOp,
};
use crate::typed;

const RUNTIME: &str = include_str!("../runtime/lumo.js");

const RESERVED: &[&str] = &[
    "arguments",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "console",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "document",
    "else",
    "enum",
    "eval",
    "export",
    "exports",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "globalThis",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "module",
    "new",
    "null",
    "package",
    "private",
    "process",
    "protected",
    "public",
    "require",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "window",
    "with",
    "yield",
];

/// Собирает программу: рантайм, модули `(префикс, модуль)` по порядку и вызов `main$main()`.
/// Модуль программы имеет префикс `main`, поэтому её глобалы не затеняют глобалы JS.
pub fn emit_program(units: &[(&str, &typed::Module)]) -> String {
    let mut out = String::from(RUNTIME);
    for (prefix, module) in units {
        let mut gen = Gen::new(prefix, module);
        for item in &module.ast.items {
            match item {
                Item::Fn(f) => gen.function(f),
                Item::External(e) => gen.external(e),
                Item::Type(t) => gen.type_decl(t),
            }
        }
        out.push_str(&gen.out);
    }
    out.push_str("main$main();\n");
    out
}

/// JS-имя глобала модуля: `префикс$имя`.
fn mangle(prefix: &str, name: &str) -> String {
    format!("{prefix}${name}")
}

/// Экранирует локальное имя, совпадающее с зарезервированным словом JS.
fn escape(name: &str) -> String {
    if RESERVED.contains(&name) {
        format!("{name}$")
    } else {
        name.to_string()
    }
}

fn js_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn js_op(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add | AddF | Concat => "+",
        Sub | SubF => "-",
        Mul | MulF => "*",
        DivF => "/",
        Lt | LtF => "<",
        Gt | GtF => ">",
        Le | LeF => "<=",
        Ge | GeF => ">=",
        And => "&&",
        Or => "||",
        Div | Rem | Eq | Ne => unreachable!("обрабатываются через рантайм"),
    }
}

struct Gen<'a> {
    prefix: &'a str,
    globals: HashSet<&'a str>,
    /// Конструкторы текущего модуля (остальные — из prelude).
    ctors: HashSet<&'a str>,
    /// Lumo-имя → JS-имя, по вложенным областям.
    scopes: Vec<HashMap<String, String>>,
    /// Сколько раз имя уже связано в текущей функции верхнего уровня.
    used: HashMap<String, u32>,
    /// Счётчик служебных имён `$t1`, `$t2`, …
    temps: u32,
    out: String,
}

impl<'a> Gen<'a> {
    fn new(prefix: &'a str, module: &'a typed::Module) -> Self {
        let globals = module.globals.iter().map(|(n, _)| n.as_str()).collect();
        let ctors = module.ctors.iter().map(|(n, _)| n.as_str()).collect();
        Gen {
            prefix,
            globals,
            ctors,
            scopes: Vec::new(),
            used: HashMap::new(),
            temps: 0,
            out: String::new(),
        }
    }

    fn function(&mut self, f: &FnDecl) {
        self.used = self.globals.iter().map(|g| (g.to_string(), 1)).collect();
        self.scopes.push(HashMap::new());
        let params: Vec<String> = f.params.iter().map(|p| self.bind(&p.name)).collect();
        let body = self.tail(&f.body);
        self.scopes.pop();
        let name = mangle(self.prefix, &f.name);
        let _ = writeln!(
            self.out,
            "function {name}({}) {{\n{body}}}",
            params.join(", ")
        );
    }

    /// Конструкторы типа: с полями — функция, без полей — общий объект.
    fn type_decl(&mut self, t: &TypeDecl) {
        for v in &t.variants {
            let name = mangle(self.prefix, &v.name);
            let tag = js_string(&v.name);
            if v.fields.is_empty() {
                let _ = writeln!(self.out, "const {name} = {{ $: {tag} }};");
            } else {
                let params: Vec<String> = (0..v.fields.len()).map(|i| format!("_{i}")).collect();
                let fields: Vec<String> = params.iter().map(|p| format!("{p}: {p}")).collect();
                let _ = writeln!(
                    self.out,
                    "function {name}({}) {{ return {{ $: {tag}, {} }}; }}",
                    params.join(", "),
                    fields.join(", ")
                );
            }
        }
    }

    fn external(&mut self, e: &ExternalDecl) {
        let params: Vec<String> = e.params.iter().map(|p| escape(&p.name)).collect();
        let list = params.join(", ");
        let name = mangle(self.prefix, &e.name);
        let _ = writeln!(
            self.out,
            "function {name}({list}) {{ return ({})({list}); }}",
            e.js
        );
    }

    /// Связывает локальное имя в текущей области и возвращает уникальное JS-имя.
    fn bind(&mut self, name: &str) -> String {
        let n = self.used.entry(name.to_string()).or_insert(0);
        let js = if *n == 0 {
            escape(name)
        } else {
            format!("{name}${n}")
        };
        *n += 1;
        self.scopes
            .last_mut()
            .expect("bind вызывается внутри области")
            .insert(name.to_string(), js.clone());
        js
    }

    /// Свежее служебное имя (`$` не встречается в именах Lumo).
    fn temp(&mut self) -> String {
        self.temps += 1;
        format!("$t{}", self.temps)
    }

    /// `const` для каждого имени из образца `let`; значение вычисляется один раз.
    fn bind_let(&mut self, pattern: &Pattern, value: String) -> String {
        if let PatternKind::Var(name) = &pattern.kind {
            let js = self.bind(name);
            return format!("const {js} = {value};\n");
        }
        let tmp = self.temp();
        let mut out = format!("const {tmp} = {value};\n");
        for (name, access) in pattern_accesses(pattern, &tmp) {
            let js = self.bind(&name);
            let _ = writeln!(out, "const {js} = {access};");
        }
        out
    }

    fn var(&self, name: &str) -> String {
        for scope in self.scopes.iter().rev() {
            if let Some(js) = scope.get(name) {
                return js.clone();
            }
        }
        if self.globals.contains(name) {
            mangle(self.prefix, name)
        } else {
            mangle("prelude", name)
        }
    }

    /// Операторы JS, которые вычисляют `e` и делают `return`.
    fn tail(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Block(stmts) => {
                self.scopes.push(HashMap::new());
                let mut out = String::new();
                for (i, s) in stmts.iter().enumerate() {
                    let is_last = i + 1 == stmts.len();
                    match s {
                        Stmt::Let { pattern, value, .. } => {
                            let v = self.expr(value);
                            out.push_str(&self.bind_let(pattern, v));
                        }
                        Stmt::Expr(e) if is_last => out.push_str(&self.tail(e)),
                        Stmt::Expr(e) => {
                            let v = self.expr(e);
                            let _ = writeln!(out, "{v};");
                        }
                    }
                }
                if !matches!(stmts.last(), Some(Stmt::Expr(_))) {
                    out.push_str("return undefined;\n");
                }
                self.scopes.pop();
                out
            }
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.expr(cond);
                let then = self.tail(then_branch);
                match else_branch {
                    Some(else_branch) => {
                        let els = self.tail(else_branch);
                        format!("if ({c}) {{\n{then}}} else {{\n{els}}}\n")
                    }
                    None => format!("if ({c}) {{\n{then}}}\nreturn undefined;\n"),
                }
            }
            _ => format!("return {};\n", self.expr(e)),
        }
    }

    fn expr(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(n) => n.to_string(),
            ExprKind::Float(f) => format!("{f:?}"),
            ExprKind::Str(s) => js_string(s),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Var(name) => self.var(name),
            ExprKind::Ctor(name) => {
                if self.ctors.contains(name.as_str()) {
                    mangle(self.prefix, name)
                } else {
                    mangle("prelude", name)
                }
            }
            ExprKind::ModuleAccess { module, name } => mangle(module, name),
            ExprKind::Unary { op, expr } => {
                let x = self.expr(expr);
                match op {
                    UnOp::Neg | UnOp::NegF => format!("(-{x})"),
                    UnOp::Not => format!("(!{x})"),
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                match op {
                    BinOp::Div => format!("$idiv({l}, {r})"),
                    BinOp::Rem => format!("$imod({l}, {r})"),
                    BinOp::Eq => format!("$eq({l}, {r})"),
                    BinOp::Ne => format!("(!$eq({l}, {r}))"),
                    _ => format!("({l} {} {r})", js_op(*op)),
                }
            }
            ExprKind::Call { callee, args } => {
                let f = self.expr(callee);
                let args: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                format!("{f}({})", args.join(", "))
            }
            ExprKind::Lambda { params, body } => {
                self.scopes.push(HashMap::new());
                let ps: Vec<String> = params.iter().map(|p| self.bind(&p.name)).collect();
                let b = self.tail(body);
                self.scopes.pop();
                format!("(({}) => {{\n{b}}})", ps.join(", "))
            }
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.expr(cond);
                let t = self.expr(then_branch);
                let other = match else_branch {
                    Some(e) => self.expr(e),
                    None => "undefined".to_string(),
                };
                format!("({c} ? {t} : {other})")
            }
            ExprKind::List { items, tail } => {
                let tail = match tail {
                    Some(t) => self.expr(t),
                    None => "null".to_string(),
                };
                if items.is_empty() {
                    tail
                } else {
                    let items: Vec<String> = items.iter().map(|i| self.expr(i)).collect();
                    format!("$list([{}], {tail})", items.join(", "))
                }
            }
            ExprKind::Tuple(items) => {
                let items: Vec<String> = items.iter().map(|i| self.expr(i)).collect();
                format!("[{}]", items.join(", "))
            }
            ExprKind::Block(stmts) => match stmts.as_slice() {
                [Stmt::Expr(only)] => self.expr(only),
                _ => format!("(() => {{\n{}}})()", self.tail(e)),
            },
        }
    }
}

/// Пары (имя Lumo, JS-путь к значению) для имён, которые связывает образец.
fn pattern_accesses(p: &Pattern, access: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    collect_accesses(p, access.to_string(), &mut out);
    out
}

fn collect_accesses(p: &Pattern, access: String, out: &mut Vec<(String, String)>) {
    match &p.kind {
        PatternKind::Wildcard => {}
        PatternKind::Var(name) => out.push((name.clone(), access)),
        PatternKind::Tuple(items) => {
            for (i, item) in items.iter().enumerate() {
                collect_accesses(item, format!("{access}[{i}]"), out);
            }
        }
    }
}

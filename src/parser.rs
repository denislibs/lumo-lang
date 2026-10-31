use crate::ast::{BinOp, Expr, ExprKind, ExternalDecl, FnDecl, Item, Module, Param, Stmt, TypeExpr, UnOp};
use crate::diagnostics::{Diag, Span};
use crate::lexer::{Tok, Token};

type PResult<T> = Result<T, Diag>;

/// Разбирает одно выражение (тесты, REPL).
pub fn parse_expr(tokens: Vec<Token>) -> Result<Expr, Vec<Diag>> {
    let mut p = Parser::new(tokens);
    p.skip_newlines();
    let result = p.expr().and_then(|e| {
        p.skip_newlines();
        if p.at(&Tok::Eof) {
            Ok(e)
        } else {
            Err(Diag::error(p.span(), "лишний текст после выражения"))
        }
    });
    result.map_err(|d| vec![d])
}

/// Разбирает файл. Ошибки собираются по всем объявлениям.
pub fn parse(tokens: Vec<Token>) -> Result<Module, Vec<Diag>> {
    let mut p = Parser::new(tokens);
    let module = p.module();
    if p.diags.is_empty() {
        Ok(module)
    } else {
        Err(p.diags)
    }
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    diags: Vec<Diag>,
}

impl Parser {
    fn new(toks: Vec<Token>) -> Self {
        Parser { toks, pos: 0, diags: Vec::new() }
    }

    fn module(&mut self) -> Module {
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::Eof) {
                break;
            }
            let start = self.pos;
            match self.item() {
                Ok(item) => items.push(item),
                Err(d) => {
                    self.diags.push(d);
                    // Гарантия прогресса: если ошибка не съела ни одного токена, съедаем один.
                    if self.pos == start {
                        self.advance();
                    }
                    self.synchronize();
                }
            }
        }
        Module { items }
    }

    /// Пропускает токены до начала следующего объявления на новой строке.
    fn synchronize(&mut self) {
        while !self.at(&Tok::Eof) {
            let after_newline = self.pos > 0 && self.toks[self.pos - 1].tok == Tok::Newline;
            if after_newline && matches!(self.peek(), Tok::Fn | Tok::At | Tok::Type) {
                return;
            }
            self.advance();
        }
    }

    fn item(&mut self) -> PResult<Item> {
        match self.peek() {
            Tok::Fn => Ok(Item::Fn(self.fn_decl()?)),
            Tok::At => Ok(Item::External(self.external()?)),
            _ => Err(Diag::error(self.span(), "ожидалось объявление: fn или @external")),
        }
    }

    fn fn_decl(&mut self) -> PResult<FnDecl> {
        let start = self.expect(&Tok::Fn, "fn")?;
        let (name, _) = self.ident()?;
        let params = self.params()?;
        let ret = if self.eat(&Tok::Arrow) { Some(self.type_expr()?) } else { None };
        let body = self.block()?;
        Ok(FnDecl { name, params, ret, span: start.to(body.span), body })
    }

    fn external(&mut self) -> PResult<ExternalDecl> {
        let start = self.expect(&Tok::At, "@")?;
        let (attr, attr_span) = self.ident()?;
        if attr != "external" {
            return Err(Diag::error(attr_span, format!("неизвестный атрибут @{attr}")));
        }
        self.expect(&Tok::LParen, "(")?;
        let t = self.advance();
        let Tok::Str(js) = t.tok else {
            return Err(Diag::error(t.span, "ожидалась строка с JS-выражением"));
        };
        self.expect(&Tok::RParen, ")")?;
        self.skip_newlines();
        self.expect(&Tok::Fn, "fn")?;
        let (name, _) = self.ident()?;
        let params = self.params()?;
        if let Some(p) = params.iter().find(|p| p.ty.is_none()) {
            return Err(Diag::error(p.span, "у @external-функции типы параметров обязательны"));
        }
        self.expect(&Tok::Arrow, "->")?;
        let ret = self.type_expr()?;
        let span = start.to(ret.span());
        Ok(ExternalDecl { js, name, params, ret, span })
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn span(&self) -> Span {
        self.toks[self.pos].span
    }

    fn at(&self, tok: &Tok) -> bool {
        self.peek() == tok
    }

    fn advance(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if t.tok != Tok::Eof {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.at(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: &Tok, what: &str) -> PResult<Span> {
        if self.at(tok) {
            Ok(self.advance().span)
        } else {
            Err(Diag::error(self.span(), format!("ожидалось `{what}`")))
        }
    }

    fn skip_newlines(&mut self) {
        while self.eat(&Tok::Newline) {}
    }

    fn ident(&mut self) -> PResult<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let span = self.advance().span;
                Ok((name, span))
            }
            _ => Err(Diag::error(self.span(), "ожидалось имя")),
        }
    }

    fn params(&mut self) -> PResult<Vec<Param>> {
        self.expect(&Tok::LParen, "(")?;
        let mut params = Vec::new();
        while !self.at(&Tok::RParen) {
            let (name, span) = self.ident()?;
            let ty = if self.eat(&Tok::Colon) { Some(self.type_expr()?) } else { None };
            params.push(Param { name, ty, span });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RParen, ")")?;
        Ok(params)
    }

    fn type_expr(&mut self) -> PResult<TypeExpr> {
        let t = self.advance();
        match t.tok {
            Tok::Upper(name) => {
                let mut args = Vec::new();
                let mut end = t.span;
                if self.eat(&Tok::LParen) {
                    while !self.at(&Tok::RParen) {
                        args.push(self.type_expr()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    end = self.expect(&Tok::RParen, ")")?;
                }
                Ok(TypeExpr::Named { name, args, span: t.span.to(end) })
            }
            Tok::Ident(name) => Ok(TypeExpr::Var { name, span: t.span }),
            Tok::Fn => {
                self.expect(&Tok::LParen, "(")?;
                let mut params = Vec::new();
                while !self.at(&Tok::RParen) {
                    params.push(self.type_expr()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RParen, ")")?;
                self.expect(&Tok::Arrow, "->")?;
                let ret = self.type_expr()?;
                let span = t.span.to(ret.span());
                Ok(TypeExpr::Fn { params, ret: Box::new(ret), span })
            }
            _ => Err(Diag::error(t.span, "ожидался тип")),
        }
    }

    fn block(&mut self) -> PResult<Expr> {
        let start = self.expect(&Tok::LBrace, "{")?;
        let mut stmts = Vec::new();
        loop {
            while self.eat(&Tok::Newline) || self.eat(&Tok::Semi) {}
            if self.at(&Tok::RBrace) {
                break;
            }
            stmts.push(self.stmt()?);
            if !matches!(self.peek(), Tok::Newline | Tok::Semi | Tok::RBrace) {
                return Err(Diag::error(self.span(), "ожидался перевод строки или `}`"));
            }
        }
        let end = self.expect(&Tok::RBrace, "}")?;
        Ok(Expr { kind: ExprKind::Block(stmts), span: start.to(end) })
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        if !self.at(&Tok::Let) {
            return Ok(Stmt::Expr(self.expr()?));
        }
        let start = self.advance().span;
        let (name, _) = self.ident()?;
        let ty = if self.eat(&Tok::Colon) { Some(self.type_expr()?) } else { None };
        self.expect(&Tok::Eq, "=")?;
        let value = self.expr()?;
        let span = start.to(value.span);
        Ok(Stmt::Let { name, ty, value, span })
    }

    fn expr(&mut self) -> PResult<Expr> {
        self.binary(0)
    }

    fn binary(&mut self, min_prec: u8) -> PResult<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let Some((prec, op)) = binop(self.peek()) else { break };
            if prec < min_prec {
                break;
            }
            self.advance();
            let rhs = self.binary(prec + 1)?;
            let span = lhs.span.to(rhs.span);
            lhs = match op {
                None => pipe(lhs, rhs),
                Some(op) => Expr {
                    kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                    span,
                },
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::MinusDot => UnOp::NegF,
            Tok::Bang => UnOp::Not,
            _ => return self.postfix(),
        };
        let start = self.advance().span;
        let inner = self.unary()?;
        let span = start.to(inner.span);
        Ok(Expr { kind: ExprKind::Unary { op, expr: Box::new(inner) }, span })
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.eat(&Tok::LParen) {
                let mut args = Vec::new();
                while !self.at(&Tok::RParen) {
                    args.push(self.expr()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                let end = self.expect(&Tok::RParen, ")")?;
                let span = e.span.to(end);
                e = Expr { kind: ExprKind::Call { callee: Box::new(e), args }, span };
            } else if self.eat(&Tok::Dot) {
                let (name, name_span) = self.ident()?;
                let span = e.span.to(name_span);
                e = match e.kind {
                    ExprKind::Var(module) => Expr { kind: ExprKind::ModuleAccess { module, name }, span },
                    _ => {
                        return Err(Diag::error(
                            span,
                            "доступ через точку возможен только к функциям модулей, например list.map",
                        ))
                    }
                };
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> PResult<Expr> {
        if self.at(&Tok::LBrace) {
            return self.block();
        }
        let t = self.advance();
        let kind = match t.tok {
            Tok::Int(n) => ExprKind::Int(n),
            Tok::Float(f) => ExprKind::Float(f),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::Ident(name) => ExprKind::Var(name),
            Tok::LParen => {
                let inner = self.expr()?;
                self.expect(&Tok::RParen, ")")?;
                return Ok(inner);
            }
            Tok::If => return self.if_expr(t.span),
            Tok::Fn => return self.lambda(t.span),
            _ => return Err(Diag::error(t.span, "ожидалось выражение")),
        };
        Ok(Expr { kind, span: t.span })
    }

    fn if_expr(&mut self, start: Span) -> PResult<Expr> {
        let cond = self.expr()?;
        let then_branch = self.block()?;
        let mut end = then_branch.span;
        let else_branch = if self.eat(&Tok::Else) {
            let e = if self.at(&Tok::If) {
                let s = self.advance().span;
                self.if_expr(s)?
            } else {
                self.block()?
            };
            end = e.span;
            Some(Box::new(e))
        } else {
            None
        };
        Ok(Expr {
            kind: ExprKind::If { cond: Box::new(cond), then_branch: Box::new(then_branch), else_branch },
            span: start.to(end),
        })
    }

    fn lambda(&mut self, start: Span) -> PResult<Expr> {
        let params = self.params()?;
        let body = self.block()?;
        let span = start.to(body.span);
        Ok(Expr { kind: ExprKind::Lambda { params, body: Box::new(body) }, span })
    }
}

/// Приоритет и оператор; `None` вместо оператора — пайп.
fn binop(tok: &Tok) -> Option<(u8, Option<BinOp>)> {
    use BinOp::*;
    let (prec, op) = match tok {
        Tok::Pipe => return Some((1, None)),
        Tok::OrOr => (2, Or),
        Tok::AndAnd => (3, And),
        Tok::EqEq => (4, Eq),
        Tok::NotEq => (4, Ne),
        Tok::Lt => (5, Lt),
        Tok::Gt => (5, Gt),
        Tok::Le => (5, Le),
        Tok::Ge => (5, Ge),
        Tok::LtDot => (5, LtF),
        Tok::GtDot => (5, GtF),
        Tok::LeDot => (5, LeF),
        Tok::GeDot => (5, GeF),
        Tok::Plus => (6, Add),
        Tok::Minus => (6, Sub),
        Tok::PlusDot => (6, AddF),
        Tok::MinusDot => (6, SubF),
        Tok::Concat => (6, Concat),
        Tok::Star => (7, Mul),
        Tok::Slash => (7, Div),
        Tok::Percent => (7, Rem),
        Tok::StarDot => (7, MulF),
        Tok::SlashDot => (7, DivF),
        _ => return None,
    };
    Some((prec, Some(op)))
}

/// `a |> f(b)` → `f(a, b)`; `a |> f` → `f(a)`.
fn pipe(lhs: Expr, rhs: Expr) -> Expr {
    let span = lhs.span.to(rhs.span);
    match rhs.kind {
        ExprKind::Call { callee, mut args } => {
            args.insert(0, lhs);
            Expr { kind: ExprKind::Call { callee, args }, span }
        }
        other => {
            let callee = Expr { kind: other, span: rhs.span };
            Expr { kind: ExprKind::Call { callee: Box::new(callee), args: vec![lhs] }, span }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::sexpr;

    fn p(src: &str) -> String {
        sexpr::expr(&parse_expr(lex(src).unwrap()).unwrap())
    }

    fn perr(src: &str) -> String {
        parse_expr(lex(src).unwrap()).unwrap_err()[0].message.clone()
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(p("1 + 2 * 3"), "(+ 1 (* 2 3))");
        assert_eq!(p("a - b - c"), "(- (- a b) c)");
        assert_eq!(p("a == b && c < d || e"), "(|| (&& (== a b) (< c d)) e)");
        assert_eq!(p("(1 + 2) * 3"), "(* (+ 1 2) 3)");
    }

    #[test]
    fn unary_operators() {
        assert_eq!(p("-x +. 1.5"), "(+. (- x) 1.5)");
        assert_eq!(p("!f(x)"), "(! (call f x))");
    }

    #[test]
    fn pipe_inserts_first_argument() {
        assert_eq!(p("x |> f(1) |> g"), "(call g (call f x 1))");
    }

    #[test]
    fn module_access_and_calls() {
        assert_eq!(p("list.map(xs, f)"), "(call list.map xs f)");
        assert_eq!(p("f(a)(b)"), "(call (call f a) b)");
    }

    #[test]
    fn lambdas_ifs_blocks() {
        assert_eq!(p("fn(x, y) { x }"), "(fn (x y) (block x))");
        assert_eq!(
            p("if a { 1 } else if b { 2 } else { 3 }"),
            "(if a (block 1) (if b (block 2) (block 3)))"
        );
        assert_eq!(p("{\n let x = 1\n x + 1\n}"), "(block (let x 1) (+ x 1))");
        assert_eq!(p("\"hi\" <> s"), "(<> \"hi\" s)");
        assert_eq!(p("{}"), "(block)");
    }

    #[test]
    fn multiline_pipeline() {
        assert_eq!(p("{\n  xs\n  |> f\n  |> g(1)\n}"), "(block (call g (call f xs) 1))");
    }

    #[test]
    fn syntax_errors() {
        assert!(perr("1 +").contains("ожидалось выражение"));
        assert!(perr("f(1, 2").contains("ожидалось"));
        assert!(perr("(1 + 2).x").contains("доступ через точку"));
        assert!(perr("1 2").contains("лишний текст"));
    }

    use crate::ast::{Item, Module};

    fn module(src: &str) -> Module {
        parse(lex(src).unwrap()).unwrap()
    }

    fn module_errs(src: &str) -> Vec<Diag> {
        parse(lex(src).unwrap()).unwrap_err()
    }

    #[test]
    fn function_declaration_with_annotations() {
        let m = module("fn add(a: Int, b) -> Int { a + b }");
        let Item::Fn(f) = &m.items[0] else { panic!("ожидалась функция") };
        assert_eq!(f.name, "add");
        assert_eq!(sexpr::type_expr(f.params[0].ty.as_ref().unwrap()), "Int");
        assert!(f.params[1].ty.is_none());
        assert_eq!(sexpr::type_expr(f.ret.as_ref().unwrap()), "Int");
        assert_eq!(sexpr::expr(&f.body), "(block (+ a b))");
    }

    #[test]
    fn function_and_generic_types() {
        let m = module("fn f(g: fn(a, List(Int)) -> b) { g }");
        let Item::Fn(f) = &m.items[0] else { panic!("ожидалась функция") };
        assert_eq!(sexpr::type_expr(f.params[0].ty.as_ref().unwrap()), "fn(a, List(Int)) -> b");
    }

    #[test]
    fn external_declaration() {
        let m = module("@external(\"Math.sqrt\")\nfn sqrt(x: Float) -> Float");
        let Item::External(e) = &m.items[0] else { panic!("ожидался @external") };
        assert_eq!(e.js, "Math.sqrt");
        assert_eq!(e.name, "sqrt");
        assert_eq!(sexpr::type_expr(&e.ret), "Float");
    }

    #[test]
    fn external_requires_types() {
        let errs = module_errs("@external(\"f\")\nfn f(x) -> Int");
        assert!(errs[0].message.contains("обязательны"));
    }

    #[test]
    fn several_items_with_comments() {
        let m = module("// модуль\nfn a() { 1 }\n\n\nfn b() {\n  a()\n}\n");
        let names: Vec<&str> = m.items.iter().map(|i| i.name()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn recovers_and_reports_all_errors() {
        let errs = module_errs("fn a() { 1 + }\nfn b() { 2 }\nfn c() { ) }");
        assert_eq!(errs.len(), 2, "{errs:?}");
    }

    #[test]
    fn rejects_unknown_top_level() {
        let errs = module_errs("let x = 1");
        assert!(errs[0].message.contains("ожидалось объявление"));
    }

    #[test]
    fn recovery_always_makes_progress() {
        let errs = module_errs("fn a() { 1 }\ntype T\nfn b() { 2 }");
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].message.contains("ожидалось объявление"));
    }

    #[test]
    fn recovery_keeps_boundary_after_consumed_newline() {
        let errs = module_errs("fn a() { if\nfn b() { ) }");
        assert_eq!(errs.len(), 2, "{errs:?}");
    }
}

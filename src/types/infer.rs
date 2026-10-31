use std::collections::HashMap;

use crate::ast::{BinOp, Expr, ExprKind, Stmt, TypeExpr, UnOp};
use crate::diagnostics::{Diag, Span};

use super::env::Env;
use super::ty::{display, Scheme, Type};
use super::unify::{Subst, UnifyError};

/// Выводит обобщённый тип одного выражения (тесты, `:t` в REPL).
pub fn type_of_expr(expr: &Expr, env: &Env) -> Result<Scheme, Vec<Diag>> {
    let mut c = Checker::new(env);
    c.level = 1;
    let ty = c.infer(expr);
    c.level = 0;
    let scheme = c.subst.generalize(&ty, 0);
    c.finish(scheme)
}

struct Checker {
    env: Env,
    subst: Subst,
    level: u32,
    locals: Vec<(String, Scheme)>,
    diags: Vec<Diag>,
}

impl Checker {
    fn new(env: &Env) -> Self {
        Checker {
            env: env.clone(),
            subst: Subst::new(),
            level: 0,
            locals: Vec::new(),
            diags: Vec::new(),
        }
    }

    fn finish<T>(self, value: T) -> Result<T, Vec<Diag>> {
        if self.diags.is_empty() {
            Ok(value)
        } else {
            Err(self.diags)
        }
    }

    fn infer(&mut self, e: &Expr) -> Type {
        match &e.kind {
            ExprKind::Int(_) => Type::int(),
            ExprKind::Float(_) => Type::float(),
            ExprKind::Str(_) => Type::string(),
            ExprKind::Bool(_) => Type::bool(),
            ExprKind::Var(name) => self.lookup(name, e.span),
            ExprKind::ModuleAccess { module, name } => self.module_access(module, name, e.span),
            ExprKind::Unary { op, expr } => {
                let expected = match op {
                    UnOp::Neg => Type::int(),
                    UnOp::NegF => Type::float(),
                    UnOp::Not => Type::bool(),
                };
                let actual = self.infer(expr);
                self.expect(expr.span, &actual, &expected);
                expected
            }
            ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs),
            ExprKind::Call { callee, args } => self.call(callee, args, e.span),
            ExprKind::Lambda { params, body } => {
                let base = self.locals.len();
                let mut vars = HashMap::new();
                let tys: Vec<Type> = params
                    .iter()
                    .map(|p| self.annotation(p.ty.as_ref(), &mut vars))
                    .collect();
                for (p, t) in params.iter().zip(&tys) {
                    self.locals.push((p.name.clone(), Scheme::mono(t.clone())));
                }
                let ret = self.infer(body);
                self.locals.truncate(base);
                Type::Fn(tys, Box::new(ret))
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                let c = self.infer(cond);
                self.expect(cond.span, &c, &Type::bool());
                let then_ty = self.infer(then_branch);
                match else_branch {
                    Some(else_branch) => {
                        let else_ty = self.infer(else_branch);
                        self.expect(else_branch.span, &else_ty, &then_ty);
                        then_ty
                    }
                    None => {
                        if !self.expect(then_branch.span, &then_ty, &Type::unit()) {
                            self.hint_last("у if без else ветка должна возвращать Unit — добавь else");
                        }
                        Type::unit()
                    }
                }
            }
            ExprKind::Block(stmts) => self.block(stmts),
        }
    }

    fn block(&mut self, stmts: &[Stmt]) -> Type {
        let base = self.locals.len();
        let mut last = Type::unit();
        for stmt in stmts {
            last = match stmt {
                Stmt::Let { name, ty, value, .. } => {
                    self.level += 1;
                    let actual = self.infer(value);
                    if let Some(t) = ty {
                        let expected = self.convert(t, &mut HashMap::new());
                        self.expect(value.span, &actual, &expected);
                    }
                    self.level -= 1;
                    let scheme = self.subst.generalize(&actual, self.level);
                    self.locals.push((name.clone(), scheme));
                    Type::unit()
                }
                Stmt::Expr(e) => self.infer(e),
            };
        }
        self.locals.truncate(base);
        last
    }

    fn lookup(&mut self, name: &str, span: Span) -> Type {
        let scheme = self
            .locals
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s.clone())
            .or_else(|| self.env.values.get(name).cloned());
        match scheme {
            Some(s) => self.subst.instantiate(&s, self.level),
            None => {
                self.diags.push(Diag::error(span, format!("неизвестное имя `{name}`")));
                self.subst.fresh(self.level)
            }
        }
    }

    fn module_access(&mut self, module: &str, name: &str, span: Span) -> Type {
        let scheme = match self.env.modules.get(module) {
            None => Err(format!("неизвестный модуль `{module}`")),
            Some(m) => m
                .get(name)
                .cloned()
                .ok_or_else(|| format!("в модуле `{module}` нет `{name}`")),
        };
        match scheme {
            Ok(s) => self.subst.instantiate(&s, self.level),
            Err(msg) => {
                self.diags.push(Diag::error(span, msg));
                self.subst.fresh(self.level)
            }
        }
    }

    fn binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Type {
        use BinOp::*;
        let l = self.infer(lhs);
        let r = self.infer(rhs);
        let (operand, result) = match op {
            Add | Sub | Mul | Div | Rem => (Type::int(), Type::int()),
            AddF | SubF | MulF | DivF => (Type::float(), Type::float()),
            Lt | Gt | Le | Ge => (Type::int(), Type::bool()),
            LtF | GtF | LeF | GeF => (Type::float(), Type::bool()),
            And | Or => (Type::bool(), Type::bool()),
            Concat => (Type::string(), Type::string()),
            Eq | Ne => {
                self.expect(rhs.span, &r, &l);
                return Type::bool();
            }
        };
        for (side, t) in [(lhs, &l), (rhs, &r)] {
            if !self.expect(side.span, t, &operand)
                && operand == Type::int()
                && self.subst.zonk(t) == Type::float()
            {
                self.hint_last(&format!("для Float используй {}.", op.symbol()));
            }
        }
        result
    }

    fn call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Type {
        let f = self.infer(callee);
        let arg_tys: Vec<Type> = args.iter().map(|a| self.infer(a)).collect();
        match self.subst.shallow(&f) {
            Type::Fn(params, ret) => {
                if params.len() != args.len() {
                    self.diags.push(Diag::error(
                        span,
                        format!(
                            "функция принимает аргументов: {}, а передано: {}",
                            params.len(),
                            args.len()
                        ),
                    ));
                    return *ret;
                }
                for ((arg, actual), expected) in args.iter().zip(&arg_tys).zip(&params) {
                    self.expect(arg.span, actual, expected);
                }
                *ret
            }
            Type::Var(_) => {
                let ret = self.subst.fresh(self.level);
                let expected = Type::Fn(arg_tys, Box::new(ret.clone()));
                self.expect(callee.span, &f, &expected);
                ret
            }
            other => {
                let shown = display(&self.subst.zonk(&other));
                self.diags.push(Diag::error(
                    callee.span,
                    format!("это не функция, а значение типа {shown}"),
                ));
                self.subst.fresh(self.level)
            }
        }
    }

    /// Тип из аннотации или свежая переменная, если аннотации нет.
    fn annotation(&mut self, t: Option<&TypeExpr>, vars: &mut HashMap<String, Type>) -> Type {
        match t {
            Some(t) => self.convert(t, vars),
            None => self.subst.fresh(self.level),
        }
    }

    /// Аннотация → тип. `vars` — переменные типов, видимые в объявлении.
    fn convert(&mut self, t: &TypeExpr, vars: &mut HashMap<String, Type>) -> Type {
        match t {
            TypeExpr::Named { name, args, span } => {
                if !matches!(name.as_str(), "Int" | "Float" | "Bool" | "String" | "Unit") {
                    self.diags.push(Diag::error(*span, format!("неизвестный тип `{name}`")));
                    return self.subst.fresh(self.level);
                }
                if !args.is_empty() {
                    self.diags.push(Diag::error(*span, format!("тип `{name}` не принимает параметров")));
                }
                Type::con(name)
            }
            TypeExpr::Var { name, .. } => {
                let level = self.level;
                vars.entry(name.clone())
                    .or_insert_with(|| self.subst.fresh(level))
                    .clone()
            }
            TypeExpr::Fn { params, ret, .. } => {
                let params = params.iter().map(|p| self.convert(p, vars)).collect();
                let ret = self.convert(ret, vars);
                Type::Fn(params, Box::new(ret))
            }
        }
    }

    /// Унифицирует и записывает ошибку при несовпадении. Возвращает `true`, если типы совместимы.
    fn expect(&mut self, span: Span, actual: &Type, expected: &Type) -> bool {
        match self.subst.unify(actual, expected) {
            Ok(()) => true,
            Err(UnifyError::Occurs) => {
                self.diags.push(Diag::error(
                    span,
                    "бесконечный тип: значение используется как часть самого себя",
                ));
                false
            }
            Err(UnifyError::Mismatch) => {
                let expected = display(&self.subst.zonk(expected));
                let actual = display(&self.subst.zonk(actual));
                self.diags.push(Diag::error(
                    span,
                    format!("несовпадение типов: ожидался {expected}, а здесь {actual}"),
                ));
                false
            }
        }
    }

    fn hint_last(&mut self, hint: &str) {
        if let Some(d) = self.diags.last_mut() {
            d.hint = Some(hint.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse_expr;

    fn expr(src: &str) -> Expr {
        parse_expr(lex(src).unwrap()).unwrap()
    }

    fn t(src: &str) -> String {
        display(&type_of_expr(&expr(src), &Env::default()).unwrap().ty)
    }

    fn errs(src: &str) -> Vec<Diag> {
        type_of_expr(&expr(src), &Env::default()).unwrap_err()
    }

    fn err(src: &str) -> String {
        errs(src)[0].message.clone()
    }

    #[test]
    fn literals_and_operators() {
        assert_eq!(t("1 + 2"), "Int");
        assert_eq!(t("1.5 *. 2.0"), "Float");
        assert_eq!(t("\"a\" <> \"b\""), "String");
        assert_eq!(t("1 < 2 && true"), "Bool");
        assert_eq!(t("{}"), "Unit");
    }

    #[test]
    fn polymorphic_functions() {
        assert_eq!(t("fn(x) { x }"), "fn(a) -> a");
        assert_eq!(t("fn(f, x) { f(f(x)) }"), "fn(fn(a) -> a, a) -> a");
        assert_eq!(t("fn(x, y) { x == y }"), "fn(a, a) -> Bool");
    }

    #[test]
    fn let_polymorphism() {
        assert_eq!(t("{\n let id = fn(x) { x }\n let a = id(1)\n id(\"s\")\n}"), "String");
    }

    #[test]
    fn lambda_params_are_monomorphic() {
        assert!(err("fn(f) {\n let a = f(1)\n f(\"s\")\n}").contains("несовпадение типов"));
    }

    #[test]
    fn if_expressions() {
        assert_eq!(t("if true { 1 } else { 2 }"), "Int");
        assert!(err("if 1 { 2 } else { 3 }").contains("ожидался Bool, а здесь Int"));
        let d = &errs("if true { 1 }")[0];
        assert!(d.message.contains("ожидался Unit, а здесь Int"));
        assert!(d.hint.as_deref().unwrap().contains("добавь else"));
    }

    #[test]
    fn float_operator_hint() {
        let d = &errs("1 + 2.5")[0];
        assert!(d.message.contains("ожидался Int, а здесь Float"));
        assert_eq!(d.hint.as_deref(), Some("для Float используй +."));
    }

    #[test]
    fn name_and_call_errors() {
        assert!(err("foo").contains("неизвестное имя `foo`"));
        assert!(err("(fn(x) { x })(1, 2)").contains("передано: 2"));
        assert!(err("1(2)").contains("это не функция, а значение типа Int"));
        assert!(err("fn(x) { x(x) }").contains("бесконечный тип"));
    }

    #[test]
    fn annotations() {
        assert_eq!(t("fn(x: Int) { x }"), "fn(Int) -> Int");
        assert!(err("fn(x: Strin) { x }").contains("неизвестный тип `Strin`"));
        assert!(err("{\n let x: Int = \"s\"\n x\n}").contains("несовпадение типов"));
    }
}

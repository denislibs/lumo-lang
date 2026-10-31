//! Компактная печать AST для тестов: `(+ 1 (* 2 3))`.

use crate::ast::{Expr, ExprKind, Pattern, PatternKind, Stmt, TypeExpr};

pub fn expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(n) => n.to_string(),
        ExprKind::Float(f) => format!("{f:?}"),
        ExprKind::Str(s) => format!("{s:?}"),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Var(name) => name.clone(),
        ExprKind::Ctor(name) => name.clone(),
        ExprKind::ModuleAccess { module, name } => format!("{module}.{name}"),
        ExprKind::Unary { op, expr: inner } => format!("({} {})", op.symbol(), expr(inner)),
        ExprKind::Binary { op, lhs, rhs } => {
            format!("({} {} {})", op.symbol(), expr(lhs), expr(rhs))
        }
        ExprKind::Call { callee, args } => {
            let args: String = args.iter().map(|a| format!(" {}", expr(a))).collect();
            format!("(call {}{args})", expr(callee))
        }
        ExprKind::Lambda { params, body } => {
            let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
            format!("(fn ({}) {})", names.join(" "), expr(body))
        }
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => match else_branch {
            Some(e2) => format!("(if {} {} {})", expr(cond), expr(then_branch), expr(e2)),
            None => format!("(if {} {})", expr(cond), expr(then_branch)),
        },
        ExprKind::Block(stmts) => {
            let body: String = stmts.iter().map(|s| format!(" {}", stmt(s))).collect();
            format!("(block{body})")
        }
        ExprKind::List { items, tail } => {
            let items: String = items.iter().map(|i| format!(" {}", expr(i))).collect();
            let tail = tail
                .as_ref()
                .map(|t| format!(" ..{}", expr(t)))
                .unwrap_or_default();
            format!("(list{items}{tail})")
        }
        ExprKind::Tuple(items) => {
            let items: String = items.iter().map(|i| format!(" {}", expr(i))).collect();
            format!("(tuple{items})")
        }
    }
}

fn stmt(s: &Stmt) -> String {
    match s {
        Stmt::Let {
            pattern: pat,
            value,
            ..
        } => format!("(let {} {})", pattern(pat), expr(value)),
        Stmt::Expr(e) => expr(e),
    }
}

pub fn type_expr(t: &TypeExpr) -> String {
    match t {
        TypeExpr::Named { name, args, .. } if args.is_empty() => name.clone(),
        TypeExpr::Named { name, args, .. } => format!("{name}({})", type_list(args)),
        TypeExpr::Var { name, .. } => name.clone(),
        TypeExpr::Fn { params, ret, .. } => {
            format!("fn({}) -> {}", type_list(params), type_expr(ret))
        }
        TypeExpr::Tuple { elems, .. } => format!("#({})", type_list(elems)),
    }
}

fn type_list(ts: &[TypeExpr]) -> String {
    ts.iter().map(type_expr).collect::<Vec<_>>().join(", ")
}

pub fn pattern(p: &Pattern) -> String {
    match &p.kind {
        PatternKind::Wildcard => "_".to_string(),
        PatternKind::Var(name) => name.clone(),
        PatternKind::Tuple(items) => format!("#({})", pattern_list(items)),
    }
}

fn pattern_list(ps: &[Pattern]) -> String {
    ps.iter().map(pattern).collect::<Vec<_>>().join(", ")
}

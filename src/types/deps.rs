use std::collections::HashSet;

use crate::ast::{Expr, ExprKind, Stmt};

/// Все имена переменных, упомянутые в выражении. Затенение не учитывается:
/// лишние рёбра только укрупняют компоненты и не ломают вывод.
pub fn referenced_names(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Var(name) => {
            out.insert(name.clone());
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::ModuleAccess { .. } => {}
        ExprKind::Unary { expr, .. } => referenced_names(expr, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            referenced_names(lhs, out);
            referenced_names(rhs, out);
        }
        ExprKind::Call { callee, args } => {
            referenced_names(callee, out);
            for a in args {
                referenced_names(a, out);
            }
        }
        ExprKind::Lambda { body, .. } => referenced_names(body, out),
        ExprKind::If { cond, then_branch, else_branch } => {
            referenced_names(cond, out);
            referenced_names(then_branch, out);
            if let Some(e) = else_branch {
                referenced_names(e, out);
            }
        }
        ExprKind::Block(stmts) => {
            for s in stmts {
                match s {
                    Stmt::Let { value, .. } => referenced_names(value, out),
                    Stmt::Expr(e) => referenced_names(e, out),
                }
            }
        }
    }
}

/// Компоненты сильной связности (Тарьян). `edges[i]` — от кого зависит `i`.
/// Результат: зависимости идут раньше зависящих.
pub fn scc(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = edges.len();
    let mut t = Tarjan {
        edges,
        index: vec![None; n],
        low: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        next: 0,
        out: Vec::new(),
    };
    for v in 0..n {
        if t.index[v].is_none() {
            t.visit(v);
        }
    }
    t.out
}

struct Tarjan<'a> {
    edges: &'a [Vec<usize>],
    index: Vec<Option<usize>>,
    low: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    next: usize,
    out: Vec<Vec<usize>>,
}

impl Tarjan<'_> {
    fn visit(&mut self, v: usize) {
        self.index[v] = Some(self.next);
        self.low[v] = self.next;
        self.next += 1;
        self.stack.push(v);
        self.on_stack[v] = true;
        let edges = self.edges;
        for &w in &edges[v] {
            match self.index[w] {
                None => {
                    self.visit(w);
                    self.low[v] = self.low[v].min(self.low[w]);
                }
                Some(iw) if self.on_stack[w] => self.low[v] = self.low[v].min(iw),
                Some(_) => {}
            }
        }
        if self.index[v] == Some(self.low[v]) {
            let mut comp = Vec::new();
            loop {
                let w = self.stack.pop().expect("v лежит в стеке");
                self.on_stack[w] = false;
                comp.push(w);
                if w == v {
                    break;
                }
            }
            comp.sort();
            self.out.push(comp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse_expr;

    #[test]
    fn collects_names() {
        let e = parse_expr(lex("{\n let x = f(y)\n g(x) + int.to_string(1)\n}").unwrap()).unwrap();
        let mut names = HashSet::new();
        referenced_names(&e, &mut names);
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort();
        assert_eq!(names, vec!["f", "g", "x", "y"]);
    }

    #[test]
    fn chain_is_ordered_dependencies_first() {
        // 0 зависит от 1, 1 от 2
        assert_eq!(scc(&[vec![1], vec![2], vec![]]), vec![vec![2], vec![1], vec![0]]);
    }

    #[test]
    fn mutual_recursion_is_one_component() {
        assert_eq!(scc(&[vec![1], vec![0], vec![]]), vec![vec![0, 1], vec![2]]);
        assert_eq!(scc(&[vec![0]]), vec![vec![0]]);
    }
}

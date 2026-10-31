use std::collections::HashSet;

use crate::ast::{Expr, ExprKind, Param, Stmt};

/// Свободные имена выражения: имена, не связанные параметрами лямбд и `let`.
/// Лишние рёбра зависимостей не безобидны: ложный цикл склеивает компоненты
/// и делает функции мономорфными, поэтому затенение учитывается.
pub fn referenced_names(e: &Expr, out: &mut HashSet<String>) {
    free_names(&[], e, out);
}

/// Свободные имена тела функции с учётом её параметров.
pub fn free_names(params: &[Param], body: &Expr, out: &mut HashSet<String>) {
    let mut bound: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
    walk(body, &mut bound, out);
}

fn walk<'a>(e: &'a Expr, bound: &mut Vec<&'a str>, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Var(name) => {
            if !bound.contains(&name.as_str()) {
                out.insert(name.clone());
            }
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::ModuleAccess { .. } => {}
        ExprKind::Unary { expr, .. } => walk(expr, bound, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            walk(lhs, bound, out);
            walk(rhs, bound, out);
        }
        ExprKind::Call { callee, args } => {
            walk(callee, bound, out);
            for a in args {
                walk(a, bound, out);
            }
        }
        ExprKind::Lambda { params, body } => {
            let base = bound.len();
            bound.extend(params.iter().map(|p| p.name.as_str()));
            walk(body, bound, out);
            bound.truncate(base);
        }
        ExprKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            walk(cond, bound, out);
            walk(then_branch, bound, out);
            if let Some(e) = else_branch {
                walk(e, bound, out);
            }
        }
        ExprKind::Block(stmts) => {
            let base = bound.len();
            for s in stmts {
                match s {
                    // имя `let` виден только следующим инструкциям, не собственному значению
                    Stmt::Let { name, value, .. } => {
                        walk(value, bound, out);
                        bound.push(name.as_str());
                    }
                    Stmt::Expr(e) => walk(e, bound, out),
                }
            }
            bound.truncate(base);
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
        assert_eq!(names, vec!["f", "g", "y"]);
    }

    fn free(src: &str) -> Vec<String> {
        let e = parse_expr(lex(src).unwrap()).unwrap();
        let mut names = HashSet::new();
        referenced_names(&e, &mut names);
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort();
        names
    }

    #[test]
    fn lambda_param_shadows_name() {
        assert_eq!(free("fn(f) { f(g) }"), vec!["g"]);
    }

    #[test]
    fn let_shadows_name_only_afterwards() {
        // `x` в собственном значении let свободно, дальше — связано
        assert_eq!(free("{\n let x = x\n x\n}"), vec!["x"]);
        assert_eq!(free("{\n let h = 1\n h\n}"), Vec::<String>::new());
        // после выхода из блока имя снова свободно
        assert_eq!(free("{\n {\n let h = 1\n h\n }\n h\n}"), vec!["h"]);
    }

    #[test]
    fn fn_params_are_bound() {
        let e = parse_expr(lex("f(x)").unwrap()).unwrap();
        let params = vec![Param {
            name: "f".into(),
            ty: None,
            span: Default::default(),
        }];
        let mut names = HashSet::new();
        free_names(&params, &e, &mut names);
        assert_eq!(names.into_iter().collect::<Vec<_>>(), vec!["x"]);
    }

    #[test]
    fn diamond_and_cross_edges() {
        assert_eq!(
            scc(&[vec![1, 2], vec![3], vec![3], vec![]]),
            vec![vec![3], vec![1], vec![2], vec![0]]
        );
        // перекрёстное ребро к уже завершённой компоненте
        assert_eq!(
            scc(&[vec![1, 2], vec![2], vec![]]),
            vec![vec![2], vec![1], vec![0]]
        );
    }

    #[test]
    fn chain_is_ordered_dependencies_first() {
        // 0 зависит от 1, 1 от 2
        assert_eq!(
            scc(&[vec![1], vec![2], vec![]]),
            vec![vec![2], vec![1], vec![0]]
        );
    }

    #[test]
    fn mutual_recursion_is_one_component() {
        assert_eq!(scc(&[vec![1], vec![0], vec![]]), vec![vec![0, 1], vec![2]]);
        assert_eq!(scc(&[vec![0]]), vec![vec![0]]);
    }
}

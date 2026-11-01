//! Проверка полноты `match` и `let`: алгоритм полезности образцов (Маранже)
//! с примером значения, которое не покрыто.

use crate::ast::{Pattern, PatternKind};
use crate::types::env::Env;

/// Упрощённый образец: конструктор с аргументами, литерал бесконечного типа или `_`.
#[derive(Debug, Clone, PartialEq)]
enum Pat {
    Wild,
    Ctor(Ctor, Vec<Pat>),
    Lit(String),
}

/// Конструктор вместе с семейством, к которому он относится.
#[derive(Debug, Clone, PartialEq)]
enum Ctor {
    Adt { type_name: String, name: String },
    True,
    False,
    Nil,
    Cons,
    Tuple(usize),
}

/// Пример значения, не подходящего ни под один образец, или `None`, если покрыто всё.
/// Образцы должны быть уже проверены по типам.
pub fn missing_case(patterns: &[&Pattern], env: &Env) -> Option<String> {
    let rows: Vec<Vec<Pat>> = patterns.iter().map(|p| vec![lower(p, env)]).collect();
    uncovered(&rows, 1, env).map(|w| show(&w[0]))
}

fn lower(p: &Pattern, env: &Env) -> Pat {
    match &p.kind {
        PatternKind::Wildcard | PatternKind::Var(_) => Pat::Wild,
        PatternKind::Int(n) => Pat::Lit(format!("i{n}")),
        PatternKind::Float(f) => Pat::Lit(format!("f{f:?}")),
        PatternKind::Str(s) => Pat::Lit(format!("s{s:?}")),
        PatternKind::Bool(true) => Pat::Ctor(Ctor::True, Vec::new()),
        PatternKind::Bool(false) => Pat::Ctor(Ctor::False, Vec::new()),
        PatternKind::Tuple(items) => Pat::Ctor(
            Ctor::Tuple(items.len()),
            items.iter().map(|i| lower(i, env)).collect(),
        ),
        PatternKind::Ctor { name, args } => {
            let type_name = env
                .ctors
                .get(name)
                .map(|c| c.type_name.clone())
                .unwrap_or_default();
            Pat::Ctor(
                Ctor::Adt {
                    type_name,
                    name: name.clone(),
                },
                args.iter().map(|a| lower(a, env)).collect(),
            )
        }
        PatternKind::List { items, tail } => {
            let mut rest = match tail {
                Some(t) => lower(t, env),
                None => Pat::Ctor(Ctor::Nil, Vec::new()),
            };
            for item in items.iter().rev() {
                rest = Pat::Ctor(Ctor::Cons, vec![lower(item, env), rest]);
            }
            rest
        }
    }
}

fn arity(c: &Ctor, env: &Env) -> usize {
    match c {
        Ctor::Adt { name, .. } => env.ctors.get(name).map_or(0, |i| i.arity),
        Ctor::True | Ctor::False | Ctor::Nil => 0,
        Ctor::Cons => 2,
        Ctor::Tuple(n) => *n,
    }
}

/// Все конструкторы семейства `c`.
fn siblings(c: &Ctor, env: &Env) -> Vec<Ctor> {
    match c {
        Ctor::Adt { type_name, .. } => match env.types.get(type_name) {
            Some(info) => info
                .ctors
                .iter()
                .map(|n| Ctor::Adt {
                    type_name: type_name.clone(),
                    name: n.clone(),
                })
                .collect(),
            None => vec![c.clone()],
        },
        Ctor::True | Ctor::False => vec![Ctor::True, Ctor::False],
        Ctor::Nil | Ctor::Cons => vec![Ctor::Nil, Ctor::Cons],
        Ctor::Tuple(n) => vec![Ctor::Tuple(*n)],
    }
}

/// Пример вектора значений ширины `width`, не покрытого строками `rows`.
fn uncovered(rows: &[Vec<Pat>], width: usize, env: &Env) -> Option<Vec<Pat>> {
    if width == 0 {
        return if rows.is_empty() {
            Some(Vec::new())
        } else {
            None
        };
    }
    let heads: Vec<&Ctor> = rows
        .iter()
        .filter_map(|r| match &r[0] {
            Pat::Ctor(c, _) => Some(c),
            _ => None,
        })
        .collect();
    let Some(first) = heads.first() else {
        // в первом столбце только `_` и литералы бесконечных типов
        return uncovered(&default_rows(rows), width - 1, env).map(|w| prepend(Pat::Wild, w));
    };
    let all = siblings(first, env);
    let missing: Vec<&Ctor> = all.iter().filter(|c| !heads.contains(c)).collect();
    if let Some(m) = missing.first() {
        let head = Pat::Ctor((*m).clone(), vec![Pat::Wild; arity(m, env)]);
        return uncovered(&default_rows(rows), width - 1, env).map(|w| prepend(head, w));
    }
    for c in &all {
        let k = arity(c, env);
        if let Some(mut w) = uncovered(&specialize(rows, c, k), k + width - 1, env) {
            let rest = w.split_off(k);
            return Some(prepend(Pat::Ctor(c.clone(), w), rest));
        }
    }
    None
}

fn prepend(head: Pat, rest: Vec<Pat>) -> Vec<Pat> {
    let mut out = vec![head];
    out.extend(rest);
    out
}

/// Строки, первый образец которых подходит под конструктор `c` (с раскрытыми аргументами).
fn specialize(rows: &[Vec<Pat>], c: &Ctor, k: usize) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter_map(|r| match &r[0] {
            Pat::Ctor(c2, args) if c2 == c => Some(args.iter().chain(&r[1..]).cloned().collect()),
            Pat::Ctor(..) | Pat::Lit(_) => None,
            Pat::Wild => Some(
                std::iter::repeat_n(Pat::Wild, k)
                    .chain(r[1..].iter().cloned())
                    .collect(),
            ),
        })
        .collect()
}

/// Строки с `_` в первом столбце, без этого столбца.
fn default_rows(rows: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter(|r| r[0] == Pat::Wild)
        .map(|r| r[1..].to_vec())
        .collect()
}

/// Образец-пример в синтаксисе Lumo.
fn show(p: &Pat) -> String {
    match p {
        Pat::Wild | Pat::Lit(_) => "_".to_string(),
        Pat::Ctor(Ctor::True, _) => "true".to_string(),
        Pat::Ctor(Ctor::False, _) => "false".to_string(),
        Pat::Ctor(Ctor::Tuple(_), args) => format!("#({})", show_list(args)),
        Pat::Ctor(Ctor::Adt { name, .. }, args) if args.is_empty() => name.clone(),
        Pat::Ctor(Ctor::Adt { name, .. }, args) => format!("{name}({})", show_list(args)),
        Pat::Ctor(Ctor::Nil | Ctor::Cons, _) => {
            let mut items = Vec::new();
            let mut cur = p;
            loop {
                match cur {
                    Pat::Ctor(Ctor::Cons, args) => {
                        items.push(show(&args[0]));
                        cur = &args[1];
                    }
                    Pat::Ctor(Ctor::Nil, _) => break,
                    _ => {
                        items.push("..".to_string());
                        break;
                    }
                }
            }
            format!("[{}]", items.join(", "))
        }
    }
}

fn show_list(ps: &[Pat]) -> String {
    ps.iter().map(show).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ExprKind;
    use crate::lexer::lex;
    use crate::parser::parse_expr;
    use crate::types::env::{CtorInfo, TypeInfo};
    use crate::types::ty::{Scheme, Type};

    fn env() -> Env {
        let mut env = Env::default();
        env.types.insert(
            "Option".to_string(),
            TypeInfo {
                arity: 1,
                ctors: vec!["Some".to_string(), "None".to_string()],
            },
        );
        for (name, arity) in [("Some", 1), ("None", 0)] {
            env.ctors.insert(
                name.to_string(),
                CtorInfo {
                    type_name: "Option".to_string(),
                    arity,
                    scheme: Scheme::mono(Type::unit()),
                },
            );
        }
        env
    }

    fn missing(arms: &str) -> Option<String> {
        let e = parse_expr(lex(&format!("match x {{\n{arms}\n}}")).unwrap()).unwrap();
        let ExprKind::Match { arms, .. } = e.kind else {
            panic!("ожидался match")
        };
        let pats: Vec<&Pattern> = arms.iter().map(|a| &a.pattern).collect();
        missing_case(&pats, &env())
    }

    #[test]
    fn algebraic_types() {
        assert_eq!(missing("Some(_) -> 1\nNone -> 2"), None);
        assert_eq!(missing("Some(_) -> 1"), Some("None".to_string()));
        assert_eq!(missing("None -> 1"), Some("Some(_)".to_string()));
        assert_eq!(
            missing("Some(Some(_)) -> 1\nNone -> 2"),
            Some("Some(None)".to_string())
        );
    }

    #[test]
    fn bools_and_tuples() {
        assert_eq!(missing("true -> 1"), Some("false".to_string()));
        assert_eq!(
            missing("#(true, _) -> 1\n#(_, true) -> 2"),
            Some("#(false, false)".to_string())
        );
    }

    #[test]
    fn lists() {
        assert_eq!(missing("[] -> 1\n[_] -> 2"), Some("[_, _, ..]".to_string()));
        assert_eq!(missing("[] -> 1\n[_, ..] -> 2"), None);
        assert_eq!(missing("[_, ..] -> 1"), Some("[]".to_string()));
    }

    #[test]
    fn literals_and_wildcards() {
        assert_eq!(missing("0 -> 1"), Some("_".to_string()));
        assert_eq!(missing("0 -> 1\n_ -> 2"), None);
        assert_eq!(missing("\"a\" -> 1"), Some("_".to_string()));
        assert_eq!(missing("x -> 1"), None);
    }
}

use std::collections::HashMap;

pub type TVar = u32;

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Con(String, Vec<Type>),
    Fn(Vec<Type>, Box<Type>),
    Var(TVar),
}

impl Type {
    pub fn con(name: &str) -> Type {
        Type::Con(name.to_string(), Vec::new())
    }

    pub fn int() -> Type {
        Type::con("Int")
    }

    pub fn float() -> Type {
        Type::con("Float")
    }

    pub fn bool() -> Type {
        Type::con("Bool")
    }

    pub fn string() -> Type {
        Type::con("String")
    }

    pub fn unit() -> Type {
        Type::con("Unit")
    }

    pub fn list(elem: Type) -> Type {
        Type::Con("List".to_string(), vec![elem])
    }

    /// Кортеж: `#(a, b)`.
    pub fn tuple(elems: Vec<Type>) -> Type {
        Type::Con("#".to_string(), elems)
    }
}

/// Полиморфный тип: `vars` квантифицированы.
#[derive(Debug, Clone, PartialEq)]
pub struct Scheme {
    pub vars: Vec<TVar>,
    pub ty: Type,
}

impl Scheme {
    pub fn mono(ty: Type) -> Scheme {
        Scheme {
            vars: Vec::new(),
            ty,
        }
    }
}

/// Печатает тип в синтаксисе Lumo. Переменные получают имена a, b, c… в порядке появления.
pub fn display(ty: &Type) -> String {
    let mut names = HashMap::new();
    let mut out = String::new();
    write_type(ty, &mut names, &mut out);
    out
}

fn write_type(ty: &Type, names: &mut HashMap<TVar, String>, out: &mut String) {
    match ty {
        Type::Var(v) => {
            let next = names.len();
            let name = names.entry(*v).or_insert_with(|| var_name(next));
            out.push_str(name);
        }
        Type::Con(name, args) => {
            out.push_str(name);
            if !args.is_empty() || name == "#" {
                out.push('(');
                write_list(args, names, out);
                out.push(')');
            }
        }
        Type::Fn(params, ret) => {
            out.push_str("fn(");
            write_list(params, names, out);
            out.push_str(") -> ");
            write_type(ret, names, out);
        }
    }
}

fn write_list(tys: &[Type], names: &mut HashMap<TVar, String>, out: &mut String) {
    for (i, t) in tys.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_type(t, names, out);
    }
}

fn var_name(i: usize) -> String {
    if i < 26 {
        ((b'a' + i as u8) as char).to_string()
    } else {
        format!("t{i}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_types_with_named_vars() {
        let t = Type::Fn(
            vec![Type::Var(7), Type::Con("List".into(), vec![Type::Var(3)])],
            Box::new(Type::Var(7)),
        );
        assert_eq!(display(&t), "fn(a, List(b)) -> a");
        assert_eq!(display(&Type::unit()), "Unit");
    }

    #[test]
    fn displays_tuples() {
        assert_eq!(
            display(&Type::tuple(vec![Type::int(), Type::string()])),
            "#(Int, String)"
        );
        assert_eq!(display(&Type::tuple(vec![])), "#()");
    }
}

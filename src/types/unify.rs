use std::collections::HashMap;

use super::ty::{Scheme, TVar, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnifyError {
    Mismatch,
    Occurs,
}

#[derive(Debug, Clone)]
enum VarState {
    Unbound { level: u32 },
    Bound(Type),
}

/// Хранилище переменных типов: каждая либо свободна (с уровнем для обобщения), либо связана.
#[derive(Debug, Default)]
pub struct Subst {
    vars: Vec<VarState>,
}

impl Subst {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fresh(&mut self, level: u32) -> Type {
        let id = self.vars.len() as TVar;
        self.vars.push(VarState::Unbound { level });
        Type::Var(id)
    }

    /// Раскрывает связанные переменные на верхнем уровне типа.
    pub fn shallow(&self, t: &Type) -> Type {
        let mut t = t.clone();
        while let Type::Var(v) = t {
            match &self.vars[v as usize] {
                VarState::Bound(next) => t = next.clone(),
                VarState::Unbound { .. } => break,
            }
        }
        t
    }

    /// Подставляет все связанные переменные.
    pub fn zonk(&self, t: &Type) -> Type {
        match self.shallow(t) {
            Type::Con(name, args) => Type::Con(name, args.iter().map(|a| self.zonk(a)).collect()),
            Type::Fn(params, ret) => Type::Fn(
                params.iter().map(|p| self.zonk(p)).collect(),
                Box::new(self.zonk(&ret)),
            ),
            var => var,
        }
    }

    pub fn unify(&mut self, a: &Type, b: &Type) -> Result<(), UnifyError> {
        let a = self.shallow(a);
        let b = self.shallow(b);
        match (&a, &b) {
            (Type::Var(x), Type::Var(y)) if x == y => Ok(()),
            (Type::Var(v), t) | (t, Type::Var(v)) => self.bind(*v, t),
            (Type::Con(n1, a1), Type::Con(n2, a2)) => {
                if n1 != n2 || a1.len() != a2.len() {
                    return Err(UnifyError::Mismatch);
                }
                for (x, y) in a1.iter().zip(a2) {
                    self.unify(x, y)?;
                }
                Ok(())
            }
            (Type::Fn(p1, r1), Type::Fn(p2, r2)) => {
                if p1.len() != p2.len() {
                    return Err(UnifyError::Mismatch);
                }
                for (x, y) in p1.iter().zip(p2) {
                    self.unify(x, y)?;
                }
                self.unify(r1, r2)
            }
            _ => Err(UnifyError::Mismatch),
        }
    }

    fn bind(&mut self, v: TVar, t: &Type) -> Result<(), UnifyError> {
        let level = match self.vars[v as usize] {
            VarState::Unbound { level } => level,
            VarState::Bound(_) => unreachable!("shallow вернул связанную переменную"),
        };
        self.occurs_and_adjust(v, level, t)?;
        self.vars[v as usize] = VarState::Bound(t.clone());
        Ok(())
    }

    /// Occurs check + понижение уровней переменных внутри `t` до `level`.
    fn occurs_and_adjust(&mut self, v: TVar, level: u32, t: &Type) -> Result<(), UnifyError> {
        match self.shallow(t) {
            Type::Var(w) => {
                if w == v {
                    return Err(UnifyError::Occurs);
                }
                if let VarState::Unbound { level: wl } = self.vars[w as usize] {
                    if wl > level {
                        self.vars[w as usize] = VarState::Unbound { level };
                    }
                }
                Ok(())
            }
            Type::Con(_, args) => {
                for a in &args {
                    self.occurs_and_adjust(v, level, a)?;
                }
                Ok(())
            }
            Type::Fn(params, ret) => {
                for p in &params {
                    self.occurs_and_adjust(v, level, p)?;
                }
                self.occurs_and_adjust(v, level, &ret)
            }
        }
    }

    /// Квантифицирует свободные переменные с уровнем больше `level`.
    pub fn generalize(&self, t: &Type, level: u32) -> Scheme {
        let ty = self.zonk(t);
        let mut vars = Vec::new();
        self.collect_vars(&ty, level, &mut vars);
        Scheme { vars, ty }
    }

    fn collect_vars(&self, t: &Type, level: u32, out: &mut Vec<TVar>) {
        match t {
            Type::Var(v) => {
                if let VarState::Unbound { level: l } = self.vars[*v as usize] {
                    if l > level && !out.contains(v) {
                        out.push(*v);
                    }
                }
            }
            Type::Con(_, args) => {
                for a in args {
                    self.collect_vars(a, level, out);
                }
            }
            Type::Fn(params, ret) => {
                for p in params {
                    self.collect_vars(p, level, out);
                }
                self.collect_vars(ret, level, out);
            }
        }
    }

    /// Заменяет квантифицированные переменные схемы свежими.
    pub fn instantiate(&mut self, s: &Scheme, level: u32) -> Type {
        let map: HashMap<TVar, Type> = s.vars.iter().map(|&v| (v, self.fresh(level))).collect();
        replace(&s.ty, &map)
    }
}

fn replace(t: &Type, map: &HashMap<TVar, Type>) -> Type {
    match t {
        Type::Var(v) => map.get(v).cloned().unwrap_or_else(|| t.clone()),
        Type::Con(name, args) => Type::Con(name.clone(), args.iter().map(|a| replace(a, map)).collect()),
        Type::Fn(params, ret) => Type::Fn(
            params.iter().map(|p| replace(p, map)).collect(),
            Box::new(replace(ret, map)),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ty::display;

    #[test]
    fn binds_variable() {
        let mut s = Subst::new();
        let a = s.fresh(0);
        s.unify(&a, &Type::int()).unwrap();
        assert_eq!(s.zonk(&a), Type::int());
    }

    #[test]
    fn unifies_functions() {
        let mut s = Subst::new();
        let a = s.fresh(0);
        let b = s.fresh(0);
        let f1 = Type::Fn(vec![a.clone()], Box::new(a));
        let f2 = Type::Fn(vec![Type::int()], Box::new(b.clone()));
        s.unify(&f1, &f2).unwrap();
        assert_eq!(s.zonk(&b), Type::int());
    }

    #[test]
    fn reports_mismatch_and_occurs() {
        let mut s = Subst::new();
        assert_eq!(s.unify(&Type::int(), &Type::bool()), Err(UnifyError::Mismatch));
        let a = s.fresh(0);
        let f = Type::Fn(vec![a.clone()], Box::new(Type::int()));
        assert_eq!(s.unify(&a, &f), Err(UnifyError::Occurs));
    }

    #[test]
    fn generalizes_only_deeper_levels() {
        let mut s = Subst::new();
        let inner = s.fresh(1);
        let outer = s.fresh(0);
        let scheme = s.generalize(&Type::Fn(vec![inner], Box::new(outer)), 0);
        assert_eq!(scheme.vars.len(), 1);
    }

    #[test]
    fn unification_lowers_levels() {
        let mut s = Subst::new();
        let outer = s.fresh(1);
        let inner = s.fresh(2);
        s.unify(&outer, &Type::Fn(vec![inner.clone()], Box::new(Type::int()))).unwrap();
        assert!(s.generalize(&inner, 1).vars.is_empty());
    }

    #[test]
    fn instantiation_gives_fresh_vars() {
        let mut s = Subst::new();
        let a = s.fresh(1);
        let scheme = s.generalize(&Type::Fn(vec![a.clone()], Box::new(a)), 0);
        let t1 = s.instantiate(&scheme, 1);
        let t2 = s.instantiate(&scheme, 1);
        assert_ne!(t1, t2);
        assert_eq!(display(&t1), "fn(a) -> a");
    }
}

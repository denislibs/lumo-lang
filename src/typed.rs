use crate::ast;
use crate::types::ty::Scheme;

/// Результат проверки типов — единственный вход бэкенда.
#[derive(Debug)]
pub struct Module {
    pub ast: ast::Module,
    /// Схемы функций верхнего уровня в порядке объявления.
    pub globals: Vec<(String, Scheme)>,
}

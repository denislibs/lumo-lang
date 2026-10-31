use crate::ast;
use crate::types::env::{CtorInfo, TypeInfo};
use crate::types::ty::Scheme;

/// Результат проверки типов — единственный вход бэкенда.
#[derive(Debug)]
pub struct Module {
    pub ast: ast::Module,
    /// Схемы функций и `@external` верхнего уровня в порядке объявления.
    pub globals: Vec<(String, Scheme)>,
    /// Типы модуля в порядке объявления.
    pub types: Vec<(String, TypeInfo)>,
    /// Конструкторы модуля в порядке объявления.
    pub ctors: Vec<(String, CtorInfo)>,
}

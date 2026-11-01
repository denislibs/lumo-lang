use std::collections::HashMap;

use super::ty::Scheme;

/// Окружение типов: значения (prelude и текущий модуль), модули stdlib,
/// пользовательские типы и конструкторы. Все схемы замкнуты.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub values: HashMap<String, Scheme>,
    pub modules: HashMap<String, HashMap<String, Scheme>>,
    pub types: HashMap<String, TypeInfo>,
    pub ctors: HashMap<String, CtorInfo>,
}

/// Пользовательский тип: число параметров и конструкторы в порядке объявления.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeInfo {
    pub arity: usize,
    pub ctors: Vec<String>,
}

/// Конструктор: тип-владелец, число полей и схема (`fn(поля) -> T` или `T`).
#[derive(Debug, Clone, PartialEq)]
pub struct CtorInfo {
    pub type_name: String,
    pub arity: usize,
    pub scheme: Scheme,
}

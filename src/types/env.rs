use std::collections::HashMap;

use super::ty::Scheme;

/// Окружение типов: значения (prelude и текущий модуль) и модули stdlib.
/// Все схемы замкнуты — не содержат свободных переменных.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub values: HashMap<String, Scheme>,
    pub modules: HashMap<String, HashMap<String, Scheme>>,
}

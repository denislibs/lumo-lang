use crate::ast;
use crate::codegen;
use crate::diagnostics::{self, Diag, Span};
use crate::lexer;
use crate::parser;
use crate::typed;
use crate::types::env::Env;
use crate::types::infer::check_module;

/// (имя модуля, файл, исходник). Порядок важен: модуль видит только предыдущие.
const STDLIB: &[(&str, &str, &str)] = &[
    ("prelude", "prelude.lumo", include_str!("../stdlib/prelude.lumo")),
    ("int", "int.lumo", include_str!("../stdlib/int.lumo")),
];

/// Ошибки компиляции вместе с исходником, чтобы их можно было напечатать.
#[derive(Debug)]
pub struct Failure {
    pub file: String,
    pub src: String,
    pub diags: Vec<Diag>,
}

impl Failure {
    pub fn render(&self, color: bool) -> String {
        diagnostics::render(&self.diags, &self.file, &self.src, color)
    }
}

pub struct Stdlib {
    pub env: Env,
    pub modules: Vec<(&'static str, typed::Module)>,
}

/// Проверяет stdlib и строит начальное окружение. Ошибка в stdlib — баг компилятора.
pub fn load_stdlib() -> Stdlib {
    let mut env = Env::default();
    let mut modules = Vec::new();
    for &(name, file, src) in STDLIB {
        let module = frontend(file, src, &env)
            .unwrap_or_else(|f| panic!("ошибка в stdlib:\n{}", f.render(false)));
        let target = if name == "prelude" {
            &mut env.values
        } else {
            env.modules.entry(name.to_string()).or_default()
        };
        for (n, s) in &module.globals {
            target.insert(n.clone(), s.clone());
        }
        modules.push((name, module));
    }
    Stdlib { env, modules }
}

/// Проверяет программу: синтаксис, типы, наличие `main`.
pub fn check(file: &str, src: &str) -> Result<typed::Module, Failure> {
    check_with(file, src, &load_stdlib())
}

/// Компилирует программу в самодостаточный JS.
pub fn compile(file: &str, src: &str) -> Result<String, Failure> {
    let stdlib = load_stdlib();
    let module = check_with(file, src, &stdlib)?;
    let mut units: Vec<(&str, &typed::Module)> = stdlib.modules.iter().map(|(p, m)| (*p, m)).collect();
    units.push(("", &module));
    Ok(codegen::emit_program(&units))
}

fn check_with(file: &str, src: &str, stdlib: &Stdlib) -> Result<typed::Module, Failure> {
    let module = frontend(file, src, &stdlib.env)?;
    validate_main(&module).map_err(|d| failure(file, src, vec![d]))?;
    Ok(module)
}

fn frontend(file: &str, src: &str, env: &Env) -> Result<typed::Module, Failure> {
    let fail = |diags| failure(file, src, diags);
    let tokens = lexer::lex(src).map_err(fail)?;
    let module = parser::parse(tokens).map_err(fail)?;
    check_module(module, env).map_err(fail)
}

fn failure(file: &str, src: &str, diags: Vec<Diag>) -> Failure {
    Failure { file: file.to_string(), src: src.to_string(), diags }
}

fn validate_main(module: &typed::Module) -> Result<(), Diag> {
    let main = module.ast.items.iter().find_map(|i| match i {
        ast::Item::Fn(f) if f.name == "main" => Some(f),
        _ => None,
    });
    match main {
        None => Err(Diag::error(Span::new(0, 0), "в программе нет функции main")),
        Some(f) if !f.params.is_empty() => Err(Diag::error(f.span, "main не должна принимать параметры")),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(src: &str) -> String {
        check("t.lumo", src).unwrap_err().render(false)
    }

    #[test]
    fn accepts_program_using_stdlib() {
        assert!(check("t.lumo", "fn main() {\n  print(int.to_string(1))\n}\n").is_ok());
    }

    #[test]
    fn reports_type_error_with_file_name() {
        let out = err("fn main() {\n  print(1)\n}\n");
        assert!(out.contains("ожидался String, а здесь Int"), "{out}");
        assert!(out.contains("t.lumo"), "{out}");
    }

    #[test]
    fn reports_lexer_errors() {
        assert!(err("fn main() {\n  \"abc\n}\n").contains("незакрытая строка"));
    }

    #[test]
    fn requires_main_without_params() {
        assert!(err("fn helper() { 1 }\n").contains("нет функции main"));
        assert!(err("fn main(x) { x }\n").contains("не должна принимать"));
    }
}

# Lumo M2 — данные и сопоставление с образцом: план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить в Lumo кортежи, списки, алгебраические типы (`type`), `match` и деструктурирующий `let` с проверкой полноты, `debug` и хвостовую рекурсию без роста стека.

**Architecture:** Расширяем существующий конвейер M1 вертикальными срезами: каждая задача проводит одну возможность через AST → парсер → вывод типов → codegen → golden-тест. Проверка полноты — отдельный модуль `src/exhaust.rs` (алгоритм Маранже с примером непокрытого значения), вызываемый из тайпчекера. Хвостовые самовызовы codegen превращает в цикл `while (true)`.

**Tech Stack:** Rust 2021 (stable), `ariadne 0.4`, Node.js ≥ 18.

**Спецификация:** `docs/superpowers/specs/2026-10-31-lumo-design.md` (разделы 1 «Pattern matching», 3 «Компиляция в JavaScript»). **Предыдущий план:** `docs/superpowers/plans/2026-10-31-lumo-m1-core-slice.md` (раздел «Заметки для M2»).

## Global Constraints

- Rust 2021, stable. Зависимости: только `ariadne = "0.4"`. Других крейтов не добавлять.
- Node.js ≥ 18 в `PATH` нужен для `lumo run` и golden-тестов.
- Все сообщения об ошибках компилятора — на русском; уже существующие тексты ошибок, на которые опираются тесты, не менять.
- Сгенерированный JS и `runtime/lumo.js` не используют API Node — только стандартный JS и `console`.
- Глобалы модуля в JS — `префикс$имя` (`main$` для программы, `prelude$`, `int$` для stdlib); служебные имена codegen начинаются с `$` (в именах Lumo `$` не встречается).
- Представление данных в JS: кортеж — массив `[a, b]`; список — `null` (пустой) или ячейка `{ h, t }` (создаётся `$cons(h, t)`); значение АТД — `{ $: "Имя", _0: …, _1: … }`, конструктор без полей — общий объект `{ $: "Имя" }`.
- Имена типов и конструкторов — с заглавной буквы, переменные и функции — со строчной.
- Каждая задача заканчивается зелёным `cargo test`, `cargo clippy --all-targets` без ошибок, `cargo fmt` и коммитом.

### Отклонения от спеки (вносятся в спеку в Task 6)

- Тег конструктора — строка с именем (`$: "Some"`), а не номер: `debug` печатает значения без таблиц имён, а сравнение тегов внутри одного типа однозначно.
- Пустой список — `null`, а не отдельный объект-синглтон.
- Предупреждение о недостижимых ветках `match` — в M3 (вместе с уровнями диагностик); в M2 только ошибка о неполноте.

## Структура файлов M2

| Файл | Изменение |
|---|---|
| `src/ast.rs` | `TypeExpr::Tuple`, `ExprKind::{Tuple, List, Ctor, Match}`, `Arm`, `Pattern`/`PatternKind`, `Stmt::Let { pattern, .. }`, `Item::Type(TypeDecl)`, `Variant` |
| `src/parser.rs` | кортежи, списки, конструкторы, `match`, образцы, `type` |
| `src/sexpr.rs` | печать новых узлов и образцов |
| `src/types/ty.rs` | `Type::tuple`, `Type::list`, печать `#()` |
| `src/types/env.rs` | `TypeInfo`, `CtorInfo`, поля `types`, `ctors` |
| `src/types/deps.rs` | связывание имён из образцов |
| `src/types/infer.rs` | вывод для новых выражений, `bind_pattern`, объявления типов, вызов проверки полноты |
| `src/exhaust.rs` (новый) | проверка полноты, пример непокрытого случая |
| `src/typed.rs` | экспорт `types`, `ctors` модуля |
| `src/driver.rs` | типы и конструкторы prelude попадают в окружение |
| `src/codegen.rs` | кортежи, списки, конструкторы, `match`, деструктуризация `let`, хвостовая рекурсия |
| `runtime/lumo.js` | `$cons`, `$inspect`, `$debug`, итеративный `$eq` |
| `stdlib/prelude.lumo` | `debug` |
| `tests/programs/*.lumo/.out` | `tuples`, `lists`, `adt`, `match`, `tco` |

---

### Task 1: Кортежи и деструктурирующий `let`

**Files:**
- Modify: `src/ast.rs`, `src/parser.rs`, `src/sexpr.rs`, `src/types/ty.rs`, `src/types/deps.rs`, `src/types/infer.rs`, `src/codegen.rs`
- Create: `tests/programs/tuples.lumo`, `tests/programs/tuples.out`

**Interfaces:**
- Consumes: всё из M1 (`Parser`, `Checker`, `Gen`, `Subst`, `Type`).
- Produces:
  - `ast::TypeExpr::Tuple { elems: Vec<TypeExpr>, span: Span }`, `ast::ExprKind::Tuple(Vec<Expr>)`.
  - `ast::Pattern { kind: PatternKind, span: Span }`, `ast::PatternKind::{Wildcard, Var(String), Tuple(Vec<Pattern>)}` (Task 4 добавит варианты), `Pattern::bindings(&self) -> Vec<(&str, Span)>`.
  - `ast::Stmt::Let { pattern: Pattern, ty: Option<TypeExpr>, value: Expr, span: Span }` (поле `name` заменено на `pattern`).
  - `Parser::pattern()`, `Parser::pattern_list(close: &Tok)`.
  - `sexpr::pattern(&Pattern) -> String`; кортеж печатается `(tuple a b)`, тип — `#(A, B)`, образец — `#(a, b)`.
  - `Type::tuple(Vec<Type>) -> Type` (= `Type::Con("#", elems)`), печать `#(Int, String)`, пустой — `#()`.
  - `Checker::bind_pattern(&mut self, p: &Pattern, expected: &Type, binds: &mut Vec<(String, Type)>)`.
  - Codegen: `Gen::temp() -> String` (`$t1`, `$t2`, …), `Gen::bind_let(&Pattern, String) -> String`, свободные `pattern_accesses(p, access) -> Vec<(String, String)>`.
- Тексты ошибок: `ожидался образец`, `имя `{x}` повторяется в образце`.

- [ ] **Step 1: AST.** В `src/ast.rs`:

В `enum TypeExpr` добавить вариант и ветку `span()`:
```rust
    /// `#(Int, String)`
    Tuple {
        elems: Vec<TypeExpr>,
        span: Span,
    },
```
```rust
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named { span, .. }
            | TypeExpr::Var { span, .. }
            | TypeExpr::Fn { span, .. }
            | TypeExpr::Tuple { span, .. } => *span,
        }
    }
```

В `enum ExprKind` добавить:
```rust
    /// `#(a, b)`
    Tuple(Vec<Expr>),
```

Заменить `Stmt::Let`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        pattern: Pattern,
        ty: Option<TypeExpr>,
        value: Expr,
        span: Span,
    },
    Expr(Expr),
}
```

Добавить в конец файла:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    /// `_`
    Wildcard,
    Var(String),
    /// `#(a, b)`
    Tuple(Vec<Pattern>),
}

impl Pattern {
    /// Имена, которые связывает образец, слева направо.
    pub fn bindings(&self) -> Vec<(&str, Span)> {
        let mut out = Vec::new();
        self.collect_bindings(&mut out);
        out
    }

    fn collect_bindings<'a>(&'a self, out: &mut Vec<(&'a str, Span)>) {
        match &self.kind {
            PatternKind::Wildcard => {}
            PatternKind::Var(name) => out.push((name, self.span)),
            PatternKind::Tuple(items) => {
                for p in items {
                    p.collect_bindings(out);
                }
            }
        }
    }
}
```

- [ ] **Step 2: Падающие тесты парсера** — в `mod tests` файла `src/parser.rs`:

```rust
    #[test]
    fn tuples_and_let_patterns() {
        assert_eq!(p("#(1, x)"), "(tuple 1 x)");
        assert_eq!(p("#()"), "(tuple)");
        assert_eq!(p("{\n let #(a, _) = t\n a\n}"), "(block (let #(a, _) t) a)");
        assert_eq!(
            p("{\n let #(a, #(b, c)) = t\n b\n}"),
            "(block (let #(a, #(b, c)) t) b)"
        );
        assert!(perr("{\n let + = x\n x\n}").contains("ожидался образец"));
    }

    #[test]
    fn tuple_types() {
        let m = module("fn f(p: #(Int, fn(a) -> b)) { p }");
        let Item::Fn(f) = &m.items[0] else {
            panic!("ожидалась функция")
        };
        assert_eq!(
            sexpr::type_expr(f.params[0].ty.as_ref().unwrap()),
            "#(Int, fn(a) -> b)"
        );
    }
```

- [ ] **Step 3: Запустить** — `cargo test parser`. Expected: FAIL (не компилируется: нет `ExprKind::Tuple`, `Stmt::Let` без `pattern` и т. д.). Компиляция будет падать и в других модулях до Step 7 — это ожидаемо.

- [ ] **Step 4: Парсер.** В `src/parser.rs`:

Импорт AST дополнить `Pattern, PatternKind`.

В `type_expr` добавить ветку перед `_ =>`:
```rust
            Tok::Hash => {
                self.expect(&Tok::LParen, "(")?;
                let mut elems = Vec::new();
                while !self.at(&Tok::RParen) {
                    elems.push(self.type_expr()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                let end = self.expect(&Tok::RParen, ")")?;
                Ok(TypeExpr::Tuple {
                    elems,
                    span: t.span.to(end),
                })
            }
```

В `stmt` заменить `let (name, _) = self.ident()?;` на `let pattern = self.pattern()?;` и в конструировании `Stmt::Let` — `name` на `pattern`.

В `primary` добавить ветку перед `_ =>`:
```rust
            Tok::Hash => return self.tuple(t.span),
```

Добавить методы в `impl Parser`:
```rust
    /// `#(a, b)` — после `#`.
    fn tuple(&mut self, start: Span) -> PResult<Expr> {
        self.expect(&Tok::LParen, "(")?;
        let mut items = Vec::new();
        while !self.at(&Tok::RParen) {
            items.push(self.expr()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        let end = self.expect(&Tok::RParen, ")")?;
        Ok(Expr {
            kind: ExprKind::Tuple(items),
            span: start.to(end),
        })
    }

    fn pattern(&mut self) -> PResult<Pattern> {
        let t = self.advance();
        let kind = match t.tok {
            Tok::Ident(name) if name == "_" => PatternKind::Wildcard,
            Tok::Ident(name) => PatternKind::Var(name),
            Tok::Hash => {
                self.expect(&Tok::LParen, "(")?;
                let items = self.pattern_list(&Tok::RParen)?;
                let end = self.expect(&Tok::RParen, ")")?;
                return Ok(Pattern {
                    kind: PatternKind::Tuple(items),
                    span: t.span.to(end),
                });
            }
            _ => return Err(Diag::error(t.span, "ожидался образец")),
        };
        Ok(Pattern { kind, span: t.span })
    }

    /// Образцы через запятую до `close` (сам `close` не съедается).
    fn pattern_list(&mut self, close: &Tok) -> PResult<Vec<Pattern>> {
        let mut items = Vec::new();
        while !self.at(close) {
            items.push(self.pattern()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        Ok(items)
    }
```

- [ ] **Step 5: Печать.** В `src/sexpr.rs`: импорт дополнить `Pattern, PatternKind`; в `expr` добавить ветку
```rust
        ExprKind::Tuple(items) => {
            let items: String = items.iter().map(|i| format!(" {}", expr(i))).collect();
            format!("(tuple{items})")
        }
```
в `stmt` ветку `Let` заменить на
```rust
        Stmt::Let { pattern: pat, value, .. } => format!("(let {} {})", pattern(pat), expr(value)),
```
в `type_expr` добавить
```rust
        TypeExpr::Tuple { elems, .. } => format!("#({})", type_list(elems)),
```
и добавить функцию
```rust
pub fn pattern(p: &Pattern) -> String {
    match &p.kind {
        PatternKind::Wildcard => "_".to_string(),
        PatternKind::Var(name) => name.clone(),
        PatternKind::Tuple(items) => format!("#({})", pattern_list(items)),
    }
}

fn pattern_list(ps: &[Pattern]) -> String {
    ps.iter().map(pattern).collect::<Vec<_>>().join(", ")
}
```

- [ ] **Step 6: Типы.** В `src/types/ty.rs` в `impl Type` добавить:
```rust
    /// Кортеж: `#(a, b)`.
    pub fn tuple(elems: Vec<Type>) -> Type {
        Type::Con("#".to_string(), elems)
    }
```
В `write_type` ветку `Type::Con` заменить, чтобы пустой кортеж печатался `#()`:
```rust
        Type::Con(name, args) => {
            out.push_str(name);
            if !args.is_empty() || name == "#" {
                out.push('(');
                write_list(args, names, out);
                out.push(')');
            }
        }
```
и добавить тест в `mod tests` файла `ty.rs`:
```rust
    #[test]
    fn displays_tuples() {
        assert_eq!(display(&Type::tuple(vec![Type::int(), Type::string()])), "#(Int, String)");
        assert_eq!(display(&Type::tuple(vec![])), "#()");
    }
```

- [ ] **Step 7: Зависимости.** В `src/types/deps.rs` в `walk` добавить ветку
```rust
        ExprKind::Tuple(items) => {
            for i in items {
                walk(i, bound, out);
            }
        }
```
и в ветке `Stmt::Let` связывать имена образца:
```rust
                    Stmt::Let { pattern, value, .. } => {
                        walk(value, bound, out);
                        bound.extend(pattern.bindings().into_iter().map(|(n, _)| n));
                    }
```
Тест в `mod tests` файла `deps.rs`:
```rust
    #[test]
    fn let_pattern_binds_names() {
        let e = parse_expr(lex("{\n let #(f, g) = #(h, 1)\n f(g)\n}").unwrap()).unwrap();
        let mut names = HashSet::new();
        referenced_names(&e, &mut names);
        let names: Vec<_> = names.into_iter().collect();
        assert_eq!(names, vec!["h".to_string()]);
    }
```

- [ ] **Step 8: Падающие тесты вывода типов** — в `mod tests` файла `src/types/infer.rs`:
```rust
    #[test]
    fn tuples_and_destructuring() {
        assert_eq!(t("#(1, \"s\")"), "#(Int, String)");
        assert_eq!(t("fn(p) {\n let #(a, b) = p\n a\n}"), "fn(#(a, b)) -> a");
        assert_eq!(
            t("{\n let #(f, n) = #(fn(x) { x }, 1)\n let s = f(\"s\")\n f(n)\n}"),
            "Int"
        );
        assert_eq!(t("fn(p: #(Int, Bool)) { p }"), "fn(#(Int, Bool)) -> #(Int, Bool)");
        assert!(err("{\n let #(a, b) = 1\n a\n}").contains("ожидался Int, а здесь #(a, b)"));
        assert!(err("{\n let #(a, a) = #(1, 2)\n a\n}").contains("имя `a` повторяется в образце"));
    }
```

- [ ] **Step 9: Вывод типов.** В `src/types/infer.rs`:

Импорт AST дополнить `Pattern, PatternKind`.

В `infer` добавить ветку:
```rust
            ExprKind::Tuple(items) => {
                let tys = items.iter().map(|i| self.infer(i)).collect();
                Type::tuple(tys)
            }
```

В `block` ветку `Stmt::Let` заменить:
```rust
                Stmt::Let {
                    pattern, ty, value, ..
                } => {
                    self.level += 1;
                    let actual = self.infer(value);
                    // с аннотацией имя получает объявленный тип, чтобы ошибка не тянула за собой новые
                    let bound = match ty {
                        Some(t) => {
                            let expected = self.convert(t, &mut HashMap::new());
                            self.expect(value.span, &actual, &expected);
                            expected
                        }
                        None => actual,
                    };
                    let mut binds = Vec::new();
                    self.bind_pattern(pattern, &bound, &mut binds);
                    self.level -= 1;
                    for (name, t) in binds {
                        let scheme = self.subst.generalize(&t, self.level);
                        self.locals.push((name, scheme));
                    }
                    Type::unit()
                }
```

В `convert` добавить ветку:
```rust
            TypeExpr::Tuple { elems, .. } => {
                let elems = elems.iter().map(|e| self.convert(e, vars)).collect();
                Type::tuple(elems)
            }
```

Добавить метод в `impl Checker`:
```rust
    /// Проверяет образец против типа значения и собирает связанные имена с их типами.
    fn bind_pattern(&mut self, p: &Pattern, expected: &Type, binds: &mut Vec<(String, Type)>) {
        match &p.kind {
            PatternKind::Wildcard => {}
            PatternKind::Var(name) => {
                if binds.iter().any(|(n, _)| n == name) {
                    self.diags.push(Diag::error(
                        p.span,
                        format!("имя `{name}` повторяется в образце"),
                    ));
                }
                binds.push((name.clone(), expected.clone()));
            }
            PatternKind::Tuple(items) => {
                let tys: Vec<Type> = items.iter().map(|_| self.subst.fresh(self.level)).collect();
                self.expect(p.span, &Type::tuple(tys.clone()), expected);
                for (item, t) in items.iter().zip(&tys) {
                    self.bind_pattern(item, t, binds);
                }
            }
        }
    }
```

- [ ] **Step 10: Codegen.** В `src/codegen.rs`:

Импорт AST дополнить `Pattern, PatternKind`. В `struct Gen` добавить поле `temps: u32` (в `Gen::new` — `temps: 0`).

В `expr` добавить ветку:
```rust
            ExprKind::Tuple(items) => {
                let items: Vec<String> = items.iter().map(|i| self.expr(i)).collect();
                format!("[{}]", items.join(", "))
            }
```

В `tail` ветку `Stmt::Let` заменить:
```rust
                        Stmt::Let { pattern, value, .. } => {
                            let v = self.expr(value);
                            out.push_str(&self.bind_let(pattern, v));
                        }
```

Добавить методы в `impl Gen`:
```rust
    /// Свежее служебное имя (`$` не встречается в именах Lumo).
    fn temp(&mut self) -> String {
        self.temps += 1;
        format!("$t{}", self.temps)
    }

    /// `const` для каждого имени из образца `let`; значение вычисляется один раз.
    fn bind_let(&mut self, pattern: &Pattern, value: String) -> String {
        if let PatternKind::Var(name) = &pattern.kind {
            let js = self.bind(name);
            return format!("const {js} = {value};\n");
        }
        let tmp = self.temp();
        let mut out = format!("const {tmp} = {value};\n");
        for (name, access) in pattern_accesses(pattern, &tmp) {
            let js = self.bind(&name);
            let _ = writeln!(out, "const {js} = {access};");
        }
        out
    }
```

Добавить свободные функции:
```rust
/// Пары (имя Lumo, JS-путь к значению) для имён, которые связывает образец.
fn pattern_accesses(p: &Pattern, access: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    collect_accesses(p, access.to_string(), &mut out);
    out
}

fn collect_accesses(p: &Pattern, access: String, out: &mut Vec<(String, String)>) {
    match &p.kind {
        PatternKind::Wildcard => {}
        PatternKind::Var(name) => out.push((name.clone(), access)),
        PatternKind::Tuple(items) => {
            for (i, item) in items.iter().enumerate() {
                collect_accesses(item, format!("{access}[{i}]"), out);
            }
        }
    }
}
```

- [ ] **Step 11: Golden-тест.** `tests/programs/tuples.lumo`:
```lumo
fn swap(p) {
  let #(a, b) = p
  #(b, a)
}

fn main() {
  let #(x, y) = swap(#(1, "один"))
  print(x)
  print(int.to_string(y))
  let #(_, #(n, m)) = #("вложенный", #(2, 3))
  print(int.to_string(n * m))
  let point: #(Int, Int) = #(4, 5)
  let #(px, py) = point
  print(int.to_string(px + py))
  print(if #(1, "a") == #(1, "a") { "равны" } else { "разные" })
}
```
`tests/programs/tuples.out` (с переводом строки в конце):
```
один
1
6
9
равны
```

- [ ] **Step 12: Запустить всё** — `cargo test`, `cargo clippy --all-targets`, `cargo fmt`. Expected: всё зелёное, clippy без ошибок.

- [ ] **Step 13: Commit**
```bash
git add src tests/programs/tuples.lumo tests/programs/tuples.out
git commit -m "feat: кортежи и деструктурирующий let"
```

---

### Task 2: Списки и `debug`

**Files:**
- Modify: `src/ast.rs`, `src/parser.rs`, `src/sexpr.rs`, `src/types/ty.rs`, `src/types/deps.rs`, `src/types/infer.rs`, `src/codegen.rs`, `runtime/lumo.js`, `stdlib/prelude.lumo`
- Create: `tests/programs/lists.lumo`, `tests/programs/lists.out`

**Interfaces:**
- Consumes: Task 1 (`Type::tuple`, `bind_pattern`, `Gen`).
- Produces:
  - `ast::ExprKind::List { items: Vec<Expr>, tail: Option<Box<Expr>> }` — `[a, b]`, `[a, ..rest]`; печать `(list a b)`, `(list a ..rest)`.
  - `Type::list(elem: Type) -> Type` (= `Type::Con("List", vec![elem])`).
  - Свободная `fn builtin_arity(name: &str) -> Option<usize>` в `infer.rs` (`Int Float Bool String Unit` → 0, `List` → 1) и `Checker::type_arity(&self, name) -> Option<usize>` (Task 3 добавит пользовательские типы).
  - Рантайм: `$cons(h, t)`, `$inspect(v) -> string`, `$debug(v)` (печатает и возвращает `v`); `$eq` сравнивает хвосты списков циклом.
  - prelude: `debug(x: a) -> a`.
- Тексты ошибок: `тип `{T}` принимает параметров: {n}, а передано: {m}` (для типов с параметрами; для типов без параметров остаётся `тип `{T}` не принимает параметров`).

- [ ] **Step 1: Падающие тесты.** В `src/parser.rs`:
```rust
    #[test]
    fn list_literals() {
        assert_eq!(p("[1, 2]"), "(list 1 2)");
        assert_eq!(p("[]"), "(list)");
        assert_eq!(p("[x, ..xs]"), "(list x ..xs)");
        assert_eq!(p("[\n 1,\n 2\n]"), "(list 1 2)");
    }
```
В `src/types/infer.rs`:
```rust
    #[test]
    fn lists() {
        assert_eq!(t("[1, 2]"), "List(Int)");
        assert_eq!(t("[]"), "List(a)");
        assert_eq!(t("fn(x, xs) { [x, ..xs] }"), "fn(a, List(a)) -> List(a)");
        assert_eq!(t("fn(xs: List(Int)) { xs }"), "fn(List(Int)) -> List(Int)");
        assert!(err("[1, \"s\"]").contains("ожидался Int, а здесь String"));
        assert!(err("[1, ..2]").contains("ожидался List(Int), а здесь Int"));
        assert!(err("fn(x: List) { x }").contains("принимает параметров: 1, а передано: 0"));
        assert!(err("fn(x: Int(Int)) { x }").contains("не принимает параметров"));
    }
```
Run: `cargo test` — Expected: FAIL (нет `ExprKind::List`).

- [ ] **Step 2: AST и парсер.** В `src/ast.rs` в `ExprKind`:
```rust
    /// `[a, b]` или `[a, ..rest]`
    List {
        items: Vec<Expr>,
        tail: Option<Box<Expr>>,
    },
```
В `src/parser.rs` в `primary` перед `_ =>`:
```rust
            Tok::LBracket => return self.list(t.span),
```
и метод:
```rust
    /// `[a, b]` или `[a, b, ..rest]` — после `[`.
    fn list(&mut self, start: Span) -> PResult<Expr> {
        let mut items = Vec::new();
        let mut tail = None;
        while !self.at(&Tok::RBracket) {
            if self.eat(&Tok::DotDot) {
                tail = Some(Box::new(self.expr()?));
                break;
            }
            items.push(self.expr()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        let end = self.expect(&Tok::RBracket, "]")?;
        Ok(Expr {
            kind: ExprKind::List { items, tail },
            span: start.to(end),
        })
    }
```

- [ ] **Step 3: Печать и зависимости.** В `src/sexpr.rs`:
```rust
        ExprKind::List { items, tail } => {
            let items: String = items.iter().map(|i| format!(" {}", expr(i))).collect();
            let tail = tail
                .as_ref()
                .map(|t| format!(" ..{}", expr(t)))
                .unwrap_or_default();
            format!("(list{items}{tail})")
        }
```
В `src/types/deps.rs` в `walk`:
```rust
        ExprKind::List { items, tail } => {
            for i in items {
                walk(i, bound, out);
            }
            if let Some(t) = tail {
                walk(t, bound, out);
            }
        }
```

- [ ] **Step 4: Типы.** В `src/types/ty.rs` в `impl Type`:
```rust
    pub fn list(elem: Type) -> Type {
        Type::Con("List".to_string(), vec![elem])
    }
```
В `src/types/infer.rs` в `infer`:
```rust
            ExprKind::List { items, tail } => {
                let elem = self.subst.fresh(self.level);
                for item in items {
                    let t = self.infer(item);
                    self.expect(item.span, &t, &elem);
                }
                let list = Type::list(elem);
                if let Some(tail) = tail {
                    let t = self.infer(tail);
                    self.expect(tail.span, &t, &list);
                }
                list
            }
```
Ветку `TypeExpr::Named` в `convert` заменить:
```rust
            TypeExpr::Named { name, args, span } => {
                let Some(arity) = self.type_arity(name) else {
                    self.diags
                        .push(Diag::error(*span, format!("неизвестный тип `{name}`")));
                    return self.subst.fresh(self.level);
                };
                if args.len() != arity {
                    let msg = if arity == 0 {
                        format!("тип `{name}` не принимает параметров")
                    } else {
                        format!(
                            "тип `{name}` принимает параметров: {arity}, а передано: {}",
                            args.len()
                        )
                    };
                    self.diags.push(Diag::error(*span, msg));
                    return self.subst.fresh(self.level);
                }
                let args = args.iter().map(|a| self.convert(a, vars)).collect();
                Type::Con(name.clone(), args)
            }
```
Добавить метод в `impl Checker`:
```rust
    /// Число параметров типа или `None`, если такого типа нет.
    fn type_arity(&self, name: &str) -> Option<usize> {
        builtin_arity(name)
    }
```
и свободную функцию:
```rust
/// Арность встроенного типа.
fn builtin_arity(name: &str) -> Option<usize> {
    match name {
        "Int" | "Float" | "Bool" | "String" | "Unit" => Some(0),
        "List" => Some(1),
        _ => None,
    }
}
```

- [ ] **Step 5: Codegen.** В `src/codegen.rs` в `expr`:
```rust
            ExprKind::List { items, tail } => {
                let mut out = match tail {
                    Some(t) => self.expr(t),
                    None => "null".to_string(),
                };
                for item in items.iter().rev() {
                    let v = self.expr(item);
                    out = format!("$cons({v}, {out})");
                }
                out
            }
```

- [ ] **Step 6: Рантайм.** В `runtime/lumo.js` заменить `$eq` и добавить функции:
```js
function $eq(a, b) {
  for (;;) {
    // функции сравниваются с ошибкой всегда, даже `f == f`
    if (typeof a === "function" || typeof b === "function") {
      throw new Error("Lumo: функции нельзя сравнивать через ==");
    }
    if (a === b) return true;
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
      return false;
    }
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) {
      if (k !== "t" && !$eq(a[k], b[k])) return false;
    }
    // ячейка списка: хвост сравниваем циклом, чтобы длинные списки не переполняли стек
    if (!("t" in a)) return true;
    a = a.t;
    b = b.t;
  }
}

function $cons(h, t) {
  return { h, t };
}

// Значение в синтаксисе Lumo. Целые и дробные числа неразличимы в рантайме: 2.0 печатается как 2.
function $inspect(v) {
  if (v === undefined) return "Unit";
  if (v === null) return "[]";
  if (typeof v === "string") return JSON.stringify(v);
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  if (typeof v === "function") return "<fn>";
  if (Array.isArray(v)) return "#(" + v.map($inspect).join(", ") + ")";
  if ("h" in v) {
    const items = [];
    for (let c = v; c !== null; c = c.t) items.push($inspect(c.h));
    return "[" + items.join(", ") + "]";
  }
  const args = [];
  for (let i = 0; "_" + i in v; i++) args.push($inspect(v["_" + i]));
  return args.length === 0 ? v.$ : v.$ + "(" + args.join(", ") + ")";
}

function $debug(v) {
  console.log($inspect(v));
  return v;
}
```
В `stdlib/prelude.lumo` добавить в конец:
```lumo

@external("$debug")
fn debug(x: a) -> a
```

- [ ] **Step 7: Golden-тест.** `tests/programs/lists.lumo`:
```lumo
fn main() {
  let xs = [1, 2, 3]
  debug(xs)
  debug([0, ..xs])
  debug([])
  debug(#([1], "два", true))
  print(if [1, 2] == [1, 2] { "равны" } else { "разные" })
  print(if [1, 2] == [1, 3] { "равны" } else { "разные" })
  debug([[1], [], [2, 3]])
  debug(debug(7) + 1)
}
```
`tests/programs/lists.out`:
```
[1, 2, 3]
[0, 1, 2, 3]
[]
#([1], "два", true)
равны
разные
[[1], [], [2, 3]]
7
8
```

- [ ] **Step 8: Запустить всё** — `cargo test`, `cargo clippy --all-targets`, `cargo fmt`. Expected: зелёное.

- [ ] **Step 9: Commit**
```bash
git add src runtime stdlib tests/programs/lists.lumo tests/programs/lists.out
git commit -m "feat: списки, debug и итеративное сравнение списков"
```

---

### Task 3: Алгебраические типы (`type`) и конструкторы

**Files:**
- Modify: `src/ast.rs`, `src/parser.rs`, `src/sexpr.rs`, `src/types/env.rs`, `src/types/infer.rs`, `src/typed.rs`, `src/driver.rs`, `src/codegen.rs`
- Create: `tests/programs/adt.lumo`, `tests/programs/adt.out`

**Interfaces:**
- Consumes: Task 2 (`type_arity`, `builtin_arity`, `$inspect`).
- Produces:
  - `ast::Item::Type(TypeDecl)`; `ast::TypeDecl { name: String, params: Vec<String>, variants: Vec<Variant>, span: Span }`; `ast::Variant { name: String, fields: Vec<TypeExpr>, span: Span }`; `Item::name()` для типа — имя типа.
  - `ast::ExprKind::Ctor(String)` — конструктор как значение (`Some`, `None`); вызов `Some(1)` — обычный `Call` с `Ctor` в роли функции.
  - `Parser::type_decl()`, `Parser::upper() -> PResult<(String, Span)>`.
  - `types::env::TypeInfo { arity: usize, ctors: Vec<String> }`, `types::env::CtorInfo { type_name: String, arity: usize, scheme: Scheme }` (Debug, Clone, PartialEq); `Env { values, modules, types: HashMap<String, TypeInfo>, ctors: HashMap<String, CtorInfo> }`. Схема конструктора: `fn(поля) -> T` или `T` без полей, замкнута.
  - `typed::Module { ast, globals, types: Vec<(String, TypeInfo)>, ctors: Vec<(String, CtorInfo)> }` — `globals` только функции и `@external`, в порядке объявления.
  - Типы и конструкторы модуля `prelude` попадают в окружение программы (`driver::load_stdlib`).
  - JS: конструктор с полями — `function p$Some(_0) { return { $: "Some", _0: _0 }; }`, без полей — `const p$None = { $: "None" };`.
- Тексты ошибок: `ожидалось объявление: fn, type или @external`, `ожидалось имя с заглавной буквы`, `у типа должен быть хотя бы один вариант`, `тип `{T}` уже встроен в язык`, `параметр типа `{a}` объявлен дважды`, `конструктор `{C}` уже объявлен в этом модуле`, `неизвестная переменная типа `{a}`: объяви её в параметрах типа`, `неизвестный конструктор `{C}``.

- [ ] **Step 1: Падающие тесты парсера** — в `src/parser.rs`:
```rust
    #[test]
    fn type_declarations() {
        let m = module("type Option(a) {\n  Some(a)\n  None\n}\n\ntype Shape { Circle(Float); Rect(Float, Float) }");
        let Item::Type(t) = &m.items[0] else {
            panic!("ожидался тип")
        };
        assert_eq!(t.name, "Option");
        assert_eq!(t.params, vec!["a".to_string()]);
        let variants: Vec<(&str, usize)> = t
            .variants
            .iter()
            .map(|v| (v.name.as_str(), v.fields.len()))
            .collect();
        assert_eq!(variants, vec![("Some", 1), ("None", 0)]);
        let Item::Type(s) = &m.items[1] else {
            panic!("ожидался тип")
        };
        assert_eq!(s.variants.len(), 2);
        assert!(module_errs("type Empty {}")[0]
            .message
            .contains("хотя бы один вариант"));
        assert_eq!(p("Some(1)"), "(call Some 1)");
        assert_eq!(p("None"), "None");
    }
```
Тест `recovery_always_makes_progress` заменить (теперь `type` разбирается, и ошибка — отсутствие `{`):
```rust
    #[test]
    fn recovery_always_makes_progress() {
        let errs = module_errs("fn a() { 1 }\ntype T\nfn b() { 2 }");
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].message.contains("ожидалось `{`"), "{errs:?}");
        let errs = module_errs("fn a() { 1 }\nlet x\nfn b() { 2 }");
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(errs[0].message.contains("ожидалось объявление"));
    }
```

- [ ] **Step 2: Падающие тесты типов** — в `mod tests` файла `src/types/infer.rs` (используются существующие `check_src`, `sig`):
```rust
    const OPTION: &str = "type Option(a) {\n  Some(a)\n  None\n}\n";

    #[test]
    fn algebraic_types() {
        let m = check_src(
            &format!("{OPTION}fn f() {{ Some(1) }}\nfn g() {{ None }}\nfn h(x: Option(Int)) {{ x }}"),
            &Env::default(),
        )
        .unwrap();
        assert_eq!(sig(&m, "f"), "fn() -> Option(Int)");
        assert_eq!(sig(&m, "g"), "fn() -> Option(a)");
        assert_eq!(sig(&m, "h"), "fn(Option(Int)) -> Option(Int)");
        assert_eq!(m.types.len(), 1);
        assert_eq!(m.ctors.len(), 2);
        let names: Vec<&str> = m.globals.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["f", "g", "h"]);
        let m = check_src(
            "type Tree(a) {\n  Leaf\n  Node(Tree(a), a, Tree(a))\n}\nfn t() { Node(Leaf, 1, Leaf) }",
            &Env::default(),
        )
        .unwrap();
        assert_eq!(sig(&m, "t"), "fn() -> Tree(Int)");
    }

    #[test]
    fn algebraic_type_errors() {
        let e = |src: &str| check_src(src, &Env::default()).unwrap_err()[0].message.clone();
        assert!(e("type Box {\n  Box(a)\n}").contains("неизвестная переменная типа `a`"));
        assert!(e("type Box(a) {\n  Box(a)\n}\nfn f(x: Box) { x }")
            .contains("принимает параметров: 1, а передано: 0"));
        assert!(e("type A {\n  X\n}\ntype B {\n  X\n}").contains("конструктор `X` уже объявлен"));
        assert!(e("type Int {\n  I\n}").contains("уже встроен"));
        assert!(e("type P(a, a) {\n  P(a)\n}").contains("параметр типа `a` объявлен дважды"));
        assert!(e("fn f() { Nope }").contains("неизвестный конструктор `Nope`"));
        assert!(e(&format!("{OPTION}fn f() {{ Some(1) == Some(\"s\") }}"))
            .contains("несовпадение типов"));
    }
```
Run: `cargo test` — Expected: FAIL (не компилируется).

- [ ] **Step 3: AST.** В `src/ast.rs`:
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Fn(FnDecl),
    External(ExternalDecl),
    Type(TypeDecl),
}

impl Item {
    pub fn name(&self) -> &str {
        match self {
            Item::Fn(f) => &f.name,
            Item::External(e) => &e.name,
            Item::Type(t) => &t.name,
        }
    }
}

/// `type Name(a, b) { Variant(T1, T2) ... }`
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub name: String,
    pub params: Vec<String>,
    pub variants: Vec<Variant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<TypeExpr>,
    pub span: Span,
}
```
В `ExprKind`:
```rust
    /// Конструктор как значение: `Some`, `None`; `Some(1)` — вызов.
    Ctor(String),
```

- [ ] **Step 4: Парсер.** В `src/parser.rs`: импорт дополнить `TypeDecl, Variant`. `item` заменить:
```rust
    fn item(&mut self) -> PResult<Item> {
        match self.peek() {
            Tok::Fn => Ok(Item::Fn(self.fn_decl()?)),
            Tok::At => Ok(Item::External(self.external()?)),
            Tok::Type => Ok(Item::Type(self.type_decl()?)),
            _ => Err(Diag::error(
                self.span(),
                "ожидалось объявление: fn, type или @external",
            )),
        }
    }
```
Добавить методы:
```rust
    fn type_decl(&mut self) -> PResult<TypeDecl> {
        let start = self.expect(&Tok::Type, "type")?;
        let (name, _) = self.upper()?;
        let mut params = Vec::new();
        if self.eat(&Tok::LParen) {
            while !self.at(&Tok::RParen) {
                params.push(self.ident()?.0);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(&Tok::RParen, ")")?;
        }
        self.expect(&Tok::LBrace, "{")?;
        let mut variants = Vec::new();
        loop {
            while self.eat(&Tok::Newline) || self.eat(&Tok::Semi) {}
            if self.at(&Tok::RBrace) {
                break;
            }
            let (vname, vspan) = self.upper()?;
            let mut fields = Vec::new();
            let mut end = vspan;
            if self.eat(&Tok::LParen) {
                while !self.at(&Tok::RParen) {
                    fields.push(self.type_expr()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                end = self.expect(&Tok::RParen, ")")?;
            }
            variants.push(Variant {
                name: vname,
                fields,
                span: vspan.to(end),
            });
            if !matches!(self.peek(), Tok::Newline | Tok::Semi | Tok::RBrace) {
                return Err(Diag::error(self.span(), "ожидался перевод строки или `}`"));
            }
        }
        let end = self.expect(&Tok::RBrace, "}")?;
        if variants.is_empty() {
            return Err(Diag::error(
                start.to(end),
                "у типа должен быть хотя бы один вариант",
            ));
        }
        Ok(TypeDecl {
            name,
            params,
            variants,
            span: start.to(end),
        })
    }

    fn upper(&mut self) -> PResult<(String, Span)> {
        match self.peek().clone() {
            Tok::Upper(name) => {
                let span = self.advance().span;
                Ok((name, span))
            }
            _ => Err(Diag::error(self.span(), "ожидалось имя с заглавной буквы")),
        }
    }
```
В `primary` перед `_ =>`:
```rust
            Tok::Upper(name) => ExprKind::Ctor(name),
```

- [ ] **Step 5: Печать и зависимости.** `src/sexpr.rs`, в `expr`: `ExprKind::Ctor(name) => name.clone(),`. `src/types/deps.rs`, в `walk`: добавить `ExprKind::Ctor(_)` к ветке без вложенных выражений (`Int | Float | Str | Bool | ModuleAccess`) — конструкторы не создают зависимостей между функциями.

- [ ] **Step 6: Окружение и typed.** `src/types/env.rs` заменить целиком:
```rust
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
```
`src/typed.rs` заменить целиком:
```rust
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
```

- [ ] **Step 7: Вывод типов.** В `src/types/infer.rs`:

Импорты: `use super::env::{CtorInfo, Env, TypeInfo};`.

`type_arity` заменить:
```rust
    /// Число параметров типа или `None`, если такого типа нет.
    fn type_arity(&self, name: &str) -> Option<usize> {
        builtin_arity(name).or_else(|| self.env.types.get(name).map(|t| t.arity))
    }
```

В `infer` добавить ветку:
```rust
            ExprKind::Ctor(name) => match self.env.ctors.get(name).cloned() {
                Some(info) => self.subst.instantiate(&info.scheme, self.level),
                None => {
                    self.diags
                        .push(Diag::error(e.span, format!("неизвестный конструктор `{name}`")));
                    self.subst.fresh(self.level)
                }
            },
```

Добавить методы в `impl Checker`:
```rust
    /// Регистрирует типы модуля (сначала имена — ради рекурсивных типов), затем конструкторы.
    fn declare_types(
        &mut self,
        decls: &[&ast::TypeDecl],
    ) -> (Vec<(String, TypeInfo)>, Vec<(String, CtorInfo)>) {
        let mut types = Vec::new();
        for d in decls {
            if builtin_arity(&d.name).is_some() {
                self.diags.push(Diag::error(
                    d.span,
                    format!("тип `{}` уже встроен в язык", d.name),
                ));
                continue;
            }
            let info = TypeInfo {
                arity: d.params.len(),
                ctors: d.variants.iter().map(|v| v.name.clone()).collect(),
            };
            self.env.types.insert(d.name.clone(), info.clone());
            types.push((d.name.clone(), info));
        }
        let mut ctors = Vec::new();
        let mut seen = HashSet::new();
        for d in decls {
            if builtin_arity(&d.name).is_some() {
                continue;
            }
            self.level = 1;
            let mut params: HashMap<String, Type> = HashMap::new();
            for p in &d.params {
                if params.contains_key(p) {
                    self.diags.push(Diag::error(
                        d.span,
                        format!("параметр типа `{p}` объявлен дважды"),
                    ));
                }
                let v = self.subst.fresh(1);
                params.insert(p.clone(), v);
            }
            let result = Type::Con(
                d.name.clone(),
                d.params.iter().map(|p| params[p].clone()).collect(),
            );
            for v in &d.variants {
                if !seen.insert(v.name.clone()) {
                    self.diags.push(Diag::error(
                        v.span,
                        format!("конструктор `{}` уже объявлен в этом модуле", v.name),
                    ));
                }
                let fields: Vec<Type> = v
                    .fields
                    .iter()
                    .map(|f| self.convert_closed(f, &params))
                    .collect();
                let ty = if fields.is_empty() {
                    result.clone()
                } else {
                    Type::Fn(fields.clone(), Box::new(result.clone()))
                };
                let info = CtorInfo {
                    type_name: d.name.clone(),
                    arity: fields.len(),
                    scheme: self.subst.generalize(&ty, 0),
                };
                self.env.ctors.insert(v.name.clone(), info.clone());
                ctors.push((v.name.clone(), info));
            }
            self.level = 0;
        }
        (types, ctors)
    }

    /// Тип поля конструктора: переменные типа — только параметры объявления.
    fn convert_closed(&mut self, t: &TypeExpr, params: &HashMap<String, Type>) -> Type {
        let mut vars = params.clone();
        let ty = self.convert(t, &mut vars);
        let mut unknown: Vec<&String> = vars.keys().filter(|n| !params.contains_key(*n)).collect();
        unknown.sort();
        for name in unknown {
            self.diags.push(Diag::error(
                t.span(),
                format!("неизвестная переменная типа `{name}`: объяви её в параметрах типа"),
            ));
        }
        ty
    }
```

В `check_module`:
- в цикле дубликатов добавить ветку `Item::Type(t) => t.span,`;
- сразу после цикла дубликатов (до `@external`) объявить типы:
```rust
    let type_decls: Vec<&ast::TypeDecl> = module
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Type(t) => Some(t),
            _ => None,
        })
        .collect();
    let (types, ctors) = c.declare_types(&type_decls);
```
- в `filter_map` для `fns` заменить `Item::External(_) => None` на `_ => None`;
- `globals` собирать без типов:
```rust
    let globals = module
        .items
        .iter()
        .filter(|i| !matches!(i, Item::Type(_)))
        .map(|i| (i.name().to_string(), c.env.values[i.name()].clone()))
        .collect();
```
- в `c.finish(typed::Module { ast: module, globals })` добавить `types, ctors`.

- [ ] **Step 8: Драйвер.** В `src/driver.rs` в `load_stdlib` перед блоком `let target = ...` добавить:
```rust
        // типы и конструкторы prelude видны программе без префикса
        if name == "prelude" {
            env.types.extend(module.types.iter().cloned());
            env.ctors.extend(module.ctors.iter().cloned());
        }
```

- [ ] **Step 9: Codegen.** В `src/codegen.rs`: импорт AST дополнить `TypeDecl`. В `struct Gen` добавить поле `ctors: HashSet<&'a str>`, в `Gen::new`:
```rust
        let ctors = module.ctors.iter().map(|(n, _)| n.as_str()).collect();
```
(и `ctors,` в конструкторе структуры). В `emit_program` в `match item` добавить `Item::Type(t) => gen.type_decl(t),`. В `expr`:
```rust
            ExprKind::Ctor(name) => {
                if self.ctors.contains(name.as_str()) {
                    mangle(self.prefix, name)
                } else {
                    mangle("prelude", name)
                }
            }
```
Метод:
```rust
    /// Конструкторы типа: с полями — функция, без полей — общий объект.
    fn type_decl(&mut self, t: &TypeDecl) {
        for v in &t.variants {
            let name = mangle(self.prefix, &v.name);
            let tag = js_string(&v.name);
            if v.fields.is_empty() {
                let _ = writeln!(self.out, "const {name} = {{ $: {tag} }};");
            } else {
                let params: Vec<String> = (0..v.fields.len()).map(|i| format!("_{i}")).collect();
                let fields: Vec<String> = params.iter().map(|p| format!("{p}: {p}")).collect();
                let _ = writeln!(
                    self.out,
                    "function {name}({}) {{ return {{ $: {tag}, {} }}; }}",
                    params.join(", "),
                    fields.join(", ")
                );
            }
        }
    }
```

- [ ] **Step 10: Golden-тест.** `tests/programs/adt.lumo`:
```lumo
type Shape {
  Circle(Float)
  Rect(Float, Float)
}

type Pair(a, b) {
  Pair(a, b)
}

type Color {
  Red
  Green
}

fn main() {
  debug(Circle(1.5))
  debug(Rect(2.5, 3.5))
  debug(Pair("ключ", [1, 2]))
  debug(Red)
  let make = Pair
  debug(make(1, Green))
  print(if Pair(1, Red) == Pair(1, Red) { "равны" } else { "разные" })
  print(if Red == Green { "равны" } else { "разные" })
}
```
`tests/programs/adt.out`:
```
Circle(1.5)
Rect(2.5, 3.5)
Pair("ключ", [1, 2])
Red
Pair(1, Green)
равны
разные
```

- [ ] **Step 11: Запустить всё** — `cargo test`, `cargo clippy --all-targets`, `cargo fmt`. Expected: зелёное.

- [ ] **Step 12: Commit**
```bash
git add src tests/programs/adt.lumo tests/programs/adt.out
git commit -m "feat: алгебраические типы и конструкторы"
```

---

### Task 4: `match` и образцы

**Files:**
- Modify: `src/ast.rs`, `src/parser.rs`, `src/sexpr.rs`, `src/types/deps.rs`, `src/types/infer.rs`, `src/codegen.rs`
- Create: `tests/programs/match.lumo`, `tests/programs/match.out`

**Interfaces:**
- Consumes: Tasks 1–3 (`Pattern`, `bind_pattern`, `pattern_accesses`, `bind_let`, `temp`, `CtorInfo`, `$cons`).
- Produces:
  - `ast::PatternKind` получает `Int(i64)`, `Float(f64)`, `Str(String)`, `Bool(bool)`, `Ctor { name: String, args: Vec<Pattern> }`, `List { items: Vec<Pattern>, tail: Option<Box<Pattern>> }`. `[a, ..]` разбирается как `tail = Some(Wildcard)`.
  - `ast::ExprKind::Match { scrutinee: Box<Expr>, arms: Vec<Arm> }`, `ast::Arm { pattern: Pattern, body: Expr, span: Span }`. Печать: `(match s (pat body) ...)`; образцы — в синтаксисе Lumo (`Some(#(a, _))`, `[1, ..rest]`, `-3`).
  - Codegen: свободная `pattern_conds(p, access, &mut Vec<String>)`, `Gen::match_tail(scrutinee, arms) -> String`; `bind_let` проверяет опровержимые образцы (`throw`, если не подошло).
- Тексты ошибок: `у match должна быть хотя бы одна ветка`, `после `-` в образце ожидалось целое число`, `конструктор `{C}` принимает аргументов: {n}, а передано: {m}`; во время выполнения — `Lumo: match не покрыл значение`, `Lumo: образец let не подошёл`.

- [ ] **Step 1: Падающие тесты.** `src/parser.rs`:
```rust
    #[test]
    fn match_and_patterns() {
        assert_eq!(
            p("match x {\n Some(#(a, _)) -> a\n [1, ..rest] -> 2\n -3 -> 4\n \"s\" -> 5\n [] -> 6\n [a, ..] -> 7\n None -> 8\n}"),
            "(match x (Some(#(a, _)) a) ([1, ..rest] 2) (-3 4) (\"s\" 5) ([] 6) ([a, .._] 7) (None 8))"
        );
        assert_eq!(p("match b {\n true -> 1\n false -> 2\n}"), "(match b (true 1) (false 2))");
        assert!(perr("match x {}").contains("хотя бы одна ветка"));
        assert!(perr("match x {\n - a -> 1\n}").contains("после `-`"));
    }
```
`src/types/infer.rs` (константа `OPTION` из Task 3):
```rust
    #[test]
    fn match_typing() {
        let m = check_src(
            &format!("{OPTION}fn unwrap(o, d) {{\n  match o {{\n    Some(x) -> x\n    None -> d\n  }}\n}}\nfn head(xs, d) {{\n  match xs {{\n    [x, ..] -> x\n    [] -> d\n  }}\n}}"),
            &Env::default(),
        )
        .unwrap();
        assert_eq!(sig(&m, "unwrap"), "fn(Option(a), a) -> a");
        assert_eq!(sig(&m, "head"), "fn(List(a), a) -> a");
        assert_eq!(t("match 1 {\n 0 -> \"ноль\"\n _ -> \"другое\"\n}"), "String");
    }

    #[test]
    fn match_errors() {
        let e = |src: &str| check_src(src, &Env::default()).unwrap_err()[0].message.clone();
        assert!(e(&format!("{OPTION}fn f(o) {{\n  match o {{\n    Some(a, b) -> a\n    _ -> 1\n  }}\n}}"))
            .contains("принимает аргументов: 1, а передано: 2"));
        assert!(e(&format!("{OPTION}fn f(o) {{\n  match o {{\n    Some(x) -> x\n    None -> x\n  }}\n}}"))
            .contains("неизвестное имя `x`"));
        assert!(e("fn f() {\n  match 1 {\n    Nope -> 1\n    _ -> 2\n  }\n}")
            .contains("неизвестный конструктор `Nope`"));
        assert!(err("match 1 {\n 1 -> \"a\"\n _ -> 2\n}").contains("ожидался String, а здесь Int"));
        assert!(err("match 1 {\n \"s\" -> 1\n _ -> 2\n}").contains("ожидался Int, а здесь String"));
    }
```
Run: `cargo test` — Expected: FAIL (не компилируется).

- [ ] **Step 2: AST.** В `src/ast.rs` в `PatternKind` добавить варианты:
```rust
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    /// `Some(x)`, `None`
    Ctor {
        name: String,
        args: Vec<Pattern>,
    },
    /// `[a, b]`, `[a, ..rest]`, `[a, ..]`
    List {
        items: Vec<Pattern>,
        tail: Option<Box<Pattern>>,
    },
```
`collect_bindings` заменить:
```rust
    fn collect_bindings<'a>(&'a self, out: &mut Vec<(&'a str, Span)>) {
        match &self.kind {
            PatternKind::Wildcard
            | PatternKind::Int(_)
            | PatternKind::Float(_)
            | PatternKind::Str(_)
            | PatternKind::Bool(_) => {}
            PatternKind::Var(name) => out.push((name, self.span)),
            PatternKind::Tuple(items) | PatternKind::Ctor { args: items, .. } => {
                for p in items {
                    p.collect_bindings(out);
                }
            }
            PatternKind::List { items, tail } => {
                for p in items {
                    p.collect_bindings(out);
                }
                if let Some(t) = tail {
                    t.collect_bindings(out);
                }
            }
        }
    }
```
В `ExprKind`:
```rust
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<Arm>,
    },
```
и структура:
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Arm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}
```

- [ ] **Step 3: Парсер.** Импорт AST дополнить `Arm`. В `pattern` добавить ветки перед `_ =>`:
```rust
            Tok::Int(n) => PatternKind::Int(n),
            Tok::Float(f) => PatternKind::Float(f),
            Tok::Str(s) => PatternKind::Str(s),
            Tok::True => PatternKind::Bool(true),
            Tok::False => PatternKind::Bool(false),
            Tok::Minus => {
                let n = self.advance();
                let Tok::Int(v) = n.tok else {
                    return Err(Diag::error(n.span, "после `-` в образце ожидалось целое число"));
                };
                return Ok(Pattern {
                    kind: PatternKind::Int(-v),
                    span: t.span.to(n.span),
                });
            }
            Tok::Upper(name) => {
                let mut args = Vec::new();
                let mut end = t.span;
                if self.eat(&Tok::LParen) {
                    args = self.pattern_list(&Tok::RParen)?;
                    end = self.expect(&Tok::RParen, ")")?;
                }
                return Ok(Pattern {
                    kind: PatternKind::Ctor { name, args },
                    span: t.span.to(end),
                });
            }
            Tok::LBracket => {
                let mut items = Vec::new();
                let mut tail = None;
                while !self.at(&Tok::RBracket) {
                    if self.at(&Tok::DotDot) {
                        let dots = self.advance().span;
                        let rest = if self.at(&Tok::RBracket) {
                            Pattern {
                                kind: PatternKind::Wildcard,
                                span: dots,
                            }
                        } else {
                            self.pattern()?
                        };
                        tail = Some(Box::new(rest));
                        break;
                    }
                    items.push(self.pattern()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                let end = self.expect(&Tok::RBracket, "]")?;
                return Ok(Pattern {
                    kind: PatternKind::List { items, tail },
                    span: t.span.to(end),
                });
            }
```
В `primary` перед `_ =>`: `Tok::Match => return self.match_expr(t.span),`. Метод:
```rust
    fn match_expr(&mut self, start: Span) -> PResult<Expr> {
        let scrutinee = self.expr()?;
        self.expect(&Tok::LBrace, "{")?;
        let mut arms = Vec::new();
        loop {
            while self.eat(&Tok::Newline) || self.eat(&Tok::Semi) {}
            if self.at(&Tok::RBrace) {
                break;
            }
            let pattern = self.pattern()?;
            self.expect(&Tok::Arrow, "->")?;
            let body = self.expr()?;
            let span = pattern.span.to(body.span);
            arms.push(Arm {
                pattern,
                body,
                span,
            });
            if !matches!(self.peek(), Tok::Newline | Tok::Semi | Tok::RBrace) {
                return Err(Diag::error(self.span(), "ожидался перевод строки или `}`"));
            }
        }
        let end = self.expect(&Tok::RBrace, "}")?;
        if arms.is_empty() {
            return Err(Diag::error(
                start.to(end),
                "у match должна быть хотя бы одна ветка",
            ));
        }
        Ok(Expr {
            kind: ExprKind::Match {
                scrutinee: Box::new(scrutinee),
                arms,
            },
            span: start.to(end),
        })
    }
```

- [ ] **Step 4: Печать и зависимости.** `src/sexpr.rs` — в `expr`:
```rust
        ExprKind::Match { scrutinee, arms } => {
            let arms: String = arms
                .iter()
                .map(|a| format!(" ({} {})", pattern(&a.pattern), expr(&a.body)))
                .collect();
            format!("(match {}{arms})", expr(scrutinee))
        }
```
`pattern` заменить:
```rust
pub fn pattern(p: &Pattern) -> String {
    match &p.kind {
        PatternKind::Wildcard => "_".to_string(),
        PatternKind::Var(name) => name.clone(),
        PatternKind::Int(n) => n.to_string(),
        PatternKind::Float(f) => format!("{f:?}"),
        PatternKind::Str(s) => format!("{s:?}"),
        PatternKind::Bool(b) => b.to_string(),
        PatternKind::Tuple(items) => format!("#({})", pattern_list(items)),
        PatternKind::Ctor { name, args } if args.is_empty() => name.clone(),
        PatternKind::Ctor { name, args } => format!("{name}({})", pattern_list(args)),
        PatternKind::List { items, tail } => {
            let mut parts: Vec<String> = items.iter().map(pattern).collect();
            if let Some(t) = tail {
                parts.push(format!("..{}", pattern(t)));
            }
            format!("[{}]", parts.join(", "))
        }
    }
}
```
`src/types/deps.rs` — в `walk`:
```rust
        ExprKind::Match { scrutinee, arms } => {
            walk(scrutinee, bound, out);
            for arm in arms {
                let base = bound.len();
                bound.extend(arm.pattern.bindings().into_iter().map(|(n, _)| n));
                walk(&arm.body, bound, out);
                bound.truncate(base);
            }
        }
```
и тест:
```rust
    #[test]
    fn match_arm_binds_names() {
        let e = parse_expr(lex("match o {\n Some(f) -> f(g)\n None -> h\n}").unwrap()).unwrap();
        let mut names = HashSet::new();
        referenced_names(&e, &mut names);
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort();
        assert_eq!(names, vec!["g", "h", "o"]);
    }
```

- [ ] **Step 5: Вывод типов.** В `bind_pattern` добавить ветки:
```rust
            PatternKind::Int(_) => {
                self.expect(p.span, &Type::int(), expected);
            }
            PatternKind::Float(_) => {
                self.expect(p.span, &Type::float(), expected);
            }
            PatternKind::Str(_) => {
                self.expect(p.span, &Type::string(), expected);
            }
            PatternKind::Bool(_) => {
                self.expect(p.span, &Type::bool(), expected);
            }
            PatternKind::List { items, tail } => {
                let elem = self.subst.fresh(self.level);
                let list = Type::list(elem.clone());
                self.expect(p.span, &list, expected);
                for item in items {
                    self.bind_pattern(item, &elem, binds);
                }
                if let Some(tail) = tail {
                    self.bind_pattern(tail, &list, binds);
                }
            }
            PatternKind::Ctor { name, args } => {
                let info = self.env.ctors.get(name).cloned();
                let fields = match info {
                    None => {
                        self.diags.push(Diag::error(
                            p.span,
                            format!("неизвестный конструктор `{name}`"),
                        ));
                        None
                    }
                    Some(info) if info.arity != args.len() => {
                        self.diags.push(Diag::error(
                            p.span,
                            format!(
                                "конструктор `{name}` принимает аргументов: {}, а передано: {}",
                                info.arity,
                                args.len()
                            ),
                        ));
                        None
                    }
                    Some(info) => {
                        let (fields, result) = match self.subst.instantiate(&info.scheme, self.level) {
                            Type::Fn(fields, result) => (fields, *result),
                            other => (Vec::new(), other),
                        };
                        self.expect(p.span, &result, expected);
                        Some(fields)
                    }
                };
                // при ошибке имена всё равно связываем, чтобы не плодить «неизвестное имя»
                let fields = fields
                    .unwrap_or_else(|| args.iter().map(|_| self.subst.fresh(self.level)).collect());
                for (a, t) in args.iter().zip(&fields) {
                    self.bind_pattern(a, t, binds);
                }
            }
```
В `infer` ветка:
```rust
            ExprKind::Match { scrutinee, arms } => {
                let s = self.infer(scrutinee);
                let result = self.subst.fresh(self.level);
                for arm in arms {
                    let base = self.locals.len();
                    let mut binds = Vec::new();
                    self.bind_pattern(&arm.pattern, &s, &mut binds);
                    for (name, t) in binds {
                        self.locals.push((name, Scheme::mono(t)));
                    }
                    let t = self.infer(&arm.body);
                    self.expect(arm.body.span, &t, &result);
                    self.locals.truncate(base);
                }
                result
            }
```

- [ ] **Step 6: Codegen.** Импорт AST дополнить `Arm`. Ветки `PatternKind` в `collect_accesses` заменить полным набором:
```rust
fn collect_accesses(p: &Pattern, access: String, out: &mut Vec<(String, String)>) {
    match &p.kind {
        PatternKind::Wildcard
        | PatternKind::Int(_)
        | PatternKind::Float(_)
        | PatternKind::Str(_)
        | PatternKind::Bool(_) => {}
        PatternKind::Var(name) => out.push((name.clone(), access)),
        PatternKind::Tuple(items) => {
            for (i, item) in items.iter().enumerate() {
                collect_accesses(item, format!("{access}[{i}]"), out);
            }
        }
        PatternKind::Ctor { args, .. } => {
            for (i, a) in args.iter().enumerate() {
                collect_accesses(a, format!("{access}._{i}"), out);
            }
        }
        PatternKind::List { items, tail } => {
            let mut cell = access;
            for item in items {
                collect_accesses(item, format!("{cell}.h"), out);
                cell = format!("{cell}.t");
            }
            if let Some(t) = tail {
                collect_accesses(t, cell, out);
            }
        }
    }
}

/// Условия JS, при которых значение по пути `access` подходит под образец.
fn pattern_conds(p: &Pattern, access: &str, out: &mut Vec<String>) {
    match &p.kind {
        PatternKind::Wildcard | PatternKind::Var(_) => {}
        PatternKind::Int(n) => out.push(format!("{access} === {n}")),
        PatternKind::Float(f) => out.push(format!("{access} === {f:?}")),
        PatternKind::Str(s) => out.push(format!("{access} === {}", js_string(s))),
        PatternKind::Bool(b) => out.push(format!("{access} === {b}")),
        PatternKind::Tuple(items) => {
            for (i, item) in items.iter().enumerate() {
                pattern_conds(item, &format!("{access}[{i}]"), out);
            }
        }
        PatternKind::Ctor { name, args } => {
            out.push(format!("{access}.$ === {}", js_string(name)));
            for (i, a) in args.iter().enumerate() {
                pattern_conds(a, &format!("{access}._{i}"), out);
            }
        }
        PatternKind::List { items, tail } => {
            let mut cell = access.to_string();
            for item in items {
                out.push(format!("{cell} !== null"));
                pattern_conds(item, &format!("{cell}.h"), out);
                cell = format!("{cell}.t");
            }
            match tail {
                None => out.push(format!("{cell} === null")),
                Some(t) => pattern_conds(t, &cell, out),
            }
        }
    }
}
```
`bind_let` — после `let mut out = format!("const {tmp} = {value};\n");` добавить проверку опровержимого образца (после Task 5 такие `let` отвергаются при компиляции, проверка остаётся страховкой):
```rust
        let mut conds = Vec::new();
        pattern_conds(pattern, &tmp, &mut conds);
        if !conds.is_empty() {
            let _ = writeln!(
                out,
                "if (!({})) throw new Error(\"Lumo: образец let не подошёл\");",
                conds.join(" && ")
            );
        }
```
В `tail` добавить ветку перед `_ =>`:
```rust
            ExprKind::Match { scrutinee, arms } => self.match_tail(scrutinee, arms),
```
В `expr`:
```rust
            ExprKind::Match { .. } => format!("(() => {{\n{}}})()", self.tail(e)),
```
Метод:
```rust
    /// Цепочка `if` по веткам `match`; каждая ветка заканчивается `return`.
    fn match_tail(&mut self, scrutinee: &Expr, arms: &[Arm]) -> String {
        let tmp = self.temp();
        let value = self.expr(scrutinee);
        let mut out = format!("const {tmp} = {value};\n");
        for arm in arms {
            let mut conds = Vec::new();
            pattern_conds(&arm.pattern, &tmp, &mut conds);
            self.scopes.push(HashMap::new());
            let mut body = String::new();
            for (name, access) in pattern_accesses(&arm.pattern, &tmp) {
                let js = self.bind(&name);
                let _ = writeln!(body, "const {js} = {access};");
            }
            body.push_str(&self.tail(&arm.body));
            self.scopes.pop();
            if conds.is_empty() {
                // ветка подходит всегда: остальные недостижимы
                let _ = writeln!(out, "{{\n{body}}}");
                return out;
            }
            let _ = writeln!(out, "if ({}) {{\n{body}}}", conds.join(" && "));
        }
        out.push_str("throw new Error(\"Lumo: match не покрыл значение\");\n");
        out
    }
```

- [ ] **Step 7: Golden-тест.** `tests/programs/match.lumo`:
```lumo
type Option(a) {
  Some(a)
  None
}

fn find(xs, pred) {
  match xs {
    [] -> None
    [x, ..rest] -> if pred(x) { Some(x) } else { find(rest, pred) }
  }
}

fn describe(n) {
  match n {
    0 -> "ноль"
    -1 -> "минус один"
    _ -> "другое"
  }
}

fn sum(xs) {
  match xs {
    [] -> 0
    [x, ..rest] -> x + sum(rest)
  }
}

fn shape(xs) {
  match xs {
    [] -> "пусто"
    [_] -> "один"
    [a, b] -> "два: " <> a <> b
    [a, ..] -> "много, первый " <> a
  }
}

fn label(o) {
  match o {
    Some(#(name, true)) -> name <> " (вкл)"
    Some(#(name, false)) -> name <> " (выкл)"
    None -> "нет"
  }
}

fn main() {
  debug(find([1, 4, 9, 16], fn(n) { n > 5 }))
  debug(find([1, 2], fn(n) { n > 5 }))
  print(describe(0))
  print(describe(-1))
  print(describe(7))
  print(int.to_string(sum([1, 2, 3, 4])))
  print(shape([]))
  print(shape(["a"]))
  print(shape(["a", "b"]))
  print(shape(["a", "b", "c"]))
  print(label(Some(#("свет", true))))
  print(label(Some(#("звук", false))))
  print(label(None))
  let message = match Some(3) {
    Some(n) -> "есть " <> int.to_string(n)
    None -> "пусто"
  }
  print(message)
}
```
`tests/programs/match.out`:
```
Some(9)
None
ноль
минус один
другое
10
пусто
один
два: ab
много, первый a
свет (вкл)
звук (выкл)
нет
есть 3
```

- [ ] **Step 8: Запустить всё** — `cargo test`, `cargo clippy --all-targets`, `cargo fmt`. Expected: зелёное.

- [ ] **Step 9: Commit**
```bash
git add src tests/programs/match.lumo tests/programs/match.out
git commit -m "feat: match и образцы"
```

---

### Task 5: Проверка полноты `match` и `let`

**Files:**
- Create: `src/exhaust.rs`
- Modify: `src/lib.rs`, `src/types/infer.rs`, `tests/cli.rs`

**Interfaces:**
- Consumes: `ast::{Pattern, PatternKind}` (Task 4), `Env { types, ctors }` (Task 3).
- Produces: `exhaust::missing_case(patterns: &[&Pattern], env: &Env) -> Option<String>` — пример непокрытого значения в синтаксисе Lumo (`None`, `[_, _, ..]`, `#(false, false)`, `_`) или `None`, если покрыто всё. Предполагает, что образцы уже прошли проверку типов.
- Тексты ошибок: `неполный match: не обработан случай `{case}``; `образец в let должен подходить под любое значение; не подходит `{case}`` с подсказкой `для разбора вариантов используй match`.

- [ ] **Step 1: Падающие тесты модуля** — создать `src/exhaust.rs`, пока только с этим модулем тестов (реализация — в Step 2):
```rust
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
        assert_eq!(missing("Some(Some(_)) -> 1\nNone -> 2"), Some("Some(None)".to_string()));
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
```
В `src/lib.rs` добавить `pub mod exhaust;` (после `driver`). Run: `cargo test exhaust` — Expected: FAIL (`cannot find function missing_case`).

- [ ] **Step 2: Реализация** — начало `src/exhaust.rs`:
```rust
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
        return if rows.is_empty() { Some(Vec::new()) } else { None };
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
            Pat::Ctor(c2, args) if c2 == c => {
                Some(args.iter().chain(&r[1..]).cloned().collect())
            }
            Pat::Ctor(..) | Pat::Lit(_) => None,
            Pat::Wild => Some(
                std::iter::repeat(Pat::Wild)
                    .take(k)
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
```
Run: `cargo test exhaust` — Expected: PASS (4 теста).

- [ ] **Step 3: Падающие тесты тайпчекера** — в `mod tests` файла `src/types/infer.rs`:
```rust
    #[test]
    fn exhaustiveness_errors() {
        let e = |src: &str| check_src(src, &Env::default()).unwrap_err()[0].clone();
        let d = e(&format!("{OPTION}fn f(o) {{\n  match o {{\n    Some(x) -> x\n  }}\n}}"));
        assert!(d.message.contains("неполный match: не обработан случай `None`"), "{d:?}");
        let d = e("fn f(b) {\n  match b {\n    true -> 1\n  }\n}");
        assert!(d.message.contains("`false`"));
        let d = e(&format!("{OPTION}fn f(o) {{\n  let Some(x) = o\n  x\n}}"));
        assert!(d.message.contains("образец в let должен подходить под любое значение"));
        assert_eq!(d.hint.as_deref(), Some("для разбора вариантов используй match"));
        assert!(check_src("fn f(p) {\n  let #(a, _) = p\n  a\n}", &Env::default()).is_ok());
    }
```
Run: `cargo test infer` — Expected: FAIL (ошибок о неполноте нет).

- [ ] **Step 4: Вызов из тайпчекера.** В `src/types/infer.rs` добавить импорт `use crate::exhaust::missing_case;`.

Ветку `ExprKind::Match` в `infer` заменить:
```rust
            ExprKind::Match { scrutinee, arms } => {
                let s = self.infer(scrutinee);
                let result = self.subst.fresh(self.level);
                let mut pattern_errors = false;
                for arm in arms {
                    let base = self.locals.len();
                    let mut binds = Vec::new();
                    let before = self.diags.len();
                    self.bind_pattern(&arm.pattern, &s, &mut binds);
                    pattern_errors |= self.diags.len() > before;
                    for (name, t) in binds {
                        self.locals.push((name, Scheme::mono(t)));
                    }
                    let t = self.infer(&arm.body);
                    self.expect(arm.body.span, &t, &result);
                    self.locals.truncate(base);
                }
                // полноту проверяем только для корректно типизированных образцов
                if !pattern_errors {
                    let pats: Vec<&Pattern> = arms.iter().map(|a| &a.pattern).collect();
                    if let Some(case) = missing_case(&pats, &self.env) {
                        self.diags.push(Diag::error(
                            e.span,
                            format!("неполный match: не обработан случай `{case}`"),
                        ));
                    }
                }
                result
            }
```
В `block`, ветка `Stmt::Let`: заменить строку `self.bind_pattern(pattern, &bound, &mut binds);` на
```rust
                    let before = self.diags.len();
                    self.bind_pattern(pattern, &bound, &mut binds);
                    if self.diags.len() == before {
                        if let Some(case) = missing_case(&[pattern], &self.env) {
                            self.diags.push(
                                Diag::error(
                                    pattern.span,
                                    format!(
                                        "образец в let должен подходить под любое значение; не подходит `{case}`"
                                    ),
                                )
                                .with_hint("для разбора вариантов используй match"),
                            );
                        }
                    }
```

- [ ] **Step 5: Тест CLI.** В `tests/cli.rs`:
```rust
#[test]
fn check_reports_non_exhaustive_match() {
    let file = write_tmp(
        "partial.lumo",
        "fn main() {\n  let n = 2\n  match n > 1 {\n    true -> print(\"да\")\n  }\n}\n",
    );
    let out = lumo()
        .args(["check", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("не обработан случай `false`"));
}
```

- [ ] **Step 6: Запустить всё** — `cargo test`, `cargo clippy --all-targets`, `cargo fmt`. Expected: зелёное (golden-программы из Task 4 полные).

- [ ] **Step 7: Commit**
```bash
git add src tests/cli.rs
git commit -m "feat: проверка полноты match и let"
```

---

### Task 6: Хвостовая рекурсия

**Files:**
- Modify: `src/codegen.rs`, `docs/superpowers/specs/2026-10-31-lumo-design.md`
- Create: `tests/programs/tco.lumo`, `tests/programs/tco.out`

**Interfaces:**
- Consumes: `Gen` (Tasks 1–4: `tail`, `expr`, `match_tail`, `var`, `mangle`).
- Produces: самовызов функции верхнего уровня в хвостовой позиции (последнее выражение блока, ветки `if`, ветки `match`) компилируется в цикл:
  ```js
  function main$count($p0, $p1) {
  while (true) {
  const n = $p0;
  const acc = $p1;
  ...
  $p0 = (n - 1);
  $p1 = (acc + 1);
  continue;
  ...
  }
  }
  ```
  Самовызов распознаётся, только если имя разрешается в глобал этой функции (локальная переменная может его затенить) и число аргументов совпадает. Внутри лямбд и выражений, ставших IIFE (блок или `match` в позиции выражения), хвостовые вызовы не переписываются. Параметры копируются в `const` на каждой итерации, поэтому замыкания захватывают значения своей итерации.
- `struct SelfCall { name: String, js: String, params: Vec<String>, used: bool }`; поле `Gen::self_call: Option<SelfCall>`; методы `Gen::is_self_call(&self, callee: &Expr, argc: usize) -> bool`, `Gen::nested<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T`.

- [ ] **Step 1: Падающие тесты.** Golden-тест `tests/programs/tco.lumo`:
```lumo
fn count(n, acc) {
  if n == 0 { acc } else { count(n - 1, acc + 1) }
}

fn build(n, acc) {
  if n == 0 { acc } else { build(n - 1, [n, ..acc]) }
}

fn length(xs, acc) {
  match xs {
    [] -> acc
    [_, ..rest] -> length(rest, acc + 1)
  }
}

fn closures(n, acc) {
  if n == 0 { acc } else { closures(n - 1, [fn() { n }, ..acc]) }
}

fn call_all(fs, acc) {
  match fs {
    [] -> acc
    [f, ..rest] -> call_all(rest, [f(), ..acc])
  }
}

fn shadowed(n) {
  let shadowed = fn(x) { x * 2 }
  shadowed(n)
}

fn main() {
  print(int.to_string(count(1000000, 0)))
  let big = build(200000, [])
  print(int.to_string(length(big, 0)))
  print(if big == build(200000, []) { "длинные списки равны" } else { "разные" })
  debug(call_all(closures(3, []), []))
  print(int.to_string(shadowed(21)))
}
```
`tests/programs/tco.out`:
```
1000000
200000
длинные списки равны
[3, 2, 1]
42
```
Модульные тесты в конце `src/codegen.rs`:
```rust
#[cfg(test)]
mod tests {
    fn js(src: &str) -> String {
        crate::driver::compile("t.lumo", src).unwrap()
    }

    #[test]
    fn tail_self_call_becomes_loop() {
        let out = js("fn loop_to(n) {\n  if n == 0 { 0 } else { loop_to(n - 1) }\n}\nfn main() { loop_to(3) }");
        assert!(out.contains("function main$loop_to($p0) {\nwhile (true) {"), "{out}");
        assert!(out.contains("continue;"), "{out}");
    }

    #[test]
    fn non_tail_or_nested_calls_stay_calls() {
        let out = js("fn f(n) {\n  if n == 0 { 0 } else { 1 + f(n - 1) }\n}\nfn main() { f(3) }");
        assert!(!out.contains("while (true)"), "{out}");
        let out = js("fn g(n) {\n  let h = fn() { g(n) }\n  h()\n}\nfn main() { 1 }");
        assert!(!out.contains("while (true)"), "{out}");
    }
}
```
Run: `cargo test` — Expected: FAIL: `tail_self_call_becomes_loop` не находит цикл; golden `tco` падает с `RangeError: Maximum call stack size exceeded` (и `[0, 0, 0]` вместо `[3, 2, 1]` невозможно проверить до этого).

- [ ] **Step 2: Реализация.** В `src/codegen.rs`:

Добавить структуру:
```rust
/// Функция, в которой хвостовой самовызов превращается в переход на новую итерацию цикла.
struct SelfCall {
    /// Имя функции в Lumo.
    name: String,
    /// Её глобальное JS-имя.
    js: String,
    /// Переменные цикла `$p0`, `$p1`, … — по одной на параметр.
    params: Vec<String>,
    /// Был ли переписан хотя бы один вызов.
    used: bool,
}
```
В `struct Gen` добавить поле `self_call: Option<SelfCall>` (в `Gen::new` — `self_call: None`).

`function` заменить:
```rust
    fn function(&mut self, f: &FnDecl) {
        self.used = self.globals.iter().map(|g| (g.to_string(), 1)).collect();
        self.scopes.push(HashMap::new());
        let params: Vec<String> = f.params.iter().map(|p| self.bind(&p.name)).collect();
        let loop_vars: Vec<String> = (0..params.len()).map(|i| format!("$p{i}")).collect();
        let name = mangle(self.prefix, &f.name);
        self.self_call = Some(SelfCall {
            name: f.name.clone(),
            js: name.clone(),
            params: loop_vars.clone(),
            used: false,
        });
        let body = self.tail(&f.body);
        let looped = self.self_call.take().is_some_and(|s| s.used);
        self.scopes.pop();
        if looped {
            // копии в `const` на каждой итерации: замыкания видят значения своей итерации
            let copies: String = params
                .iter()
                .zip(&loop_vars)
                .map(|(p, v)| format!("const {p} = {v};\n"))
                .collect();
            let _ = writeln!(
                self.out,
                "function {name}({}) {{\nwhile (true) {{\n{copies}{body}}}\n}}",
                loop_vars.join(", ")
            );
        } else {
            let _ = writeln!(
                self.out,
                "function {name}({}) {{\n{body}}}",
                params.join(", ")
            );
        }
    }
```

В `tail` добавить ветку перед `ExprKind::Match` / `_ =>`:
```rust
            ExprKind::Call { callee, args } if self.is_self_call(callee, args.len()) => {
                let vals: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                let call = self.self_call.as_mut().expect("проверено в is_self_call");
                call.used = true;
                let mut out: String = call
                    .params
                    .iter()
                    .zip(&vals)
                    .map(|(p, v)| format!("{p} = {v};\n"))
                    .collect();
                out.push_str("continue;\n");
                out
            }
```

Добавить методы:
```rust
    /// Хвостовой вызов текущей функции по её глобальному имени (не затенённому локальным).
    fn is_self_call(&self, callee: &Expr, argc: usize) -> bool {
        let Some(call) = &self.self_call else {
            return false;
        };
        matches!(&callee.kind, ExprKind::Var(name)
            if *name == call.name && argc == call.params.len() && self.var(name) == call.js)
    }

    /// Код во вложенной JS-функции: `continue` внешнего цикла там недоступен.
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.self_call.take();
        let out = f(self);
        self.self_call = saved;
        out
    }
```

В `expr` обернуть в `nested` всё, что становится отдельной JS-функцией:
```rust
            ExprKind::Lambda { params, body } => self.nested(|g| {
                g.scopes.push(HashMap::new());
                let ps: Vec<String> = params.iter().map(|p| g.bind(&p.name)).collect();
                let b = g.tail(body);
                g.scopes.pop();
                format!("(({}) => {{\n{b}}})", ps.join(", "))
            }),
```
```rust
            ExprKind::Block(stmts) => match stmts.as_slice() {
                [Stmt::Expr(only)] => self.expr(only),
                _ => self.nested(|g| format!("(() => {{\n{}}})()", g.tail(e))),
            },
            ExprKind::Match { .. } => self.nested(|g| format!("(() => {{\n{}}})()", g.tail(e))),
```

- [ ] **Step 3: Запустить всё** — `cargo test`, `cargo clippy --all-targets`, `cargo fmt`. Expected: зелёное; `tco` печатает ровно ожидаемый вывод (в том числе `[3, 2, 1]` — замыкания не делят одну переменную цикла, и `42` — локальная `shadowed` не принята за самовызов).

- [ ] **Step 4: Спека.** В `docs/superpowers/specs/2026-10-31-lumo-design.md`, раздел 3, таблица «Lumo → JS» — заменить строки `List`, «конструктор с полями», «конструктор без полей» на:
```markdown
| `List` | `null` (пустой) или ячейка `{ h, t }`, создаётся `$cons(h, t)`; `[x, ..rest]` разбирается за O(1) |
| конструктор с полями | `{ $: "Имя", _0: ..., _1: ... }` — тег равен имени конструктора |
| конструктор без полей | общий объект `{ $: "Имя" }` |
```
и абзац «Хвостовая рекурсия» дополнить предложением: «Самовызов распознаётся по глобальному имени (локальная переменная может его затенить); параметры копируются в `const` на каждой итерации, чтобы замыкания захватывали значения своей итерации; внутри лямбд и выражений-IIFE вызовы не переписываются.»

- [ ] **Step 5: Commit**
```bash
git add src tests/programs/tco.lumo tests/programs/tco.out docs/superpowers/specs/2026-10-31-lumo-design.md
git commit -m "feat: хвостовые самовызовы компилируются в цикл"
```

---

## Готово, когда

- `cargo test` зелёный: модульные тесты всех этапов, golden-программы `tuples`, `lists`, `adt`, `match`, `tco` и программы M1, CLI-тесты.
- `count(1000000, 0)` и обход списка из 200 000 элементов работают без переполнения стека.
- Неполный `match` и опровержимый `let` — ошибки компиляции с примером непокрытого значения.
- Следующий шаг — план M3 (stdlib, `Option`/`Result` в prelude, сахар `use`, `panic` с местом, предупреждения и снапшоты ошибок).

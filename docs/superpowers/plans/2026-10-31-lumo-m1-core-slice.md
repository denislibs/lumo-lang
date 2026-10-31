# Lumo M1 — сквозной срез ядра: план реализации

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Компилятор `lumo` на Rust, который проверяет типы (Хиндли–Милнер) и компилирует в JS программы из функций, замыканий, `let`, `if`, операторов и пайпов, и запускает их через `lumo run`.

**Architecture:** Конвейер чистых этапов: `lexer → parser → types (HM, SCC по функциям верхнего уровня) → codegen → JS`. Каждый этап — функция «вход → результат или `Vec<Diag>`». stdlib написана на Lumo через `@external` и вшита в бинарник. Бэкенд видит только `typed::Module`.

**Tech Stack:** Rust 2021 (stable), `ariadne 0.4` (вывод ошибок), Node.js ≥ 18 (запуск и golden-тесты).

**Спецификация:** `docs/superpowers/specs/2026-10-31-lumo-design.md`.

## Дорожная карта (каждая веха — отдельный план, пишется после завершения предыдущей)

| Веха | Содержание | Итог |
|---|---|---|
| **M1 (этот план)** | лексер целиком, парсер (fn, `@external`, let, блоки, if, лямбды, вызовы, операторы, пайп, `модуль.имя`), HM с let-полиморфизмом и SCC, codegen в JS, рантайм, prelude (`print`) и `int.to_string`, CLI `check/build/run`, golden-тесты | `lumo run fib.lumo` печатает результат |
| M2 | `List`, кортежи, `type` (ADT с параметрами), `match` + проверка полноты (Маранже), `let`-образцы, хвостовая рекурсия → цикл, `debug`/`inspect`, `$eq` для структур | алгебраические типы и рекурсия по спискам |
| M3 | stdlib целиком (`list`, `string`, `int`, `float`, `option`, `result`), `panic` с местом в исходнике, `Option`/`Result` в prelude, диагностика с labels/notes, снапшоты ошибок (`insta`), предупреждение о недостижимых ветках | язык v1 по спеке, кроме REPL |
| M4 | `lumo repl`: `rustyline`, хост-процесс Node, накопление окружения, `:t`, `:quit` | v1 завершён |

## Global Constraints

- Rust 2021, stable. Зависимости M1: только `ariadne = "0.4"`. Других крейтов не добавлять.
- Node.js ≥ 18 в `PATH` нужен для `lumo run` и golden-тестов.
- Все сообщения об ошибках компилятора и CLI — на русском.
- Сгенерированный JS и `runtime/lumo.js` не используют API Node — только стандартный JS и `console`.
- `Span` — байтовые смещения в исходнике. Идентификаторы Lumo — ASCII; строки и комментарии — любой Unicode.
- `Int` → JS `number` (безопасный диапазон ±2⁵³). Целочисленные `/` и `%` при делителе 0 дают `0`.
- Имена типов и конструкторов — с заглавной буквы, переменные и функции — со строчной.
- Пайп `a |> f(b)` ≡ `f(a, b)`; `a |> f` ≡ `f(a)`.
- Каждая задача заканчивается зелёным `cargo test` и коммитом.

### Сознательные упрощения M1 относительно спеки (закрываются в M2–M3)

- `typed::Module` = исходный AST + схемы типов глобалов (типы по узлам добавятся, когда понадобятся бэкенду).
- Переменные типов в аннотациях гибкие (как в OCaml): `fn f(x: a) -> Int { x + 1 }` принимается с `a = Int`.
- Парсер тестируется через S-выражения (`src/sexpr.rs`) вместо `insta`; `insta` появится в M3 для снапшотов ошибок.
- `Diag` пока без labels/notes: `span + message + hint`.

## Структура файлов M1

```
Cargo.toml
src/lib.rs            — объявления модулей
src/main.rs           — CLI
src/diagnostics.rs    — Span, Diag, render
src/lexer.rs          — Tok, Token, lex (scan + layout переводов строк)
src/ast.rs            — нетипизированный AST
src/sexpr.rs          — печать AST в S-выражения (для тестов)
src/parser.rs         — parse, parse_expr
src/types/mod.rs
src/types/ty.rs       — Type, Scheme, display
src/types/unify.rs    — Subst: fresh, unify, generalize, instantiate
src/types/deps.rs     — referenced_names, scc (Тарьян)
src/types/env.rs      — Env
src/types/infer.rs    — Checker, type_of_expr, check_module
src/typed.rs          — typed::Module
src/driver.rs         — stdlib, check, compile, Failure
src/codegen.rs        — emit_program
runtime/lumo.js       — JS-рантайм
stdlib/prelude.lumo
stdlib/int.lumo
tests/golden.rs       — сквозные тесты tests/programs/*.lumo → *.out
tests/programs/*.lumo, *.out
tests/cli.rs          — тесты бинарника
```

---

### Task 1: Скелет крейта и диагностика

**Files:**
- Create: `Cargo.toml`, `src/lib.rs`, `src/diagnostics.rs`, `.gitignore`

**Interfaces:**
- Produces: `diagnostics::Span { start: usize, end: usize }` (Copy) с `Span::new(start, end)`, `span.to(other) -> Span`; `diagnostics::Diag { span, message: String, hint: Option<String> }` с `Diag::error(span, msg)`, `.with_hint(h)`; `diagnostics::render(diags: &[Diag], file: &str, src: &str, color: bool) -> String`.

- [ ] **Step 1: Создать крейт**

`Cargo.toml`:
```toml
[package]
name = "lumo"
version = "0.1.0"
edition = "2021"

[dependencies]
ariadne = "0.4"
```

`.gitignore`:
```
/target
```

`src/lib.rs`:
```rust
pub mod diagnostics;
```

- [ ] **Step 2: Написать падающий тест** — в конец `src/diagnostics.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_message_line_and_hint() {
        let src = "fn main() {\n  1 + 2.5\n}\n";
        let d = Diag::error(Span::new(18, 21), "несовпадение типов")
            .with_hint("для Float используй +.");
        let out = render(&[d], "main.lumo", src, false);
        assert!(out.contains("несовпадение типов"), "{out}");
        assert!(out.contains("1 + 2.5"), "{out}");
        assert!(out.contains("для Float используй +."), "{out}");
        assert!(out.contains("main.lumo:2:"), "{out}");
    }

    #[test]
    fn span_to_joins_ranges() {
        assert_eq!(Span::new(2, 4).to(Span::new(7, 9)), Span::new(2, 9));
    }
}
```

- [ ] **Step 3: Запустить — должно не компилироваться**

Run: `cargo test diagnostics`
Expected: FAIL — `cannot find type Diag`.

- [ ] **Step 4: Реализация** — начало `src/diagnostics.rs` (над тестами):

```rust
use ariadne::{Config, IndexType, Label, Report, ReportKind, Source};

/// Байтовый диапазон в исходнике.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    /// Диапазон от начала `self` до конца `other`.
    pub fn to(self, other: Span) -> Span {
        Span::new(self.start, other.end)
    }
}

/// Ошибка компиляции.
#[derive(Debug, Clone, PartialEq)]
pub struct Diag {
    pub span: Span,
    pub message: String,
    pub hint: Option<String>,
}

impl Diag {
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Diag { span, message: message.into(), hint: None }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

/// Печатает ошибки с подсветкой кода.
pub fn render(diags: &[Diag], file: &str, src: &str, color: bool) -> String {
    let mut out = Vec::new();
    for d in diags {
        let config = Config::default()
            .with_color(color)
            .with_index_type(IndexType::Byte);
        let mut report = Report::build(ReportKind::Error, file, d.span.start)
            .with_config(config)
            .with_message(&d.message)
            .with_label(Label::new((file, d.span.start..d.span.end)));
        if let Some(hint) = &d.hint {
            report = report.with_help(hint);
        }
        report
            .finish()
            .write((file, Source::from(src)), &mut out)
            .expect("запись в Vec не падает");
    }
    String::from_utf8(out).expect("ariadne пишет UTF-8")
}
```

Если `cargo` выберет версию ariadne, где `IndexType` или сигнатура `Report::build` отличаются, закрепи `ariadne = "=0.4.1"`.

- [ ] **Step 5: Запустить тесты**

Run: `cargo test diagnostics`
Expected: PASS (2 теста).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore src/lib.rs src/diagnostics.rs
git commit -m "feat: crate skeleton and diagnostics rendering"
```

---

### Task 2: Лексер — токены

**Files:**
- Create: `src/lexer.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `Span`, `Diag` (Task 1).
- Produces: `lexer::Tok` (enum ниже), `lexer::Token { tok: Tok, span: Span }`, `lexer::lex(src: &str) -> Result<Vec<Token>, Vec<Diag>>`. Последний токен всегда `Tok::Eof`.

- [ ] **Step 1: Подключить модуль** — `src/lib.rs`:

```rust
pub mod diagnostics;
pub mod lexer;
```

- [ ] **Step 2: Написать падающие тесты** — в конец `src/lexer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use Tok::*;

    fn kinds(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    fn ident(s: &str) -> Tok {
        Ident(s.to_string())
    }

    #[test]
    fn function_tokens() {
        assert_eq!(
            kinds("fn add(a, b) { a + b }"),
            vec![
                Fn, ident("add"), LParen, ident("a"), Comma, ident("b"), RParen,
                LBrace, ident("a"), Plus, ident("b"), RBrace, Eof
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(kinds("42 3.14 1..2"), vec![Int(42), Float(3.14), Int(1), DotDot, Int(2), Eof]);
    }

    #[test]
    fn operators_longest_match() {
        assert_eq!(
            kinds("+. -> <=. <= <> |> == != && || ! % <. > @ #"),
            vec![PlusDot, Arrow, LeDot, Le, Concat, Pipe, EqEq, NotEq, AndAnd, OrOr, Bang, Percent, LtDot, Gt, At, Hash, Eof]
        );
    }

    #[test]
    fn strings_with_escapes_and_unicode() {
        assert_eq!(kinds(r#""привет\n\"мир\"""#), vec![Str("привет\n\"мир\"".to_string()), Eof]);
    }

    #[test]
    fn keywords_and_upper_names() {
        assert_eq!(
            kinds("let x = Some true false if else match type"),
            vec![Let, ident("x"), Eq, Upper("Some".to_string()), True, False, If, Else, Match, Type, Eof]
        );
    }

    #[test]
    fn comments_are_skipped() {
        assert_eq!(kinds("1 // комментарий"), vec![Int(1), Eof]);
    }

    #[test]
    fn spans_are_byte_offsets() {
        let toks = lex("ab  cd").unwrap();
        assert_eq!(toks[1].span, Span::new(4, 6));
    }

    #[test]
    fn unterminated_string_is_error() {
        let diags = lex("\"abc").unwrap_err();
        assert!(diags[0].message.contains("незакрытая строка"));
    }

    #[test]
    fn unexpected_char_is_error() {
        let diags = lex("1 $ 2").unwrap_err();
        assert!(diags[0].message.contains("неожиданный символ"));
    }
}
```

- [ ] **Step 3: Запустить**

Run: `cargo test lexer`
Expected: FAIL — не компилируется (`cannot find function lex`).

- [ ] **Step 4: Реализация** — начало `src/lexer.rs`:

```rust
use crate::diagnostics::{Diag, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),
    Upper(String),
    // ключевые слова
    Fn,
    Let,
    If,
    Else,
    Match,
    Type,
    True,
    False,
    // скобки и пунктуация
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semi,
    Dot,
    DotDot,
    Arrow,
    Eq,
    At,
    Hash,
    // операторы
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusDot,
    MinusDot,
    StarDot,
    SlashDot,
    Lt,
    Gt,
    Le,
    Ge,
    LtDot,
    GtDot,
    LeDot,
    GeDot,
    EqEq,
    NotEq,
    AndAnd,
    OrOr,
    Bang,
    Pipe,
    Concat,
    // служебные
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Текст → токены. Последний токен — `Eof`.
pub fn lex(src: &str) -> Result<Vec<Token>, Vec<Diag>> {
    scan(src)
}

fn scan(src: &str) -> Result<Vec<Token>, Vec<Diag>> {
    let bytes = src.as_bytes();
    let mut toks = Vec::new();
    let mut diags = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        match c {
            b' ' | b'\t' | b'\r' => i += 1,
            b'\n' => {
                toks.push(Token { tok: Tok::Newline, span: Span::new(i, i + 1) });
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'0'..=b'9' => {
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let is_float = bytes.get(i) == Some(&b'.')
                    && bytes.get(i + 1).map_or(false, |b| b.is_ascii_digit());
                if is_float {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    let value = src[start..i].parse().expect("цифры с точкой — валидный f64");
                    toks.push(Token { tok: Tok::Float(value), span: Span::new(start, i) });
                } else {
                    match src[start..i].parse::<i64>() {
                        Ok(n) => toks.push(Token { tok: Tok::Int(n), span: Span::new(start, i) }),
                        Err(_) => diags.push(Diag::error(Span::new(start, i), "целое число слишком большое")),
                    }
                }
            }
            b'"' => {
                i += 1;
                let mut value = String::new();
                let mut closed = false;
                while i < bytes.len() {
                    match bytes[i] {
                        b'"' => {
                            i += 1;
                            closed = true;
                            break;
                        }
                        b'\\' => {
                            let esc = src[i + 1..].chars().next();
                            let len = 1 + esc.map_or(0, |e| e.len_utf8());
                            match esc {
                                Some('n') => value.push('\n'),
                                Some('t') => value.push('\t'),
                                Some('"') => value.push('"'),
                                Some('\\') => value.push('\\'),
                                _ => diags.push(Diag::error(
                                    Span::new(i, i + len),
                                    "неизвестная escape-последовательность",
                                )),
                            }
                            i += len;
                        }
                        _ => {
                            let ch = src[i..].chars().next().expect("i на границе символа");
                            value.push(ch);
                            i += ch.len_utf8();
                        }
                    }
                }
                if !closed {
                    diags.push(Diag::error(Span::new(start, i), "незакрытая строка"));
                }
                toks.push(Token { tok: Tok::Str(value), span: Span::new(start, i) });
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let text = &src[start..i];
                let tok = match text {
                    "fn" => Tok::Fn,
                    "let" => Tok::Let,
                    "if" => Tok::If,
                    "else" => Tok::Else,
                    "match" => Tok::Match,
                    "type" => Tok::Type,
                    "true" => Tok::True,
                    "false" => Tok::False,
                    _ if c.is_ascii_uppercase() => Tok::Upper(text.to_string()),
                    _ => Tok::Ident(text.to_string()),
                };
                toks.push(Token { tok, span: Span::new(start, i) });
            }
            _ => {
                let mut matched = false;
                for len in [3, 2, 1] {
                    if let Some(tok) = src.get(i..i + len).and_then(operator) {
                        toks.push(Token { tok, span: Span::new(i, i + len) });
                        i += len;
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    let ch = src[i..].chars().next().expect("i на границе символа");
                    diags.push(Diag::error(
                        Span::new(i, i + ch.len_utf8()),
                        format!("неожиданный символ `{ch}`"),
                    ));
                    i += ch.len_utf8();
                }
            }
        }
    }
    toks.push(Token { tok: Tok::Eof, span: Span::new(src.len(), src.len()) });
    if diags.is_empty() {
        Ok(toks)
    } else {
        Err(diags)
    }
}

fn operator(text: &str) -> Option<Tok> {
    Some(match text {
        "<=." => Tok::LeDot,
        ">=." => Tok::GeDot,
        "->" => Tok::Arrow,
        "==" => Tok::EqEq,
        "!=" => Tok::NotEq,
        "<=" => Tok::Le,
        ">=" => Tok::Ge,
        "&&" => Tok::AndAnd,
        "||" => Tok::OrOr,
        "|>" => Tok::Pipe,
        "<>" => Tok::Concat,
        ".." => Tok::DotDot,
        "+." => Tok::PlusDot,
        "-." => Tok::MinusDot,
        "*." => Tok::StarDot,
        "/." => Tok::SlashDot,
        "<." => Tok::LtDot,
        ">." => Tok::GtDot,
        "(" => Tok::LParen,
        ")" => Tok::RParen,
        "{" => Tok::LBrace,
        "}" => Tok::RBrace,
        "[" => Tok::LBracket,
        "]" => Tok::RBracket,
        "," => Tok::Comma,
        ":" => Tok::Colon,
        ";" => Tok::Semi,
        "." => Tok::Dot,
        "=" => Tok::Eq,
        "@" => Tok::At,
        "#" => Tok::Hash,
        "+" => Tok::Plus,
        "-" => Tok::Minus,
        "*" => Tok::Star,
        "/" => Tok::Slash,
        "%" => Tok::Percent,
        "<" => Tok::Lt,
        ">" => Tok::Gt,
        "!" => Tok::Bang,
        _ => return None,
    })
}
```

- [ ] **Step 5: Запустить тесты**

Run: `cargo test lexer`
Expected: PASS (9 тестов).

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/lexer.rs
git commit -m "feat(lexer): tokens, literals, operators, strings"
```

---

### Task 3: Лексер — значимые переводы строк

Перевод строки разделяет выражения в блоке, кроме случаев:
- внутри `(` или `[` (ближайшая открытая скобка — круглая или квадратная);
- после токена, требующего продолжения (бинарный оператор, `,`, `->`, `=`, `.`, `:`, открывающая скобка);
- перед токеном, продолжающим выражение: `|> + * / % +. *. /. <> == != && || > >= <= >. >=. <=. <. .` и `else`. Не продолжают: `-`, `-.`, `<` (унарный минус и будущий JSX);
- повторные переводы строк схлопываются, ведущие — удаляются.

**Files:**
- Modify: `src/lexer.rs`

**Interfaces:**
- Produces: `lex` теперь возвращает токены, где `Newline` встречается только как значимый разделитель.

- [ ] **Step 1: Падающие тесты** — добавить в `mod tests` в `src/lexer.rs`:

```rust
    #[test]
    fn newlines_separate_statements_in_braces() {
        assert_eq!(
            kinds("{\n a\n b\n}"),
            vec![LBrace, ident("a"), Newline, ident("b"), Newline, RBrace, Eof]
        );
    }

    #[test]
    fn leading_pipe_continues_line() {
        assert_eq!(kinds("x\n|> f\n|> g"), vec![ident("x"), Pipe, ident("f"), Pipe, ident("g"), Eof]);
    }

    #[test]
    fn newlines_ignored_inside_parens() {
        assert_eq!(
            kinds("f(\n1,\n2\n)"),
            vec![ident("f"), LParen, Int(1), Comma, Int(2), RParen, Eof]
        );
    }

    #[test]
    fn trailing_operator_continues_line() {
        assert_eq!(kinds("a +\nb"), vec![ident("a"), Plus, ident("b"), Eof]);
    }

    #[test]
    fn else_on_next_line() {
        assert_eq!(kinds("}\nelse {"), vec![RBrace, Else, LBrace, Eof]);
    }

    #[test]
    fn blank_lines_collapse_and_leading_dropped() {
        assert_eq!(kinds("\n\na\n\n\nb"), vec![ident("a"), Newline, ident("b"), Eof]);
    }

    #[test]
    fn minus_does_not_continue() {
        assert_eq!(kinds("a\n-b"), vec![ident("a"), Newline, Minus, ident("b"), Eof]);
    }

    #[test]
    fn braces_inside_parens_are_significant() {
        assert_eq!(
            kinds("f(fn() {\na\nb\n})"),
            vec![
                ident("f"), LParen, Fn, LParen, RParen, LBrace, ident("a"), Newline,
                ident("b"), Newline, RBrace, RParen, Eof
            ]
        );
    }
```

- [ ] **Step 2: Запустить**

Run: `cargo test lexer`
Expected: FAIL — новые тесты падают (лишние `Newline`).

- [ ] **Step 3: Реализация** — заменить `lex` и добавить `layout` в `src/lexer.rs`:

```rust
/// Текст → токены. Последний токен — `Eof`.
pub fn lex(src: &str) -> Result<Vec<Token>, Vec<Diag>> {
    scan(src).map(layout)
}

/// Оставляет только значимые переводы строк (правила — в описании Task 3 плана M1).
fn layout(raw: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut open: Vec<Tok> = Vec::new();
    for (idx, t) in raw.iter().enumerate() {
        match t.tok {
            Tok::LParen | Tok::LBracket | Tok::LBrace => open.push(t.tok.clone()),
            Tok::RParen | Tok::RBracket | Tok::RBrace => {
                open.pop();
            }
            _ => {}
        }
        if t.tok == Tok::Newline {
            let in_parens = matches!(open.last(), Some(Tok::LParen) | Some(Tok::LBracket));
            let prev = out.last().map(|p| &p.tok);
            let after_continuation = prev.map_or(true, continues_after);
            let duplicate = prev == Some(&Tok::Newline);
            let next = raw[idx + 1..].iter().find(|n| n.tok != Tok::Newline);
            let before_continuation = next.map_or(false, |n| continues_before(&n.tok));
            if in_parens || after_continuation || duplicate || before_continuation {
                continue;
            }
        }
        out.push(t.clone());
    }
    out
}

fn continues_after(tok: &Tok) -> bool {
    use Tok::*;
    matches!(
        tok,
        Plus | Minus | Star | Slash | Percent | PlusDot | MinusDot | StarDot | SlashDot
            | Lt | Gt | Le | Ge | LtDot | GtDot | LeDot | GeDot | EqEq | NotEq | AndAnd
            | OrOr | Pipe | Concat | Comma | Arrow | Eq | Dot | Colon | LParen | LBracket
            | LBrace
    )
}

fn continues_before(tok: &Tok) -> bool {
    use Tok::*;
    matches!(
        tok,
        Pipe | Plus | Star | Slash | Percent | PlusDot | StarDot | SlashDot | Concat
            | EqEq | NotEq | AndAnd | OrOr | Gt | Ge | Le | GtDot | GeDot | LeDot | LtDot
            | Dot | Else
    )
}
```

- [ ] **Step 4: Запустить тесты**

Run: `cargo test lexer`
Expected: PASS (17 тестов).

- [ ] **Step 5: Commit**

```bash
git add src/lexer.rs
git commit -m "feat(lexer): significant newlines with continuation rules"
```

---

### Task 4: AST и парсер выражений

**Files:**
- Create: `src/ast.rs`, `src/sexpr.rs`, `src/parser.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `lexer::{Tok, Token, lex}`, `Diag`, `Span`.
- Produces:
  - AST-типы ниже (`ast::Module`, `Item`, `FnDecl`, `ExternalDecl`, `Param`, `TypeExpr`, `Expr { kind, span }`, `ExprKind`, `Stmt`, `BinOp`, `UnOp`), `BinOp::symbol()`, `UnOp::symbol()`, `TypeExpr::span()`, `Item::name()`.
  - `parser::parse_expr(tokens: Vec<Token>) -> Result<ast::Expr, Vec<Diag>>`.
  - `sexpr::expr(&Expr) -> String`, `sexpr::type_expr(&TypeExpr) -> String`.
  - Пайп раскрывается в парсере в `ExprKind::Call`; `имя.имя` → `ExprKind::ModuleAccess`.

Приоритеты (выше — сильнее): постфикс (вызов, `.`) > унарные `- -. !` > `* / % *. /.` (7) > `+ - +. -. <>` (6) > сравнения (5) > `== !=` (4) > `&&` (3) > `||` (2) > `|>` (1). Все бинарные — левоассоциативные.

- [ ] **Step 1: AST** — `src/ast.rs`:

```rust
use crate::diagnostics::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Fn(FnDecl),
    External(ExternalDecl),
}

impl Item {
    pub fn name(&self) -> &str {
        match self {
            Item::Fn(f) => &f.name,
            Item::External(e) => &e.name,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<TypeExpr>,
    pub body: Expr,
    pub span: Span,
}

/// `@external("js.expr") fn name(params) -> Ret` — все типы обязательны.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalDecl {
    pub js: String,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: TypeExpr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Named { name: String, args: Vec<TypeExpr>, span: Span },
    Var { name: String, span: Span },
    Fn { params: Vec<TypeExpr>, ret: Box<TypeExpr>, span: Span },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named { span, .. } | TypeExpr::Var { span, .. } | TypeExpr::Fn { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Var(String),
    /// `int.to_string`
    ModuleAccess { module: String, name: String },
    Unary { op: UnOp, expr: Box<Expr> },
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Call { callee: Box<Expr>, args: Vec<Expr> },
    Lambda { params: Vec<Param>, body: Box<Expr> },
    If { cond: Box<Expr>, then_branch: Box<Expr>, else_branch: Option<Box<Expr>> },
    /// Значение блока — последнее выражение; пустой блок или `let` в конце дают `Unit`.
    Block(Vec<Stmt>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let { name: String, ty: Option<TypeExpr>, value: Expr, span: Span },
    Expr(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    AddF,
    SubF,
    MulF,
    DivF,
    Lt,
    Gt,
    Le,
    Ge,
    LtF,
    GtF,
    LeF,
    GeF,
    Eq,
    Ne,
    And,
    Or,
    Concat,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        use BinOp::*;
        match self {
            Add => "+",
            Sub => "-",
            Mul => "*",
            Div => "/",
            Rem => "%",
            AddF => "+.",
            SubF => "-.",
            MulF => "*.",
            DivF => "/.",
            Lt => "<",
            Gt => ">",
            Le => "<=",
            Ge => ">=",
            LtF => "<.",
            GtF => ">.",
            LeF => "<=.",
            GeF => ">=.",
            Eq => "==",
            Ne => "!=",
            And => "&&",
            Or => "||",
            Concat => "<>",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    NegF,
    Not,
}

impl UnOp {
    pub fn symbol(self) -> &'static str {
        match self {
            UnOp::Neg => "-",
            UnOp::NegF => "-.",
            UnOp::Not => "!",
        }
    }
}
```

- [ ] **Step 2: Печать в S-выражения** — `src/sexpr.rs`:

```rust
//! Компактная печать AST для тестов: `(+ 1 (* 2 3))`.

use crate::ast::{Expr, ExprKind, Stmt, TypeExpr};

pub fn expr(e: &Expr) -> String {
    match &e.kind {
        ExprKind::Int(n) => n.to_string(),
        ExprKind::Float(f) => format!("{f:?}"),
        ExprKind::Str(s) => format!("{s:?}"),
        ExprKind::Bool(b) => b.to_string(),
        ExprKind::Var(name) => name.clone(),
        ExprKind::ModuleAccess { module, name } => format!("{module}.{name}"),
        ExprKind::Unary { op, expr: inner } => format!("({} {})", op.symbol(), expr(inner)),
        ExprKind::Binary { op, lhs, rhs } => format!("({} {} {})", op.symbol(), expr(lhs), expr(rhs)),
        ExprKind::Call { callee, args } => {
            let args: String = args.iter().map(|a| format!(" {}", expr(a))).collect();
            format!("(call {}{args})", expr(callee))
        }
        ExprKind::Lambda { params, body } => {
            let names: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
            format!("(fn ({}) {})", names.join(" "), expr(body))
        }
        ExprKind::If { cond, then_branch, else_branch } => match else_branch {
            Some(e2) => format!("(if {} {} {})", expr(cond), expr(then_branch), expr(e2)),
            None => format!("(if {} {})", expr(cond), expr(then_branch)),
        },
        ExprKind::Block(stmts) => {
            let body: String = stmts.iter().map(|s| format!(" {}", stmt(s))).collect();
            format!("(block{body})")
        }
    }
}

fn stmt(s: &Stmt) -> String {
    match s {
        Stmt::Let { name, value, .. } => format!("(let {name} {})", expr(value)),
        Stmt::Expr(e) => expr(e),
    }
}

pub fn type_expr(t: &TypeExpr) -> String {
    match t {
        TypeExpr::Named { name, args, .. } if args.is_empty() => name.clone(),
        TypeExpr::Named { name, args, .. } => format!("{name}({})", type_list(args)),
        TypeExpr::Var { name, .. } => name.clone(),
        TypeExpr::Fn { params, ret, .. } => format!("fn({}) -> {}", type_list(params), type_expr(ret)),
    }
}

fn type_list(ts: &[TypeExpr]) -> String {
    ts.iter().map(type_expr).collect::<Vec<_>>().join(", ")
}
```

`src/lib.rs`:
```rust
pub mod ast;
pub mod diagnostics;
pub mod lexer;
pub mod parser;
pub mod sexpr;
```

- [ ] **Step 3: Падающие тесты** — в конец `src/parser.rs` (файл пока содержит только тесты):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::sexpr;

    fn p(src: &str) -> String {
        sexpr::expr(&parse_expr(lex(src).unwrap()).unwrap())
    }

    fn perr(src: &str) -> String {
        parse_expr(lex(src).unwrap()).unwrap_err()[0].message.clone()
    }

    #[test]
    fn precedence_and_associativity() {
        assert_eq!(p("1 + 2 * 3"), "(+ 1 (* 2 3))");
        assert_eq!(p("a - b - c"), "(- (- a b) c)");
        assert_eq!(p("a == b && c < d || e"), "(|| (&& (== a b) (< c d)) e)");
        assert_eq!(p("(1 + 2) * 3"), "(* (+ 1 2) 3)");
    }

    #[test]
    fn unary_operators() {
        assert_eq!(p("-x +. 1.5"), "(+. (- x) 1.5)");
        assert_eq!(p("!f(x)"), "(! (call f x))");
    }

    #[test]
    fn pipe_inserts_first_argument() {
        assert_eq!(p("x |> f(1) |> g"), "(call g (call f x 1))");
    }

    #[test]
    fn module_access_and_calls() {
        assert_eq!(p("list.map(xs, f)"), "(call list.map xs f)");
        assert_eq!(p("f(a)(b)"), "(call (call f a) b)");
    }

    #[test]
    fn lambdas_ifs_blocks() {
        assert_eq!(p("fn(x, y) { x }"), "(fn (x y) (block x))");
        assert_eq!(
            p("if a { 1 } else if b { 2 } else { 3 }"),
            "(if a (block 1) (if b (block 2) (block 3)))"
        );
        assert_eq!(p("{\n let x = 1\n x + 1\n}"), "(block (let x 1) (+ x 1))");
        assert_eq!(p("\"hi\" <> s"), "(<> \"hi\" s)");
        assert_eq!(p("{}"), "(block)");
    }

    #[test]
    fn multiline_pipeline() {
        assert_eq!(p("{\n  xs\n  |> f\n  |> g(1)\n}"), "(block (call g (call f xs) 1))");
    }

    #[test]
    fn syntax_errors() {
        assert!(perr("1 +").contains("ожидалось выражение"));
        assert!(perr("f(1, 2").contains("ожидалось"));
        assert!(perr("(1 + 2).x").contains("доступ через точку"));
        assert!(perr("1 2").contains("лишний текст"));
    }
}
```

- [ ] **Step 4: Запустить**

Run: `cargo test parser`
Expected: FAIL — `cannot find function parse_expr`.

- [ ] **Step 5: Реализация** — начало `src/parser.rs`:

```rust
use crate::ast::{BinOp, Expr, ExprKind, Param, Stmt, TypeExpr, UnOp};
use crate::diagnostics::{Diag, Span};
use crate::lexer::{Tok, Token};

type PResult<T> = Result<T, Diag>;

/// Разбирает одно выражение (тесты, REPL).
pub fn parse_expr(tokens: Vec<Token>) -> Result<Expr, Vec<Diag>> {
    let mut p = Parser::new(tokens);
    p.skip_newlines();
    let result = p.expr().and_then(|e| {
        p.skip_newlines();
        if p.at(&Tok::Eof) {
            Ok(e)
        } else {
            Err(Diag::error(p.span(), "лишний текст после выражения"))
        }
    });
    result.map_err(|d| vec![d])
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    diags: Vec<Diag>,
}

impl Parser {
    fn new(toks: Vec<Token>) -> Self {
        Parser { toks, pos: 0, diags: Vec::new() }
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }

    fn span(&self) -> Span {
        self.toks[self.pos].span
    }

    fn at(&self, tok: &Tok) -> bool {
        self.peek() == tok
    }

    fn advance(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if t.tok != Tok::Eof {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.at(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: &Tok, what: &str) -> PResult<Span> {
        if self.at(tok) {
            Ok(self.advance().span)
        } else {
            Err(Diag::error(self.span(), format!("ожидалось `{what}`")))
        }
    }

    fn skip_newlines(&mut self) {
        while self.eat(&Tok::Newline) {}
    }

    fn ident(&mut self) -> PResult<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let span = self.advance().span;
                Ok((name, span))
            }
            _ => Err(Diag::error(self.span(), "ожидалось имя")),
        }
    }

    fn params(&mut self) -> PResult<Vec<Param>> {
        self.expect(&Tok::LParen, "(")?;
        let mut params = Vec::new();
        while !self.at(&Tok::RParen) {
            let (name, span) = self.ident()?;
            let ty = if self.eat(&Tok::Colon) { Some(self.type_expr()?) } else { None };
            params.push(Param { name, ty, span });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RParen, ")")?;
        Ok(params)
    }

    fn type_expr(&mut self) -> PResult<TypeExpr> {
        let t = self.advance();
        match t.tok {
            Tok::Upper(name) => {
                let mut args = Vec::new();
                let mut end = t.span;
                if self.eat(&Tok::LParen) {
                    while !self.at(&Tok::RParen) {
                        args.push(self.type_expr()?);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    end = self.expect(&Tok::RParen, ")")?;
                }
                Ok(TypeExpr::Named { name, args, span: t.span.to(end) })
            }
            Tok::Ident(name) => Ok(TypeExpr::Var { name, span: t.span }),
            Tok::Fn => {
                self.expect(&Tok::LParen, "(")?;
                let mut params = Vec::new();
                while !self.at(&Tok::RParen) {
                    params.push(self.type_expr()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(&Tok::RParen, ")")?;
                self.expect(&Tok::Arrow, "->")?;
                let ret = self.type_expr()?;
                let span = t.span.to(ret.span());
                Ok(TypeExpr::Fn { params, ret: Box::new(ret), span })
            }
            _ => Err(Diag::error(t.span, "ожидался тип")),
        }
    }

    fn block(&mut self) -> PResult<Expr> {
        let start = self.expect(&Tok::LBrace, "{")?;
        let mut stmts = Vec::new();
        loop {
            while self.eat(&Tok::Newline) || self.eat(&Tok::Semi) {}
            if self.at(&Tok::RBrace) {
                break;
            }
            stmts.push(self.stmt()?);
            if !matches!(self.peek(), Tok::Newline | Tok::Semi | Tok::RBrace) {
                return Err(Diag::error(self.span(), "ожидался перевод строки или `}`"));
            }
        }
        let end = self.expect(&Tok::RBrace, "}")?;
        Ok(Expr { kind: ExprKind::Block(stmts), span: start.to(end) })
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        if !self.at(&Tok::Let) {
            return Ok(Stmt::Expr(self.expr()?));
        }
        let start = self.advance().span;
        let (name, _) = self.ident()?;
        let ty = if self.eat(&Tok::Colon) { Some(self.type_expr()?) } else { None };
        self.expect(&Tok::Eq, "=")?;
        let value = self.expr()?;
        let span = start.to(value.span);
        Ok(Stmt::Let { name, ty, value, span })
    }

    fn expr(&mut self) -> PResult<Expr> {
        self.binary(0)
    }

    fn binary(&mut self, min_prec: u8) -> PResult<Expr> {
        let mut lhs = self.unary()?;
        loop {
            let Some((prec, op)) = binop(self.peek()) else { break };
            if prec < min_prec {
                break;
            }
            self.advance();
            let rhs = self.binary(prec + 1)?;
            let span = lhs.span.to(rhs.span);
            lhs = match op {
                None => pipe(lhs, rhs),
                Some(op) => Expr {
                    kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
                    span,
                },
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::MinusDot => UnOp::NegF,
            Tok::Bang => UnOp::Not,
            _ => return self.postfix(),
        };
        let start = self.advance().span;
        let inner = self.unary()?;
        let span = start.to(inner.span);
        Ok(Expr { kind: ExprKind::Unary { op, expr: Box::new(inner) }, span })
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let mut e = self.primary()?;
        loop {
            if self.eat(&Tok::LParen) {
                let mut args = Vec::new();
                while !self.at(&Tok::RParen) {
                    args.push(self.expr()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                let end = self.expect(&Tok::RParen, ")")?;
                let span = e.span.to(end);
                e = Expr { kind: ExprKind::Call { callee: Box::new(e), args }, span };
            } else if self.eat(&Tok::Dot) {
                let (name, name_span) = self.ident()?;
                let span = e.span.to(name_span);
                e = match e.kind {
                    ExprKind::Var(module) => Expr { kind: ExprKind::ModuleAccess { module, name }, span },
                    _ => {
                        return Err(Diag::error(
                            span,
                            "доступ через точку возможен только к функциям модулей, например list.map",
                        ))
                    }
                };
            } else {
                break;
            }
        }
        Ok(e)
    }

    fn primary(&mut self) -> PResult<Expr> {
        if self.at(&Tok::LBrace) {
            return self.block();
        }
        let t = self.advance();
        let kind = match t.tok {
            Tok::Int(n) => ExprKind::Int(n),
            Tok::Float(f) => ExprKind::Float(f),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::Ident(name) => ExprKind::Var(name),
            Tok::LParen => {
                let inner = self.expr()?;
                self.expect(&Tok::RParen, ")")?;
                return Ok(inner);
            }
            Tok::If => return self.if_expr(t.span),
            Tok::Fn => return self.lambda(t.span),
            _ => return Err(Diag::error(t.span, "ожидалось выражение")),
        };
        Ok(Expr { kind, span: t.span })
    }

    fn if_expr(&mut self, start: Span) -> PResult<Expr> {
        let cond = self.expr()?;
        let then_branch = self.block()?;
        let mut end = then_branch.span;
        let else_branch = if self.eat(&Tok::Else) {
            let e = if self.at(&Tok::If) {
                let s = self.advance().span;
                self.if_expr(s)?
            } else {
                self.block()?
            };
            end = e.span;
            Some(Box::new(e))
        } else {
            None
        };
        Ok(Expr {
            kind: ExprKind::If { cond: Box::new(cond), then_branch: Box::new(then_branch), else_branch },
            span: start.to(end),
        })
    }

    fn lambda(&mut self, start: Span) -> PResult<Expr> {
        let params = self.params()?;
        let body = self.block()?;
        let span = start.to(body.span);
        Ok(Expr { kind: ExprKind::Lambda { params, body: Box::new(body) }, span })
    }
}

/// Приоритет и оператор; `None` вместо оператора — пайп.
fn binop(tok: &Tok) -> Option<(u8, Option<BinOp>)> {
    use BinOp::*;
    let (prec, op) = match tok {
        Tok::Pipe => return Some((1, None)),
        Tok::OrOr => (2, Or),
        Tok::AndAnd => (3, And),
        Tok::EqEq => (4, Eq),
        Tok::NotEq => (4, Ne),
        Tok::Lt => (5, Lt),
        Tok::Gt => (5, Gt),
        Tok::Le => (5, Le),
        Tok::Ge => (5, Ge),
        Tok::LtDot => (5, LtF),
        Tok::GtDot => (5, GtF),
        Tok::LeDot => (5, LeF),
        Tok::GeDot => (5, GeF),
        Tok::Plus => (6, Add),
        Tok::Minus => (6, Sub),
        Tok::PlusDot => (6, AddF),
        Tok::MinusDot => (6, SubF),
        Tok::Concat => (6, Concat),
        Tok::Star => (7, Mul),
        Tok::Slash => (7, Div),
        Tok::Percent => (7, Rem),
        Tok::StarDot => (7, MulF),
        Tok::SlashDot => (7, DivF),
        _ => return None,
    };
    Some((prec, Some(op)))
}

/// `a |> f(b)` → `f(a, b)`; `a |> f` → `f(a)`.
fn pipe(lhs: Expr, rhs: Expr) -> Expr {
    let span = lhs.span.to(rhs.span);
    match rhs.kind {
        ExprKind::Call { callee, mut args } => {
            args.insert(0, lhs);
            Expr { kind: ExprKind::Call { callee, args }, span }
        }
        other => {
            let callee = Expr { kind: other, span: rhs.span };
            Expr { kind: ExprKind::Call { callee: Box::new(callee), args: vec![lhs] }, span }
        }
    }
}
```

Поле `diags` пока не используется (используется в Task 5); если компилятор выдаёт предупреждение `dead_code` — это ожидаемо.

- [ ] **Step 6: Запустить тесты**

Run: `cargo test parser`
Expected: PASS (7 тестов).

- [ ] **Step 7: Commit**

```bash
git add src/lib.rs src/ast.rs src/sexpr.rs src/parser.rs
git commit -m "feat(parser): AST and expression parser with Pratt operators and pipe"
```

---

### Task 5: Парсер объявлений и восстановление после ошибок

**Files:**
- Modify: `src/parser.rs`

**Interfaces:**
- Consumes: всё из Task 4.
- Produces: `parser::parse(tokens: Vec<Token>) -> Result<ast::Module, Vec<Diag>>` — собирает все синтаксические ошибки (по одной на объявление).

- [ ] **Step 1: Падающие тесты** — добавить в `mod tests` в `src/parser.rs`:

```rust
    use crate::ast::{Item, Module};

    fn module(src: &str) -> Module {
        parse(lex(src).unwrap()).unwrap()
    }

    fn module_errs(src: &str) -> Vec<Diag> {
        parse(lex(src).unwrap()).unwrap_err()
    }

    #[test]
    fn function_declaration_with_annotations() {
        let m = module("fn add(a: Int, b) -> Int { a + b }");
        let Item::Fn(f) = &m.items[0] else { panic!("ожидалась функция") };
        assert_eq!(f.name, "add");
        assert_eq!(sexpr::type_expr(f.params[0].ty.as_ref().unwrap()), "Int");
        assert!(f.params[1].ty.is_none());
        assert_eq!(sexpr::type_expr(f.ret.as_ref().unwrap()), "Int");
        assert_eq!(sexpr::expr(&f.body), "(block (+ a b))");
    }

    #[test]
    fn function_and_generic_types() {
        let m = module("fn f(g: fn(a, List(Int)) -> b) { g }");
        let Item::Fn(f) = &m.items[0] else { panic!("ожидалась функция") };
        assert_eq!(sexpr::type_expr(f.params[0].ty.as_ref().unwrap()), "fn(a, List(Int)) -> b");
    }

    #[test]
    fn external_declaration() {
        let m = module("@external(\"Math.sqrt\")\nfn sqrt(x: Float) -> Float");
        let Item::External(e) = &m.items[0] else { panic!("ожидался @external") };
        assert_eq!(e.js, "Math.sqrt");
        assert_eq!(e.name, "sqrt");
        assert_eq!(sexpr::type_expr(&e.ret), "Float");
    }

    #[test]
    fn external_requires_types() {
        let errs = module_errs("@external(\"f\")\nfn f(x) -> Int");
        assert!(errs[0].message.contains("обязательны"));
    }

    #[test]
    fn several_items_with_comments() {
        let m = module("// модуль\nfn a() { 1 }\n\n\nfn b() {\n  a()\n}\n");
        let names: Vec<&str> = m.items.iter().map(|i| i.name()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn recovers_and_reports_all_errors() {
        let errs = module_errs("fn a() { 1 + }\nfn b() { 2 }\nfn c() { ) }");
        assert_eq!(errs.len(), 2, "{errs:?}");
    }

    #[test]
    fn rejects_unknown_top_level() {
        let errs = module_errs("let x = 1");
        assert!(errs[0].message.contains("ожидалось объявление"));
    }
```

- [ ] **Step 2: Запустить**

Run: `cargo test parser`
Expected: FAIL — `cannot find function parse`.

- [ ] **Step 3: Реализация** — в `src/parser.rs` расширить импорт и добавить функции:

Заменить строку импорта AST на:
```rust
use crate::ast::{BinOp, Expr, ExprKind, ExternalDecl, FnDecl, Item, Module, Param, Stmt, TypeExpr, UnOp};
```

Добавить после `parse_expr`:
```rust
/// Разбирает файл. Ошибки собираются по всем объявлениям.
pub fn parse(tokens: Vec<Token>) -> Result<Module, Vec<Diag>> {
    let mut p = Parser::new(tokens);
    let module = p.module();
    if p.diags.is_empty() {
        Ok(module)
    } else {
        Err(p.diags)
    }
}
```

Добавить в `impl Parser`:
```rust
    fn module(&mut self) -> Module {
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::Eof) {
                break;
            }
            let start = self.pos;
            match self.item() {
                Ok(item) => items.push(item),
                Err(d) => {
                    self.diags.push(d);
                    // Гарантия прогресса: ошибка на первом же токене (например, `type` в M1).
                    if self.pos == start {
                        self.advance();
                    }
                    self.synchronize();
                }
            }
        }
        Module { items }
    }

    /// Пропускает токены до начала следующего объявления на новой строке.
    fn synchronize(&mut self) {
        while !self.at(&Tok::Eof) {
            let after_newline = self.pos > 0 && self.toks[self.pos - 1].tok == Tok::Newline;
            if after_newline && matches!(self.peek(), Tok::Fn | Tok::At | Tok::Type) {
                return;
            }
            self.advance();
        }
    }

    fn item(&mut self) -> PResult<Item> {
        match self.peek() {
            Tok::Fn => Ok(Item::Fn(self.fn_decl()?)),
            Tok::At => Ok(Item::External(self.external()?)),
            _ => Err(Diag::error(self.span(), "ожидалось объявление: fn или @external")),
        }
    }

    fn fn_decl(&mut self) -> PResult<FnDecl> {
        let start = self.expect(&Tok::Fn, "fn")?;
        let (name, _) = self.ident()?;
        let params = self.params()?;
        let ret = if self.eat(&Tok::Arrow) { Some(self.type_expr()?) } else { None };
        let body = self.block()?;
        Ok(FnDecl { name, params, ret, span: start.to(body.span), body })
    }

    fn external(&mut self) -> PResult<ExternalDecl> {
        let start = self.expect(&Tok::At, "@")?;
        let (attr, attr_span) = self.ident()?;
        if attr != "external" {
            return Err(Diag::error(attr_span, format!("неизвестный атрибут @{attr}")));
        }
        self.expect(&Tok::LParen, "(")?;
        let t = self.advance();
        let Tok::Str(js) = t.tok else {
            return Err(Diag::error(t.span, "ожидалась строка с JS-выражением"));
        };
        self.expect(&Tok::RParen, ")")?;
        self.skip_newlines();
        self.expect(&Tok::Fn, "fn")?;
        let (name, _) = self.ident()?;
        let params = self.params()?;
        if let Some(p) = params.iter().find(|p| p.ty.is_none()) {
            return Err(Diag::error(p.span, "у @external-функции типы параметров обязательны"));
        }
        self.expect(&Tok::Arrow, "->")?;
        let ret = self.type_expr()?;
        let span = start.to(ret.span());
        Ok(ExternalDecl { js, name, params, ret, span })
    }
```

- [ ] **Step 4: Запустить тесты**

Run: `cargo test parser`
Expected: PASS (14 тестов).

- [ ] **Step 5: Commit**

```bash
git add src/parser.rs
git commit -m "feat(parser): fn and @external declarations with error recovery"
```

---

### Task 6: Типы и унификация

**Files:**
- Create: `src/types/mod.rs`, `src/types/ty.rs`, `src/types/unify.rs`
- Modify: `src/lib.rs`

**Interfaces:**
- Produces:
  - `types::ty::{TVar (= u32), Type::{Con(String, Vec<Type>), Fn(Vec<Type>, Box<Type>), Var(TVar)}, Scheme { vars: Vec<TVar>, ty: Type }}`, конструкторы `Type::int() / float() / bool() / string() / unit() / con(name)`, `Scheme::mono(ty)`, `display(&Type) -> String` (переменные → `a`, `b`, … по порядку появления).
  - `types::unify::{Subst, UnifyError::{Mismatch, Occurs}}`: `Subst::new()`, `fresh(level) -> Type`, `shallow(&Type) -> Type`, `zonk(&Type) -> Type`, `unify(&Type, &Type) -> Result<(), UnifyError>`, `generalize(&Type, level) -> Scheme`, `instantiate(&Scheme, level) -> Type`.

Обобщение по уровням: у каждой свободной переменной есть уровень; `generalize(t, L)` квантифицирует переменные с уровнем > L; при связывании переменной уровни переменных внутри типа понижаются до её уровня.

- [ ] **Step 1: Модули** — `src/types/mod.rs`:

```rust
pub mod ty;
pub mod unify;
```

`src/lib.rs` — добавить `pub mod types;` (в алфавитном порядке после `sexpr`).

- [ ] **Step 2: Падающие тесты**

В конец `src/types/ty.rs`:
```rust
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
}
```

В конец `src/types/unify.rs`:
```rust
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
```

- [ ] **Step 3: Запустить**

Run: `cargo test types`
Expected: FAIL — не компилируется.

- [ ] **Step 4: Реализация `ty.rs`** — начало `src/types/ty.rs`:

```rust
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
}

/// Полиморфный тип: `vars` квантифицированы.
#[derive(Debug, Clone, PartialEq)]
pub struct Scheme {
    pub vars: Vec<TVar>,
    pub ty: Type,
}

impl Scheme {
    pub fn mono(ty: Type) -> Scheme {
        Scheme { vars: Vec::new(), ty }
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
            if !args.is_empty() {
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
```

- [ ] **Step 5: Реализация `unify.rs`** — начало `src/types/unify.rs`:

```rust
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
```

- [ ] **Step 6: Запустить тесты**

Run: `cargo test types`
Expected: PASS (7 тестов).

- [ ] **Step 7: Commit**

```bash
git add src/lib.rs src/types/
git commit -m "feat(types): type representation, display and level-based unification"
```

---

### Task 7: Анализ зависимостей функций (SCC)

**Files:**
- Create: `src/types/deps.rs`
- Modify: `src/types/mod.rs`

**Interfaces:**
- Consumes: `ast::{Expr, ExprKind, Stmt}`.
- Produces: `types::deps::referenced_names(e: &Expr, out: &mut HashSet<String>)`; `types::deps::scc(edges: &[Vec<usize>]) -> Vec<Vec<usize>>` — компоненты в порядке «зависимости раньше зависящих», индексы внутри компоненты отсортированы.

- [ ] **Step 1: Подключить** — в `src/types/mod.rs` добавить `pub mod deps;` (первой строкой).

- [ ] **Step 2: Падающие тесты** — конец `src/types/deps.rs`:

```rust
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
        assert_eq!(names, vec!["f", "g", "x", "y"]);
    }

    #[test]
    fn chain_is_ordered_dependencies_first() {
        // 0 зависит от 1, 1 от 2
        assert_eq!(scc(&[vec![1], vec![2], vec![]]), vec![vec![2], vec![1], vec![0]]);
    }

    #[test]
    fn mutual_recursion_is_one_component() {
        assert_eq!(scc(&[vec![1], vec![0], vec![]]), vec![vec![0, 1], vec![2]]);
        assert_eq!(scc(&[vec![0]]), vec![vec![0]]);
    }
}
```

- [ ] **Step 3: Запустить**

Run: `cargo test deps`
Expected: FAIL — не компилируется.

- [ ] **Step 4: Реализация** — начало `src/types/deps.rs`:

```rust
use std::collections::HashSet;

use crate::ast::{Expr, ExprKind, Stmt};

/// Все имена переменных, упомянутые в выражении. Затенение не учитывается:
/// лишние рёбра только укрупняют компоненты и не ломают вывод.
pub fn referenced_names(e: &Expr, out: &mut HashSet<String>) {
    match &e.kind {
        ExprKind::Var(name) => {
            out.insert(name.clone());
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::ModuleAccess { .. } => {}
        ExprKind::Unary { expr, .. } => referenced_names(expr, out),
        ExprKind::Binary { lhs, rhs, .. } => {
            referenced_names(lhs, out);
            referenced_names(rhs, out);
        }
        ExprKind::Call { callee, args } => {
            referenced_names(callee, out);
            for a in args {
                referenced_names(a, out);
            }
        }
        ExprKind::Lambda { body, .. } => referenced_names(body, out),
        ExprKind::If { cond, then_branch, else_branch } => {
            referenced_names(cond, out);
            referenced_names(then_branch, out);
            if let Some(e) = else_branch {
                referenced_names(e, out);
            }
        }
        ExprKind::Block(stmts) => {
            for s in stmts {
                match s {
                    Stmt::Let { value, .. } => referenced_names(value, out),
                    Stmt::Expr(e) => referenced_names(e, out),
                }
            }
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
```

- [ ] **Step 5: Запустить тесты**

Run: `cargo test deps`
Expected: PASS (3 теста).

- [ ] **Step 6: Commit**

```bash
git add src/types/mod.rs src/types/deps.rs
git commit -m "feat(types): dependency analysis and Tarjan SCC"
```

---

### Task 8: Вывод типов выражений

**Files:**
- Create: `src/types/env.rs`, `src/types/infer.rs`
- Modify: `src/types/mod.rs`

**Interfaces:**
- Consumes: `Subst`, `Type`, `Scheme`, `display` (Task 6); AST (Task 4).
- Produces:
  - `types::env::Env { values: HashMap<String, Scheme>, modules: HashMap<String, HashMap<String, Scheme>> }` (Clone, Default, Debug). Все схемы в `Env` замкнуты (без свободных переменных).
  - `types::infer::type_of_expr(expr: &ast::Expr, env: &Env) -> Result<Scheme, Vec<Diag>>`.
  - Внутренний `Checker` (используется в Task 9): поля `env`, `subst`, `level`, `locals`, `diags`; методы `infer`, `convert`, `annotation`, `expect`.
- Тексты ошибок (на них опираются тесты и следующие задачи):
  - `несовпадение типов: ожидался {E}, а здесь {A}`
  - подсказка `для Float используй {op}.` при `Int`-операторе с `Float`-операндом
  - подсказка `у if без else ветка должна возвращать Unit — добавь else`
  - `неизвестное имя `{x}``, `неизвестный модуль `{m}``, `в модуле `{m}` нет `{x}``
  - `функция принимает аргументов: {n}, а передано: {m}`
  - `это не функция, а значение типа {T}`
  - `бесконечный тип: значение используется как часть самого себя`
  - `неизвестный тип `{T}``, `тип `{T}` не принимает параметров`

- [ ] **Step 1: Модули**

`src/types/mod.rs`:
```rust
pub mod deps;
pub mod env;
pub mod infer;
pub mod ty;
pub mod unify;
```

`src/types/env.rs`:
```rust
use std::collections::HashMap;

use super::ty::Scheme;

/// Окружение типов: значения (prelude и текущий модуль) и модули stdlib.
/// Все схемы замкнуты — не содержат свободных переменных.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub values: HashMap<String, Scheme>,
    pub modules: HashMap<String, HashMap<String, Scheme>>,
}
```

- [ ] **Step 2: Падающие тесты** — конец `src/types/infer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::parse_expr;

    fn expr(src: &str) -> Expr {
        parse_expr(lex(src).unwrap()).unwrap()
    }

    fn t(src: &str) -> String {
        display(&type_of_expr(&expr(src), &Env::default()).unwrap().ty)
    }

    fn errs(src: &str) -> Vec<Diag> {
        type_of_expr(&expr(src), &Env::default()).unwrap_err()
    }

    fn err(src: &str) -> String {
        errs(src)[0].message.clone()
    }

    #[test]
    fn literals_and_operators() {
        assert_eq!(t("1 + 2"), "Int");
        assert_eq!(t("1.5 *. 2.0"), "Float");
        assert_eq!(t("\"a\" <> \"b\""), "String");
        assert_eq!(t("1 < 2 && true"), "Bool");
        assert_eq!(t("{}"), "Unit");
    }

    #[test]
    fn polymorphic_functions() {
        assert_eq!(t("fn(x) { x }"), "fn(a) -> a");
        assert_eq!(t("fn(f, x) { f(f(x)) }"), "fn(fn(a) -> a, a) -> a");
        assert_eq!(t("fn(x, y) { x == y }"), "fn(a, a) -> Bool");
    }

    #[test]
    fn let_polymorphism() {
        assert_eq!(t("{\n let id = fn(x) { x }\n let a = id(1)\n id(\"s\")\n}"), "String");
    }

    #[test]
    fn lambda_params_are_monomorphic() {
        assert!(err("fn(f) {\n let a = f(1)\n f(\"s\")\n}").contains("несовпадение типов"));
    }

    #[test]
    fn if_expressions() {
        assert_eq!(t("if true { 1 } else { 2 }"), "Int");
        assert!(err("if 1 { 2 } else { 3 }").contains("ожидался Bool, а здесь Int"));
        let d = &errs("if true { 1 }")[0];
        assert!(d.message.contains("ожидался Unit, а здесь Int"));
        assert!(d.hint.as_deref().unwrap().contains("добавь else"));
    }

    #[test]
    fn float_operator_hint() {
        let d = &errs("1 + 2.5")[0];
        assert!(d.message.contains("ожидался Int, а здесь Float"));
        assert_eq!(d.hint.as_deref(), Some("для Float используй +."));
    }

    #[test]
    fn name_and_call_errors() {
        assert!(err("foo").contains("неизвестное имя `foo`"));
        assert!(err("(fn(x) { x })(1, 2)").contains("передано: 2"));
        assert!(err("1(2)").contains("это не функция, а значение типа Int"));
        assert!(err("fn(x) { x(x) }").contains("бесконечный тип"));
    }

    #[test]
    fn annotations() {
        assert_eq!(t("fn(x: Int) { x }"), "fn(Int) -> Int");
        assert!(err("fn(x: Strin) { x }").contains("неизвестный тип `Strin`"));
        assert!(err("{\n let x: Int = \"s\"\n x\n}").contains("несовпадение типов"));
    }
}
```

- [ ] **Step 3: Запустить**

Run: `cargo test infer`
Expected: FAIL — не компилируется.

- [ ] **Step 4: Реализация** — начало `src/types/infer.rs`:

```rust
use std::collections::HashMap;

use crate::ast::{BinOp, Expr, ExprKind, Stmt, TypeExpr, UnOp};
use crate::diagnostics::{Diag, Span};

use super::env::Env;
use super::ty::{display, Scheme, Type};
use super::unify::{Subst, UnifyError};

/// Выводит обобщённый тип одного выражения (тесты, `:t` в REPL).
pub fn type_of_expr(expr: &Expr, env: &Env) -> Result<Scheme, Vec<Diag>> {
    let mut c = Checker::new(env);
    c.level = 1;
    let ty = c.infer(expr);
    c.level = 0;
    let scheme = c.subst.generalize(&ty, 0);
    c.finish(scheme)
}

struct Checker {
    env: Env,
    subst: Subst,
    level: u32,
    locals: Vec<(String, Scheme)>,
    diags: Vec<Diag>,
}

impl Checker {
    fn new(env: &Env) -> Self {
        Checker {
            env: env.clone(),
            subst: Subst::new(),
            level: 0,
            locals: Vec::new(),
            diags: Vec::new(),
        }
    }

    fn finish<T>(self, value: T) -> Result<T, Vec<Diag>> {
        if self.diags.is_empty() {
            Ok(value)
        } else {
            Err(self.diags)
        }
    }

    fn infer(&mut self, e: &Expr) -> Type {
        match &e.kind {
            ExprKind::Int(_) => Type::int(),
            ExprKind::Float(_) => Type::float(),
            ExprKind::Str(_) => Type::string(),
            ExprKind::Bool(_) => Type::bool(),
            ExprKind::Var(name) => self.lookup(name, e.span),
            ExprKind::ModuleAccess { module, name } => self.module_access(module, name, e.span),
            ExprKind::Unary { op, expr } => {
                let expected = match op {
                    UnOp::Neg => Type::int(),
                    UnOp::NegF => Type::float(),
                    UnOp::Not => Type::bool(),
                };
                let actual = self.infer(expr);
                self.expect(expr.span, &actual, &expected);
                expected
            }
            ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs),
            ExprKind::Call { callee, args } => self.call(callee, args, e.span),
            ExprKind::Lambda { params, body } => {
                let base = self.locals.len();
                let mut vars = HashMap::new();
                let tys: Vec<Type> = params
                    .iter()
                    .map(|p| self.annotation(p.ty.as_ref(), &mut vars))
                    .collect();
                for (p, t) in params.iter().zip(&tys) {
                    self.locals.push((p.name.clone(), Scheme::mono(t.clone())));
                }
                let ret = self.infer(body);
                self.locals.truncate(base);
                Type::Fn(tys, Box::new(ret))
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                let c = self.infer(cond);
                self.expect(cond.span, &c, &Type::bool());
                let then_ty = self.infer(then_branch);
                match else_branch {
                    Some(else_branch) => {
                        let else_ty = self.infer(else_branch);
                        self.expect(else_branch.span, &else_ty, &then_ty);
                        then_ty
                    }
                    None => {
                        if !self.expect(then_branch.span, &then_ty, &Type::unit()) {
                            self.hint_last("у if без else ветка должна возвращать Unit — добавь else");
                        }
                        Type::unit()
                    }
                }
            }
            ExprKind::Block(stmts) => self.block(stmts),
        }
    }

    fn block(&mut self, stmts: &[Stmt]) -> Type {
        let base = self.locals.len();
        let mut last = Type::unit();
        for stmt in stmts {
            last = match stmt {
                Stmt::Let { name, ty, value, .. } => {
                    self.level += 1;
                    let actual = self.infer(value);
                    if let Some(t) = ty {
                        let expected = self.convert(t, &mut HashMap::new());
                        self.expect(value.span, &actual, &expected);
                    }
                    self.level -= 1;
                    let scheme = self.subst.generalize(&actual, self.level);
                    self.locals.push((name.clone(), scheme));
                    Type::unit()
                }
                Stmt::Expr(e) => self.infer(e),
            };
        }
        self.locals.truncate(base);
        last
    }

    fn lookup(&mut self, name: &str, span: Span) -> Type {
        let scheme = self
            .locals
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s.clone())
            .or_else(|| self.env.values.get(name).cloned());
        match scheme {
            Some(s) => self.subst.instantiate(&s, self.level),
            None => {
                self.diags.push(Diag::error(span, format!("неизвестное имя `{name}`")));
                self.subst.fresh(self.level)
            }
        }
    }

    fn module_access(&mut self, module: &str, name: &str, span: Span) -> Type {
        let scheme = match self.env.modules.get(module) {
            None => Err(format!("неизвестный модуль `{module}`")),
            Some(m) => m
                .get(name)
                .cloned()
                .ok_or_else(|| format!("в модуле `{module}` нет `{name}`")),
        };
        match scheme {
            Ok(s) => self.subst.instantiate(&s, self.level),
            Err(msg) => {
                self.diags.push(Diag::error(span, msg));
                self.subst.fresh(self.level)
            }
        }
    }

    fn binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Type {
        use BinOp::*;
        let l = self.infer(lhs);
        let r = self.infer(rhs);
        let (operand, result) = match op {
            Add | Sub | Mul | Div | Rem => (Type::int(), Type::int()),
            AddF | SubF | MulF | DivF => (Type::float(), Type::float()),
            Lt | Gt | Le | Ge => (Type::int(), Type::bool()),
            LtF | GtF | LeF | GeF => (Type::float(), Type::bool()),
            And | Or => (Type::bool(), Type::bool()),
            Concat => (Type::string(), Type::string()),
            Eq | Ne => {
                self.expect(rhs.span, &r, &l);
                return Type::bool();
            }
        };
        for (side, t) in [(lhs, &l), (rhs, &r)] {
            if !self.expect(side.span, t, &operand)
                && operand == Type::int()
                && self.subst.zonk(t) == Type::float()
            {
                self.hint_last(&format!("для Float используй {}.", op.symbol()));
            }
        }
        result
    }

    fn call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Type {
        let f = self.infer(callee);
        let arg_tys: Vec<Type> = args.iter().map(|a| self.infer(a)).collect();
        match self.subst.shallow(&f) {
            Type::Fn(params, ret) => {
                if params.len() != args.len() {
                    self.diags.push(Diag::error(
                        span,
                        format!(
                            "функция принимает аргументов: {}, а передано: {}",
                            params.len(),
                            args.len()
                        ),
                    ));
                    return *ret;
                }
                for ((arg, actual), expected) in args.iter().zip(&arg_tys).zip(&params) {
                    self.expect(arg.span, actual, expected);
                }
                *ret
            }
            Type::Var(_) => {
                let ret = self.subst.fresh(self.level);
                let expected = Type::Fn(arg_tys, Box::new(ret.clone()));
                self.expect(callee.span, &f, &expected);
                ret
            }
            other => {
                let shown = display(&self.subst.zonk(&other));
                self.diags.push(Diag::error(
                    callee.span,
                    format!("это не функция, а значение типа {shown}"),
                ));
                self.subst.fresh(self.level)
            }
        }
    }

    /// Тип из аннотации или свежая переменная, если аннотации нет.
    fn annotation(&mut self, t: Option<&TypeExpr>, vars: &mut HashMap<String, Type>) -> Type {
        match t {
            Some(t) => self.convert(t, vars),
            None => self.subst.fresh(self.level),
        }
    }

    /// Аннотация → тип. `vars` — переменные типов, видимые в объявлении.
    fn convert(&mut self, t: &TypeExpr, vars: &mut HashMap<String, Type>) -> Type {
        match t {
            TypeExpr::Named { name, args, span } => {
                if !matches!(name.as_str(), "Int" | "Float" | "Bool" | "String" | "Unit") {
                    self.diags.push(Diag::error(*span, format!("неизвестный тип `{name}`")));
                    return self.subst.fresh(self.level);
                }
                if !args.is_empty() {
                    self.diags.push(Diag::error(*span, format!("тип `{name}` не принимает параметров")));
                }
                Type::con(name)
            }
            TypeExpr::Var { name, .. } => {
                let level = self.level;
                vars.entry(name.clone())
                    .or_insert_with(|| self.subst.fresh(level))
                    .clone()
            }
            TypeExpr::Fn { params, ret, .. } => {
                let params = params.iter().map(|p| self.convert(p, vars)).collect();
                let ret = self.convert(ret, vars);
                Type::Fn(params, Box::new(ret))
            }
        }
    }

    /// Унифицирует и записывает ошибку при несовпадении. Возвращает `true`, если типы совместимы.
    fn expect(&mut self, span: Span, actual: &Type, expected: &Type) -> bool {
        match self.subst.unify(actual, expected) {
            Ok(()) => true,
            Err(UnifyError::Occurs) => {
                self.diags.push(Diag::error(
                    span,
                    "бесконечный тип: значение используется как часть самого себя",
                ));
                false
            }
            Err(UnifyError::Mismatch) => {
                let expected = display(&self.subst.zonk(expected));
                let actual = display(&self.subst.zonk(actual));
                self.diags.push(Diag::error(
                    span,
                    format!("несовпадение типов: ожидался {expected}, а здесь {actual}"),
                ));
                false
            }
        }
    }

    fn hint_last(&mut self, hint: &str) {
        if let Some(d) = self.diags.last_mut() {
            d.hint = Some(hint.to_string());
        }
    }
}
```

- [ ] **Step 5: Запустить тесты**

Run: `cargo test infer`
Expected: PASS (8 тестов).

- [ ] **Step 6: Commit**

```bash
git add src/types/mod.rs src/types/env.rs src/types/infer.rs
git commit -m "feat(types): Hindley-Milner inference for expressions with let-polymorphism"
```

---

### Task 9: Проверка модуля: внешние функции, SCC, аннотации

**Files:**
- Create: `src/typed.rs`
- Modify: `src/types/infer.rs`, `src/lib.rs`

**Interfaces:**
- Consumes: `Checker` (Task 8), `deps::{referenced_names, scc}` (Task 7).
- Produces:
  - `typed::Module { ast: ast::Module, globals: Vec<(String, Scheme)> }` (Debug) — `globals` в порядке объявления, схемы замкнуты.
  - `types::infer::check_module(module: ast::Module, env: &Env) -> Result<typed::Module, Vec<Diag>>`. Глобалы модуля затеняют одноимённые значения из `env.values`.
  - Текст ошибки: ``{name}` уже объявлено в этом модуле`.

- [ ] **Step 1: typed.rs** — `src/typed.rs`:

```rust
use crate::ast;
use crate::types::ty::Scheme;

/// Результат проверки типов — единственный вход бэкенда.
#[derive(Debug)]
pub struct Module {
    pub ast: ast::Module,
    /// Схемы функций верхнего уровня в порядке объявления.
    pub globals: Vec<(String, Scheme)>,
}
```

`src/lib.rs` — добавить `pub mod typed;` (после `sexpr`, перед `types`).

- [ ] **Step 2: Падающие тесты** — добавить в `mod tests` в `src/types/infer.rs`:

```rust
    use crate::parser::parse;
    use crate::typed;

    fn check_src(src: &str, env: &Env) -> Result<typed::Module, Vec<Diag>> {
        check_module(parse(lex(src).unwrap()).unwrap(), env)
    }

    fn sig(m: &typed::Module, name: &str) -> String {
        display(&m.globals.iter().find(|(n, _)| n == name).unwrap().1.ty)
    }

    #[test]
    fn declaration_order_does_not_matter() {
        let m = check_src(
            "fn main() {\n  let a = id(1)\n  id(\"s\")\n}\nfn id(x) { x }",
            &Env::default(),
        )
        .unwrap();
        assert_eq!(sig(&m, "id"), "fn(a) -> a");
        assert_eq!(sig(&m, "main"), "fn() -> String");
        let names: Vec<&str> = m.globals.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["main", "id"]);
    }

    #[test]
    fn mutual_recursion() {
        let m = check_src(
            "fn is_even(n) { if n == 0 { true } else { is_odd(n - 1) } }\n\
             fn is_odd(n) { if n == 0 { false } else { is_even(n - 1) } }",
            &Env::default(),
        )
        .unwrap();
        assert_eq!(sig(&m, "is_even"), "fn(Int) -> Bool");
        assert_eq!(sig(&m, "is_odd"), "fn(Int) -> Bool");
    }

    #[test]
    fn externals() {
        let m = check_src(
            "@external(\"Math.max\")\nfn max(a: Int, b: Int) -> Int\n\
             @external(\"x => x\")\nfn ident(x: a) -> a\n\
             fn f() { max(1, ident(2)) }",
            &Env::default(),
        )
        .unwrap();
        assert_eq!(sig(&m, "ident"), "fn(a) -> a");
        assert_eq!(sig(&m, "f"), "fn() -> Int");
    }

    #[test]
    fn return_annotation_is_checked() {
        let errs = check_src("fn f() -> Int { \"s\" }", &Env::default()).unwrap_err();
        assert!(errs[0].message.contains("ожидался Int, а здесь String"));
    }

    #[test]
    fn duplicate_declarations() {
        let errs = check_src("fn a() { 1 }\nfn a() { 2 }", &Env::default()).unwrap_err();
        assert!(errs[0].message.contains("уже объявлено"));
    }

    #[test]
    fn module_access() {
        let mut env = Env::default();
        let to_string = Scheme::mono(Type::Fn(vec![Type::int()], Box::new(Type::string())));
        env.modules.entry("int".to_string()).or_default().insert("to_string".to_string(), to_string);
        let m = check_src("fn f() { int.to_string(1) }", &env).unwrap();
        assert_eq!(sig(&m, "f"), "fn() -> String");
        let errs = check_src("fn f() { int.nope(1) }", &env).unwrap_err();
        assert!(errs[0].message.contains("нет `nope`"));
        let errs = check_src("fn f() { nope.x() }", &env).unwrap_err();
        assert!(errs[0].message.contains("неизвестный модуль `nope`"));
    }

    #[test]
    fn module_globals_shadow_env() {
        let mut env = Env::default();
        env.values.insert("print".to_string(), Scheme::mono(Type::Fn(vec![Type::string()], Box::new(Type::unit()))));
        let m = check_src("fn print(x) { x }\nfn f() { print(1) }", &env).unwrap();
        assert_eq!(sig(&m, "f"), "fn() -> Int");
    }
```

- [ ] **Step 3: Запустить**

Run: `cargo test infer`
Expected: FAIL — `cannot find function check_module`.

- [ ] **Step 4: Реализация** — в `src/types/infer.rs`:

Заменить импорты на:
```rust
use std::collections::{HashMap, HashSet};

use crate::ast::{self, BinOp, Expr, ExprKind, Item, Stmt, TypeExpr, UnOp};
use crate::diagnostics::{Diag, Span};
use crate::typed;

use super::deps::{referenced_names, scc};
use super::env::Env;
use super::ty::{display, Scheme, Type};
use super::unify::{Subst, UnifyError};
```

Добавить после `type_of_expr`:
```rust
/// Проверяет модуль. Функции проверяются группами взаимной рекурсии (SCC),
/// зависимости раньше зависящих; внутри группы типы мономорфны, после — обобщаются.
pub fn check_module(module: ast::Module, env: &Env) -> Result<typed::Module, Vec<Diag>> {
    let mut c = Checker::new(env);
    let mut seen = HashSet::new();
    for item in &module.items {
        let span = match item {
            Item::Fn(f) => f.span,
            Item::External(e) => e.span,
        };
        if !seen.insert(item.name()) {
            c.diags.push(Diag::error(span, format!("`{}` уже объявлено в этом модуле", item.name())));
        }
    }
    for item in &module.items {
        if let Item::External(e) = item {
            let scheme = c.external(e);
            c.env.values.insert(e.name.clone(), scheme);
        }
    }
    let fns: Vec<&ast::FnDecl> = module
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Fn(f) => Some(f),
            Item::External(_) => None,
        })
        .collect();
    let index: HashMap<&str, usize> = fns.iter().enumerate().map(|(i, f)| (f.name.as_str(), i)).collect();
    let edges: Vec<Vec<usize>> = fns
        .iter()
        .map(|f| {
            let mut names = HashSet::new();
            referenced_names(&f.body, &mut names);
            names.iter().filter_map(|n| index.get(n.as_str()).copied()).collect()
        })
        .collect();
    for group in scc(&edges) {
        let group: Vec<&ast::FnDecl> = group.iter().map(|&i| fns[i]).collect();
        c.check_group(&group);
    }
    let globals = module
        .items
        .iter()
        .map(|i| (i.name().to_string(), c.env.values[i.name()].clone()))
        .collect();
    c.finish(typed::Module { ast: module, globals })
}
```

Добавить в `impl Checker`:
```rust
    fn external(&mut self, e: &ast::ExternalDecl) -> Scheme {
        self.level = 1;
        let mut vars = HashMap::new();
        let params: Vec<Type> = e
            .params
            .iter()
            .map(|p| self.convert(p.ty.as_ref().expect("парсер требует типы у @external"), &mut vars))
            .collect();
        let ret = self.convert(&e.ret, &mut vars);
        self.level = 0;
        self.subst.generalize(&Type::Fn(params, Box::new(ret)), 0)
    }

    fn check_group(&mut self, group: &[&ast::FnDecl]) {
        self.level = 1;
        let mut sigs = Vec::new();
        for f in group {
            let mut vars = HashMap::new();
            let params: Vec<Type> = f
                .params
                .iter()
                .map(|p| self.annotation(p.ty.as_ref(), &mut vars))
                .collect();
            let ret = self.annotation(f.ret.as_ref(), &mut vars);
            let ty = Type::Fn(params.clone(), Box::new(ret.clone()));
            self.env.values.insert(f.name.clone(), Scheme::mono(ty.clone()));
            sigs.push((params, ret, ty));
        }
        for (f, (params, ret, _)) in group.iter().zip(&sigs) {
            let base = self.locals.len();
            for (p, ty) in f.params.iter().zip(params) {
                self.locals.push((p.name.clone(), Scheme::mono(ty.clone())));
            }
            let body = self.infer(&f.body);
            self.expect(f.body.span, &body, ret);
            self.locals.truncate(base);
        }
        self.level = 0;
        for (f, (_, _, ty)) in group.iter().zip(&sigs) {
            let scheme = self.subst.generalize(ty, 0);
            self.env.values.insert(f.name.clone(), scheme);
        }
    }
```

- [ ] **Step 5: Запустить тесты**

Run: `cargo test infer`
Expected: PASS (15 тестов).

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/typed.rs src/types/infer.rs
git commit -m "feat(types): module checking with externals, SCC groups and annotations"
```

---

### Task 10: Драйвер и stdlib

**Files:**
- Create: `src/driver.rs`, `stdlib/prelude.lumo`, `stdlib/int.lumo`
- Modify: `src/lib.rs`

**Interfaces:**
- Consumes: `lexer::lex`, `parser::parse`, `types::infer::check_module`, `Env`, `typed::Module`, `diagnostics::render`.
- Produces:
  - `driver::Failure { file: String, src: String, diags: Vec<Diag> }` (Debug) с `render(&self, color: bool) -> String`.
  - `driver::Stdlib { env: Env, modules: Vec<(&'static str, typed::Module)> }` и `driver::load_stdlib() -> Stdlib`. Имя модуля `"prelude"` — его функции видны без префикса; остальные — как `модуль.имя`.
  - `driver::check(file: &str, src: &str) -> Result<typed::Module, Failure>` — включает проверку `main` (есть, без параметров).
  - Тексты: `в программе нет функции main`, `main не должна принимать параметры`.

- [ ] **Step 1: stdlib**

`stdlib/prelude.lumo`:
```lumo
// Функции, доступные без префикса.

@external("$print")
fn print(s: String) -> Unit
```

`stdlib/int.lumo`:
```lumo
// Модуль int.

@external("String")
fn to_string(n: Int) -> String
```

`src/lib.rs` — добавить `pub mod driver;` (после `diagnostics`).

- [ ] **Step 2: Падающие тесты** — конец `src/driver.rs`:

```rust
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
```

- [ ] **Step 3: Запустить**

Run: `cargo test driver`
Expected: FAIL — не компилируется.

- [ ] **Step 4: Реализация** — начало `src/driver.rs`:

```rust
use crate::ast;
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
```

- [ ] **Step 5: Запустить тесты**

Run: `cargo test driver`
Expected: PASS (4 теста).

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/driver.rs stdlib/
git commit -m "feat(driver): stdlib loading and program checking"
```

---

### Task 11: Рантайм, генерация JS и golden-тесты

**Files:**
- Create: `runtime/lumo.js`, `src/codegen.rs`, `tests/golden.rs`, `tests/programs/{hello,fib,closures,mutual,operators,shadowing}.lumo` и `.out`
- Modify: `src/driver.rs`, `src/lib.rs`

**Interfaces:**
- Consumes: `typed::Module`, AST, `driver::{load_stdlib, check_with}`.
- Produces:
  - `codegen::emit_program(units: &[(&str, &typed::Module)]) -> String` — рантайм + модули + `main$main();`. Модуль программы передаётся с префиксом `main`.
  - `driver::compile(file: &str, src: &str) -> Result<String, Failure>`.
  - Схема имён в JS: глобал модуля с префиксом `p` → `p$имя`; глобал программы → `main$имя` (так имена программы не затеняют глобалы JS); локальные — уникальны в пределах функции верхнего уровня (`x`, `x$1`, …) и не совпадают с глобалами программы.
  - Рантайм: `$idiv`, `$imod`, `$eq`, `$print`.

- [ ] **Step 1: Golden-тесты (падающие)**

`tests/golden.rs`:
```rust
//! Сквозные тесты: tests/programs/*.lumo компилируется, запускается в node,
//! stdout сравнивается с *.out.

use std::path::Path;
use std::process::Command;
use std::{env, fs, process};

#[test]
fn golden_programs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let mut programs: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map_or(false, |e| e == "lumo"))
        .collect();
    programs.sort();
    assert!(!programs.is_empty(), "нет программ в {}", dir.display());

    let mut failures = Vec::new();
    for path in programs {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let src = fs::read_to_string(&path).unwrap();
        let expected = fs::read_to_string(path.with_extension("out")).unwrap();
        let js = match lumo::driver::compile(&format!("{name}.lumo"), &src) {
            Ok(js) => js,
            Err(f) => {
                failures.push(format!("{name}: ошибка компиляции\n{}", f.render(false)));
                continue;
            }
        };
        let js_path = env::temp_dir().join(format!("lumo-golden-{}-{name}.js", process::id()));
        fs::write(&js_path, js).unwrap();
        let output = Command::new("node").arg(&js_path).output().expect("нужен node в PATH");
        let _ = fs::remove_file(&js_path);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stdout != expected {
            failures.push(format!(
                "{name}:\n--- ожидалось\n{expected}--- получено\n{stdout}--- stderr\n{stderr}"
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
```

`tests/programs/hello.lumo`:
```lumo
fn main() {
  print("Привет, Lumo!")
}
```
`tests/programs/hello.out`:
```
Привет, Lumo!
```

`tests/programs/fib.lumo`:
```lumo
// Рекурсия и if-выражения
fn fib(n) {
  if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}

fn main() {
  print(int.to_string(fib(20)))
}
```
`tests/programs/fib.out`:
```
6765
```

`tests/programs/closures.lumo`:
```lumo
fn compose(f, g) {
  fn(x) { g(f(x)) }
}

fn make_adder(n) {
  fn(x) { x + n }
}

fn add(a, b) {
  a + b
}

fn main() {
  let add2 = make_adder(2)
  let twice = compose(add2, add2)
  print(int.to_string(twice(10)))
  let id = fn(x) { x }
  print(id("полиморфизм"))
  print(int.to_string(id(42)))
  10
  |> add(5)
  |> int.to_string
  |> print
}
```
`tests/programs/closures.out`:
```
14
полиморфизм
42
15
```

`tests/programs/mutual.lumo`:
```lumo
fn main() {
  print(describe(10))
  print(describe(7))
  print(first("порядок объявления не важен", 0))
}

fn describe(n) {
  if is_even(n) { "чётное" } else { "нечётное" }
}

fn is_even(n) {
  if n == 0 { true } else { is_odd(n - 1) }
}

fn is_odd(n) {
  if n == 0 { false } else { is_even(n - 1) }
}

fn first(a, b) {
  a
}
```
`tests/programs/mutual.out`:
```
чётное
нечётное
порядок объявления не важен
```

`tests/programs/operators.lumo`:
```lumo
fn show(n) {
  print(int.to_string(n))
}

fn main() {
  show(7 / 2)
  show(-7 / 2)
  show(7 % 3)
  show(5 / 0)
  show(2 + 3 * 4)
  show((2 + 3) * 4)
  print("Lu" <> "mo")
  print(if 1.5 +. 1.5 >. 2.9 { "float ок" } else { "float сломан" })
  print(if 3 == 3 && "a" != "b" { "сравнения ок" } else { "сравнения сломаны" })
  print(if !(1 > 2) || false { "логика ок" } else { "логика сломана" })
}
```
`tests/programs/operators.out`:
```
3
-3
1
0
14
20
Lumo
float ок
сравнения ок
логика ок
```

`tests/programs/shadowing.lumo`:
```lumo
fn main() {
  let x = 1
  let x = x + 1
  let f = fn(x) { x * 10 }
  print(int.to_string(f(x)))
  let print_twice = fn(s) {
    print(s)
    print(s)
  }
  print_twice("эхо")
  let class = "зарезервированное слово JS"
  print(class)
}
```
`tests/programs/shadowing.out`:
```
20
эхо
эхо
зарезервированное слово JS
```

Каждый `.out` заканчивается переводом строки.

- [ ] **Step 2: Запустить**

Run: `cargo test --test golden`
Expected: FAIL — `cannot find function compile in module driver`.

- [ ] **Step 3: Рантайм** — `runtime/lumo.js`:

```js
"use strict";
// Рантайм Lumo. Только стандартный JS и console — без API Node.

function $idiv(a, b) {
  return b === 0 ? 0 : Math.trunc(a / b);
}

function $imod(a, b) {
  return b === 0 ? 0 : a % b;
}

function $eq(a, b) {
  if (a === b) return true;
  if (typeof a === "function" || typeof b === "function") {
    throw new Error("Lumo: функции нельзя сравнивать через ==");
  }
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
    return false;
  }
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  if (ka.length !== kb.length) return false;
  for (const k of ka) {
    if (!$eq(a[k], b[k])) return false;
  }
  return true;
}

function $print(s) {
  console.log(s);
}
```

- [ ] **Step 4: Генератор** — `src/codegen.rs`:

```rust
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use crate::ast::{BinOp, Expr, ExprKind, ExternalDecl, FnDecl, Item, Stmt, UnOp};
use crate::typed;

const RUNTIME: &str = include_str!("../runtime/lumo.js");

const RESERVED: &[&str] = &[
    "arguments", "await", "break", "case", "catch", "class", "console", "const", "continue",
    "debugger", "default", "delete", "do", "document", "else", "enum", "eval", "export",
    "exports", "extends", "false", "finally", "for", "function", "globalThis", "if",
    "implements", "import", "in", "instanceof", "interface", "let", "module", "new", "null",
    "package", "private", "process", "protected", "public", "require", "return", "static",
    "super", "switch", "this", "throw", "true", "try", "typeof", "undefined", "var", "void",
    "while", "window", "with", "yield",
];

/// Собирает программу: рантайм, модули `(префикс, модуль)` по порядку и вызов `main$main()`.
/// Модуль программы идёт с префиксом `main`, как и модули stdlib — так его имена
/// не затеняют глобалы JS.
pub fn emit_program(units: &[(&str, &typed::Module)]) -> String {
    let mut out = String::from(RUNTIME);
    for (prefix, module) in units {
        let mut gen = Gen::new(prefix, module);
        for item in &module.ast.items {
            match item {
                Item::Fn(f) => gen.function(f),
                Item::External(e) => gen.external(e),
            }
        }
        out.push_str(&gen.out);
    }
    out.push_str("main$main();\n");
    out
}

fn mangle(prefix: &str, name: &str) -> String {
    format!("{prefix}${name}")
}

fn escape(name: &str) -> String {
    if RESERVED.contains(&name) {
        format!("{name}$")
    } else {
        name.to_string()
    }
}

fn js_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn js_op(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add | AddF | Concat => "+",
        Sub | SubF => "-",
        Mul | MulF => "*",
        DivF => "/",
        Lt | LtF => "<",
        Gt | GtF => ">",
        Le | LeF => "<=",
        Ge | GeF => ">=",
        And => "&&",
        Or => "||",
        Div | Rem | Eq | Ne => unreachable!("обрабатываются через рантайм"),
    }
}

struct Gen<'a> {
    prefix: &'a str,
    globals: HashSet<&'a str>,
    /// Lumo-имя → JS-имя, по вложенным областям.
    scopes: Vec<HashMap<String, String>>,
    /// Сколько раз имя уже связано в текущей функции верхнего уровня.
    used: HashMap<String, u32>,
    out: String,
}

impl<'a> Gen<'a> {
    fn new(prefix: &'a str, module: &'a typed::Module) -> Self {
        let globals = module.globals.iter().map(|(n, _)| n.as_str()).collect();
        Gen { prefix, globals, scopes: Vec::new(), used: HashMap::new(), out: String::new() }
    }

    fn function(&mut self, f: &FnDecl) {
        self.used = self.globals.iter().map(|g| (g.to_string(), 1)).collect();
        self.scopes.push(HashMap::new());
        let params: Vec<String> = f.params.iter().map(|p| self.bind(&p.name)).collect();
        let body = self.tail(&f.body);
        self.scopes.pop();
        let name = mangle(self.prefix, &f.name);
        let _ = writeln!(self.out, "function {name}({}) {{\n{body}}}", params.join(", "));
    }

    fn external(&mut self, e: &ExternalDecl) {
        let params: Vec<String> = e.params.iter().map(|p| escape(&p.name)).collect();
        let list = params.join(", ");
        let name = mangle(self.prefix, &e.name);
        let _ = writeln!(self.out, "function {name}({list}) {{ return ({})({list}); }}", e.js);
    }

    /// Связывает локальное имя в текущей области и возвращает уникальное JS-имя.
    fn bind(&mut self, name: &str) -> String {
        let n = self.used.entry(name.to_string()).or_insert(0);
        let js = if *n == 0 { escape(name) } else { format!("{name}${n}") };
        *n += 1;
        self.scopes
            .last_mut()
            .expect("bind вызывается внутри области")
            .insert(name.to_string(), js.clone());
        js
    }

    fn var(&self, name: &str) -> String {
        for scope in self.scopes.iter().rev() {
            if let Some(js) = scope.get(name) {
                return js.clone();
            }
        }
        if self.globals.contains(name) {
            mangle(self.prefix, name)
        } else {
            mangle("prelude", name)
        }
    }

    /// Операторы JS, которые вычисляют `e` и делают `return`.
    fn tail(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Block(stmts) => {
                self.scopes.push(HashMap::new());
                let mut out = String::new();
                for (i, s) in stmts.iter().enumerate() {
                    let is_last = i + 1 == stmts.len();
                    match s {
                        Stmt::Let { name, value, .. } => {
                            let v = self.expr(value);
                            let js = self.bind(name);
                            let _ = writeln!(out, "const {js} = {v};");
                        }
                        Stmt::Expr(e) if is_last => out.push_str(&self.tail(e)),
                        Stmt::Expr(e) => {
                            let v = self.expr(e);
                            let _ = writeln!(out, "{v};");
                        }
                    }
                }
                if !matches!(stmts.last(), Some(Stmt::Expr(_))) {
                    out.push_str("return undefined;\n");
                }
                self.scopes.pop();
                out
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                let c = self.expr(cond);
                let then = self.tail(then_branch);
                match else_branch {
                    Some(else_branch) => {
                        let els = self.tail(else_branch);
                        format!("if ({c}) {{\n{then}}} else {{\n{els}}}\n")
                    }
                    None => format!("if ({c}) {{\n{then}}}\nreturn undefined;\n"),
                }
            }
            _ => format!("return {};\n", self.expr(e)),
        }
    }

    fn expr(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(n) => n.to_string(),
            ExprKind::Float(f) => format!("{f:?}"),
            ExprKind::Str(s) => js_string(s),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Var(name) => self.var(name),
            ExprKind::ModuleAccess { module, name } => mangle(module, name),
            ExprKind::Unary { op, expr } => {
                let x = self.expr(expr);
                match op {
                    UnOp::Neg | UnOp::NegF => format!("(-{x})"),
                    UnOp::Not => format!("(!{x})"),
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                match op {
                    BinOp::Div => format!("$idiv({l}, {r})"),
                    BinOp::Rem => format!("$imod({l}, {r})"),
                    BinOp::Eq => format!("$eq({l}, {r})"),
                    BinOp::Ne => format!("(!$eq({l}, {r}))"),
                    _ => format!("({l} {} {r})", js_op(*op)),
                }
            }
            ExprKind::Call { callee, args } => {
                let f = self.expr(callee);
                let args: Vec<String> = args.iter().map(|a| self.expr(a)).collect();
                format!("{f}({})", args.join(", "))
            }
            ExprKind::Lambda { params, body } => {
                self.scopes.push(HashMap::new());
                let ps: Vec<String> = params.iter().map(|p| self.bind(&p.name)).collect();
                let b = self.tail(body);
                self.scopes.pop();
                format!("(({}) => {{\n{b}}})", ps.join(", "))
            }
            ExprKind::If { cond, then_branch, else_branch } => {
                let c = self.expr(cond);
                let t = self.expr(then_branch);
                let other = match else_branch {
                    Some(e) => self.expr(e),
                    None => "undefined".to_string(),
                };
                format!("({c} ? {t} : {other})")
            }
            ExprKind::Block(stmts) => match stmts.as_slice() {
                [Stmt::Expr(only)] => self.expr(only),
                _ => format!("(() => {{\n{}}})()", self.tail(e)),
            },
        }
    }
}
```

`src/lib.rs` — добавить `pub mod codegen;` (после `ast`).

- [ ] **Step 5: `compile` в драйвере** — в `src/driver.rs` добавить импорт `use crate::codegen;` и функцию после `check`:

```rust
/// Компилирует программу в самодостаточный JS.
pub fn compile(file: &str, src: &str) -> Result<String, Failure> {
    let stdlib = load_stdlib();
    let module = check_with(file, src, &stdlib)?;
    let mut units: Vec<(&str, &typed::Module)> = stdlib.modules.iter().map(|(p, m)| (*p, m)).collect();
    units.push(("main", &module));
    Ok(codegen::emit_program(&units))
}
```

- [ ] **Step 6: Запустить все тесты**

Run: `cargo test`
Expected: PASS — все модульные тесты и `golden_programs`.

Если golden-тест падает, в сообщении есть ожидаемый и полученный вывод и stderr Node. Посмотреть сгенерированный JS можно так: временно добавить `eprintln!("{js}")` в тест или вызвать `lumo::driver::compile` из отдельного теста.

- [ ] **Step 7: Commit**

```bash
git add src/lib.rs src/codegen.rs src/driver.rs runtime/ tests/golden.rs tests/programs/
git commit -m "feat(codegen): JS generation, runtime and golden end-to-end tests"
```

---

### Task 12: CLI

**Files:**
- Create: `src/main.rs`, `tests/cli.rs`

**Interfaces:**
- Consumes: `driver::{check, compile, Failure}`.
- Produces: бинарник `lumo` с командами `check FILE`, `build FILE [-o OUT]`, `run FILE`. Коды возврата: 0 — успех; 1 — ошибка компиляции/ввода-вывода или код возврата программы в `run`; 2 — неверные аргументы. Ошибки печатаются в stderr, с цветом только если stderr — терминал.

- [ ] **Step 1: Падающие тесты** — `tests/cli.rs`:

```rust
use std::path::PathBuf;
use std::process::{self, Command};
use std::{env, fs};

fn lumo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lumo"))
}

fn write_tmp(name: &str, src: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("lumo-cli-{}", process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, src).unwrap();
    path
}

#[test]
fn run_prints_program_output() {
    let file = write_tmp("ok.lumo", "fn main() {\n  print(\"hi\")\n}\n");
    let out = lumo().args(["run", file.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
}

#[test]
fn check_reports_type_errors() {
    let file = write_tmp("bad.lumo", "fn main() {\n  print(1)\n}\n");
    let out = lumo().args(["check", file.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("ожидался String, а здесь Int"));
}

#[test]
fn build_writes_javascript() {
    let file = write_tmp("build.lumo", "fn main() {\n  print(\"hi\")\n}\n");
    let js = file.with_file_name("build-out.js");
    let out = lumo()
        .args(["build", file.to_str().unwrap(), "-o", js.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(fs::read_to_string(&js).unwrap().contains("function main$main()"));
}

#[test]
fn missing_file_is_reported() {
    let out = lumo().args(["check", "/nonexistent/x.lumo"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("не удалось прочитать"));
}

#[test]
fn usage_on_bad_arguments() {
    let out = lumo().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("использование"));
}
```

- [ ] **Step 2: Запустить**

Run: `cargo test --test cli`
Expected: FAIL — нет бинарника `lumo` (`CARGO_BIN_EXE_lumo` не определён).

- [ ] **Step 3: Реализация** — `src/main.rs`:

```rust
use std::io::IsTerminal;
use std::path::Path;
use std::process::{self, Command};
use std::{env, fs};

use lumo::driver::{self, Failure};

const USAGE: &str = "использование:
  lumo check FILE           проверить типы
  lumo build FILE [-o OUT]  собрать в JavaScript
  lumo run FILE             собрать и запустить в Node";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let code = match args.as_slice() {
        ["check", file] => check(file),
        ["build", file] => build(file, &Path::new(file).with_extension("js").to_string_lossy()),
        ["build", file, "-o", out] => build(file, out),
        ["run", file] => run(file),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    process::exit(code);
}

fn read(file: &str) -> Option<String> {
    match fs::read_to_string(file) {
        Ok(src) => Some(src),
        Err(e) => {
            eprintln!("не удалось прочитать {file}: {e}");
            None
        }
    }
}

fn report(f: &Failure) -> i32 {
    eprint!("{}", f.render(std::io::stderr().is_terminal()));
    1
}

fn check(file: &str) -> i32 {
    let Some(src) = read(file) else { return 1 };
    match driver::check(file, &src) {
        Ok(_) => 0,
        Err(f) => report(&f),
    }
}

fn build(file: &str, out: &str) -> i32 {
    let Some(src) = read(file) else { return 1 };
    match driver::compile(file, &src) {
        Ok(js) => match fs::write(out, js) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("не удалось записать {out}: {e}");
                1
            }
        },
        Err(f) => report(&f),
    }
}

fn run(file: &str) -> i32 {
    let Some(src) = read(file) else { return 1 };
    let js = match driver::compile(file, &src) {
        Ok(js) => js,
        Err(f) => return report(&f),
    };
    let path = env::temp_dir().join(format!("lumo-run-{}.js", process::id()));
    if let Err(e) = fs::write(&path, js) {
        eprintln!("не удалось записать {}: {e}", path.display());
        return 1;
    }
    let status = Command::new("node").arg(&path).status();
    let _ = fs::remove_file(&path);
    match status {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("для lumo run нужен Node.js 18+ в PATH");
            1
        }
        Err(e) => {
            eprintln!("не удалось запустить node: {e}");
            1
        }
    }
}
```

- [ ] **Step 4: Запустить все тесты**

Run: `cargo test`
Expected: PASS — все тесты, включая `cli` и `golden`.

- [ ] **Step 5: Ручная проверка**

```bash
cargo run -q -- run tests/programs/fib.lumo
```
Expected: `6765`

```bash
cargo run -q -- check tests/programs/fib.lumo; echo "код: $?"
```
Expected: `код: 0`

- [ ] **Step 6: Commit**

```bash
git add src/main.rs tests/cli.rs
git commit -m "feat(cli): lumo check, build and run"
```

---

## Готово, когда

- `cargo test` зелёный: модульные тесты всех этапов, `golden_programs`, `cli`.
- `lumo run tests/programs/fib.lumo` печатает `6765`.
- Ошибка типов в программе печатается с подсветкой строки и подсказкой (`для Float используй +.`).
- Следующий шаг — план M2 (ADT, списки, `match`, проверка полноты, хвостовая рекурсия).

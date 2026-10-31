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
        Plus | Minus
            | Star
            | Slash
            | Percent
            | PlusDot
            | MinusDot
            | StarDot
            | SlashDot
            | Lt
            | Gt
            | Le
            | Ge
            | LtDot
            | GtDot
            | LeDot
            | GeDot
            | EqEq
            | NotEq
            | AndAnd
            | OrOr
            | Pipe
            | Concat
            | Comma
            | Arrow
            | Eq
            | Dot
            | Colon
            | LParen
            | LBracket
            | LBrace
    )
}

fn continues_before(tok: &Tok) -> bool {
    use Tok::*;
    matches!(
        tok,
        Pipe | Plus
            | Star
            | Slash
            | Percent
            | PlusDot
            | StarDot
            | SlashDot
            | Concat
            | EqEq
            | NotEq
            | AndAnd
            | OrOr
            | Gt
            | Ge
            | Le
            | GtDot
            | GeDot
            | LeDot
            | LtDot
            | Dot
            | Else
    )
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
                toks.push(Token {
                    tok: Tok::Newline,
                    span: Span::new(i, i + 1),
                });
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
                    let value = src[start..i]
                        .parse()
                        .expect("цифры с точкой — валидный f64");
                    toks.push(Token {
                        tok: Tok::Float(value),
                        span: Span::new(start, i),
                    });
                } else {
                    match src[start..i].parse::<i64>() {
                        Ok(n) => toks.push(Token {
                            tok: Tok::Int(n),
                            span: Span::new(start, i),
                        }),
                        Err(_) => diags.push(Diag::error(
                            Span::new(start, i),
                            "целое число слишком большое",
                        )),
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
                toks.push(Token {
                    tok: Tok::Str(value),
                    span: Span::new(start, i),
                });
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
                toks.push(Token {
                    tok,
                    span: Span::new(start, i),
                });
            }
            _ => {
                let mut matched = false;
                for len in [3, 2, 1] {
                    if let Some(tok) = src.get(i..i + len).and_then(operator) {
                        toks.push(Token {
                            tok,
                            span: Span::new(i, i + len),
                        });
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
    toks.push(Token {
        tok: Tok::Eof,
        span: Span::new(src.len(), src.len()),
    });
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
                Fn,
                ident("add"),
                LParen,
                ident("a"),
                Comma,
                ident("b"),
                RParen,
                LBrace,
                ident("a"),
                Plus,
                ident("b"),
                RBrace,
                Eof
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            kinds("42 2.5 1..2"),
            vec![Int(42), Float(2.5), Int(1), DotDot, Int(2), Eof]
        );
    }

    #[test]
    fn operators_longest_match() {
        assert_eq!(
            kinds("+. -> <=. <= <> |> == != && || ! % <. > @ #"),
            vec![
                PlusDot, Arrow, LeDot, Le, Concat, Pipe, EqEq, NotEq, AndAnd, OrOr, Bang, Percent,
                LtDot, Gt, At, Hash, Eof
            ]
        );
    }

    #[test]
    fn strings_with_escapes_and_unicode() {
        assert_eq!(
            kinds(r#""привет\n\"мир\"""#),
            vec![Str("привет\n\"мир\"".to_string()), Eof]
        );
    }

    #[test]
    fn keywords_and_upper_names() {
        assert_eq!(
            kinds("let x = Some true false if else match type"),
            vec![
                Let,
                ident("x"),
                Eq,
                Upper("Some".to_string()),
                True,
                False,
                If,
                Else,
                Match,
                Type,
                Eof
            ]
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

    #[test]
    fn newlines_separate_statements_in_braces() {
        assert_eq!(
            kinds("{\n a\n b\n}"),
            vec![
                LBrace,
                ident("a"),
                Newline,
                ident("b"),
                Newline,
                RBrace,
                Eof
            ]
        );
    }

    #[test]
    fn leading_pipe_continues_line() {
        assert_eq!(
            kinds("x\n|> f\n|> g"),
            vec![ident("x"), Pipe, ident("f"), Pipe, ident("g"), Eof]
        );
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
        assert_eq!(
            kinds("\n\na\n\n\nb"),
            vec![ident("a"), Newline, ident("b"), Eof]
        );
    }

    #[test]
    fn minus_does_not_continue() {
        assert_eq!(
            kinds("a\n-b"),
            vec![ident("a"), Newline, Minus, ident("b"), Eof]
        );
    }

    #[test]
    fn braces_inside_parens_are_significant() {
        assert_eq!(
            kinds("f(fn() {\na\nb\n})"),
            vec![
                ident("f"),
                LParen,
                Fn,
                LParen,
                RParen,
                LBrace,
                ident("a"),
                Newline,
                ident("b"),
                Newline,
                RBrace,
                RParen,
                Eof
            ]
        );
    }
}

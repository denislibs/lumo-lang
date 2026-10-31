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

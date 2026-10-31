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
    Named {
        name: String,
        args: Vec<TypeExpr>,
        span: Span,
    },
    Var {
        name: String,
        span: Span,
    },
    Fn {
        params: Vec<TypeExpr>,
        ret: Box<TypeExpr>,
        span: Span,
    },
    /// `#(Int, String)`
    Tuple {
        elems: Vec<TypeExpr>,
        span: Span,
    },
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::Named { span, .. }
            | TypeExpr::Var { span, .. }
            | TypeExpr::Fn { span, .. }
            | TypeExpr::Tuple { span, .. } => *span,
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
    /// `[a, b]` или `[a, ..rest]`
    List {
        items: Vec<Expr>,
        tail: Option<Box<Expr>>,
    },
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Var(String),
    /// `int.to_string`
    ModuleAccess {
        module: String,
        name: String,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Lambda {
        params: Vec<Param>,
        body: Box<Expr>,
    },
    If {
        cond: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },
    /// Значение блока — последнее выражение; пустой блок или `let` в конце дают `Unit`.
    Block(Vec<Stmt>),
    /// `#(a, b)`
    Tuple(Vec<Expr>),
}

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

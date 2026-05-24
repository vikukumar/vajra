/// Vajra AST — Intermediate Abstract Syntax Tree
/// Supports multi-language syntax (Sanskrit/Hindi/English/Tamil/Arabic/Chinese/Spanish)

#[derive(Debug, Clone, PartialEq)]
pub enum VajraType {
    I64,
    F64,
    Bool,
    Str,
    Void,
    Ptr(Box<VajraType>),
    Array(Box<VajraType>, usize),
    Unknown, // resolved by type-checker
}

impl std::fmt::Display for VajraType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VajraType::I64   => write!(f, "i64"),
            VajraType::F64   => write!(f, "f64"),
            VajraType::Bool  => write!(f, "bool"),
            VajraType::Str   => write!(f, "str"),
            VajraType::Void  => write!(f, "void"),
            VajraType::Ptr(t) => write!(f, "*{}", t),
            VajraType::Array(t, n) => write!(f, "[{}; {}]", t, n),
            VajraType::Unknown => write!(f, "?"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Expression {
    Literal(Literal),
    Identifier(String),
    MethodCall {
        receiver: Box<Expression>,
        method: String,
        args: Vec<Expression>,
    },
    PropertyAccess {
        object: Box<Expression>,
        property: String,
    },
    PropertyAssign {
        object: Box<Expression>,
        property: String,
        value: Box<Expression>,
    },
    BinaryOp {
        left: Box<Expression>,
        op: String,
        right: Box<Expression>,
    },
    UnaryOp {
        op: String,
        operand: Box<Expression>,
    },
    Assign {
        name: String,
        value: Box<Expression>,
    },
    IndexAssign {
        object: Box<Expression>,
        index: Box<Expression>,
        value: Box<Expression>,
    },
    Spawn {
        task: Box<Expression>,
    },
    ObjectInstantiation {
        class_name: String,
        args: Vec<Expression>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },
    Intrinsic(Intrinsic),
    /// Index into an array: arr[idx]
    Index {
        object: Box<Expression>,
        index: Box<Expression>,
    },
    /// Cast expression: value as Type
    Cast {
        value: Box<Expression>,
        target_type: VajraType,
    },
    /// Ternary conditional operator: cond ? then_expr : else_expr
    Ternary {
        condition: Box<Expression>,
        then_expr: Box<Expression>,
        else_expr: Box<Expression>,
    },
}

#[derive(Debug, Clone)]
pub enum Intrinsic {
    Print(Vec<Expression>),
    PrintLn(Vec<Expression>),
    Alloc(Box<Expression>),         // vajra_alloc(size)
    Free(Box<Expression>),          // vajra_free(ptr)
    SysCall(Vec<Expression>),       // raw syscall(num, ...)
    Exit(Box<Expression>),          // exit(code)
    ReadLine,                        // readline()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Integer(i64),
    BigInt(String),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: VajraType,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Let {
        name: String,
        value: Expression,
        ty: VajraType,
    },
    Function {
        name: String,
        params: Vec<Param>,
        return_type: VajraType,
        body: Vec<Statement>,
        is_main: bool,
        is_extern: bool,
        is_inline: bool,
    },
    Method {
        access: AccessModifier,
        name: String,
        params: Vec<Param>,
        return_type: VajraType,
        body: Vec<Statement>,
    },
    Class {
        name: String,
        base: Option<String>,
        fields: Vec<(String, VajraType)>,
        methods: Vec<Statement>,
    },
    While {
        condition: Expression,
        body: Vec<Statement>,
    },
    For {
        var_name: String,
        iterable: Expression,
        body: Vec<Statement>,
    },
    If {
        condition: Expression,
        then_body: Vec<Statement>,
        else_body: Option<Vec<Statement>>,
    },
    TryCatch {
        try_body: Vec<Statement>,
        catch_var: String,
        catch_body: Vec<Statement>,
    },
    Throw {
        exception: Expression,
    },
    Import(String),
    Expression(Expression),
    Return(Expression),
    Break,
    Continue,
    /// Inline assembly block: asm { "mov rax, 1" }
    InlineAsm {
        code: String,
        inputs: Vec<(String, String)>,   // (constraint, identifier)
        outputs: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccessModifier {
    Public,
    Private,
    Protected,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
}

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
    BinaryOp {
        left: Box<Expression>,
        op: String,
        right: Box<Expression>,
    },
    Assign {
        name: String,
        value: Box<Expression>,
    },
    Spawn { // Lock-free concurrency primitive
        task: Box<Expression>,
    },
    ObjectInstantiation { // E.g., User(name)
        class_name: String,
        args: Vec<Expression>,
    },
    FunctionCall {
        name: String,
        args: Vec<Expression>,
    },
    Intrinsic(Intrinsic),
}

#[derive(Debug, Clone)]
pub enum Intrinsic {
    Print(Vec<Expression>),
}

#[derive(Debug, Clone)]
pub enum Literal {
    Integer(i64),
    Float(f64),
    String(String),
}

#[derive(Debug, Clone)]
pub enum Statement {
    Let {
        name: String,
        value: Expression,
    },
    Function {
        name: String,
        params: Vec<String>,
        body: Vec<Statement>,
        is_main: bool,
        is_extern: bool,
    },
    Method {
        access: AccessModifier,
        name: String,
        params: Vec<String>,
        body: Vec<Statement>,
    },
    Class {
        name: String,
        methods: Vec<Statement>, // Can be Method or Function nodes
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

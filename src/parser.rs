use crate::lexer::{Lexer, Token, TokenKind};
use crate::ast::*;

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    cur_token: Token,
    peek_token: Token,
}

impl<'a> Parser<'a> {
    pub fn new(mut lexer: Lexer<'a>) -> Self {
        let cur_token = lexer.next_token();
        let peek_token = lexer.next_token();
        Self { lexer, cur_token, peek_token }
    }

    fn next_token(&mut self) {
        self.cur_token = self.peek_token.clone();
        self.peek_token = self.lexer.next_token();
    }

    fn skip_newlines(&mut self) {
        while self.cur_token.kind == TokenKind::Newline || self.cur_token.kind == TokenKind::Semicolon {
            self.next_token();
        }
    }

    pub fn parse_program(&mut self) -> Program {
        let mut statements = Vec::new();
        while self.cur_token.kind != TokenKind::EOF {
            if let Some(stmt) = self.parse_statement() {
                statements.push(stmt);
            } else {
                self.next_token();
            }
        }
        Program { statements }
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        match &self.cur_token.kind.clone() {
            TokenKind::Newline | TokenKind::Semicolon | TokenKind::EOF => None,

            TokenKind::MainDecorator => {
                self.next_token(); // skip @main
                self.skip_newlines();
                if self.cur_token.kind == TokenKind::Function {
                    self.next_token();
                }
                self.parse_function(true, false, false)
            }

            TokenKind::ExternDecorator => {
                self.next_token(); // skip @extern
                self.skip_newlines();
                if self.cur_token.kind == TokenKind::Function {
                    self.next_token();
                }
                self.parse_extern_function()
            }

            TokenKind::InlineDecorator => {
                self.next_token(); // skip @inline
                self.skip_newlines();
                if self.cur_token.kind == TokenKind::Function {
                    self.next_token();
                }
                self.parse_function(false, false, true)
            }

            TokenKind::Import => {
                self.next_token();
                let path = match &self.cur_token.kind {
                    TokenKind::String(s) => s.clone(),
                    TokenKind::Identifier(s) => s.clone(),
                    _ => return None,
                };
                self.next_token();
                Some(Statement::Import(path))
            }

            TokenKind::Function => {
                self.next_token();
                self.parse_function(false, false, false)
            }

            TokenKind::Extern => {
                self.next_token();
                self.skip_newlines();
                if self.cur_token.kind == TokenKind::Function {
                    self.next_token();
                }
                self.parse_extern_function()
            }

            TokenKind::Public | TokenKind::Private | TokenKind::Protected => {
                let access = match &self.cur_token.kind {
                    TokenKind::Public => AccessModifier::Public,
                    TokenKind::Private => AccessModifier::Private,
                    _ => AccessModifier::Protected,
                };
                self.next_token();
                self.skip_newlines();
                if self.cur_token.kind == TokenKind::Function {
                    self.next_token();
                }
                let func_stmt = self.parse_function(false, false, false)?;
                if let Statement::Function { name, params, return_type, body, .. } = func_stmt {
                    Some(Statement::Method { access, name, params, return_type, body })
                } else { None }
            }

            TokenKind::Class => self.parse_class(),

            TokenKind::Let | TokenKind::Const | TokenKind::Var => self.parse_let_statement(),

            TokenKind::Async => {
                self.next_token();
                self.parse_statement()
            }

            TokenKind::SpawnKeyword => {
                self.next_token();
                let expr = self.parse_expression(0);
                Some(Statement::Expression(Expression::Spawn { task: Box::new(expr) }))
            }

            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::If => self.parse_if(),
            TokenKind::Try => self.parse_try_catch(),
            TokenKind::Throw => self.parse_throw(),
            TokenKind::Break => { self.next_token(); Some(Statement::Break) }
            TokenKind::Continue => { self.next_token(); Some(Statement::Continue) }

            TokenKind::Return => {
                self.next_token();
                let expr = if self.cur_token.kind == TokenKind::Newline
                    || self.cur_token.kind == TokenKind::Semicolon
                    || self.cur_token.kind == TokenKind::EOF
                {
                    Expression::Literal(Literal::Null)
                } else {
                    self.parse_expression(0)
                };
                Some(Statement::Return(expr))
            }

            // Assignment: identifier = expr (without let)
            TokenKind::Identifier(_) if matches!(self.peek_token.kind, TokenKind::Assign | TokenKind::PlusAssign | TokenKind::MinusAssign | TokenKind::MulAssign | TokenKind::DivAssign) => {
                let name = if let TokenKind::Identifier(n) = &self.cur_token.kind { n.clone() } else { unreachable!() };
                let op = self.peek_token.kind.clone();
                self.next_token(); // skip identifier
                self.next_token(); // skip assignment op
                let value = self.parse_expression(0);
                // Desugar compound assignments
                let final_value = match op {
                    TokenKind::PlusAssign =>
                        Expression::BinaryOp { left: Box::new(Expression::Identifier(name.clone())), op: "+".into(), right: Box::new(value) },
                    TokenKind::MinusAssign =>
                        Expression::BinaryOp { left: Box::new(Expression::Identifier(name.clone())), op: "-".into(), right: Box::new(value) },
                    TokenKind::MulAssign =>
                        Expression::BinaryOp { left: Box::new(Expression::Identifier(name.clone())), op: "*".into(), right: Box::new(value) },
                    TokenKind::DivAssign =>
                        Expression::BinaryOp { left: Box::new(Expression::Identifier(name.clone())), op: "/".into(), right: Box::new(value) },
                    _ => value,
                };
                Some(Statement::Let { name, value: final_value, ty: VajraType::Unknown })
            }

            _ => {
                let expr = self.parse_expression(0);
                Some(Statement::Expression(expr))
            }
        }
    }

    fn parse_class(&mut self) -> Option<Statement> {
        self.next_token(); // skip 'class'
        let name = if let TokenKind::Identifier(id) = &self.cur_token.kind { id.clone() } else { return None; };
        self.next_token();
        self.skip_newlines();

        if self.cur_token.kind != TokenKind::LBrace { return None; }
        self.next_token();

        let mut methods = Vec::new();
        let mut fields = Vec::new();

        while self.cur_token.kind != TokenKind::RBrace && self.cur_token.kind != TokenKind::EOF {
            self.skip_newlines();
            if self.cur_token.kind == TokenKind::RBrace { break; }
            if let Some(stmt) = self.parse_statement() {
                match stmt {
                    Statement::Function { .. } | Statement::Method { .. } => methods.push(stmt),
                    Statement::Let { name, ty, .. } => fields.push((name, ty)),
                    _ => {}
                }
            } else {
                self.next_token();
            }
        }
        self.next_token(); // skip '}'
        Some(Statement::Class { name, fields, methods })
    }

    fn parse_function(&mut self, is_main: bool, is_extern: bool, is_inline: bool) -> Option<Statement> {
        let name = if let TokenKind::Identifier(id) = &self.cur_token.kind { id.clone() } else { return None; };
        self.next_token();

        let params = self.parse_param_list();

        // Optional return type: -> Type
        let return_type = if self.cur_token.kind == TokenKind::Arrow {
            self.next_token();
            self.parse_type_annotation()
        } else {
            VajraType::Unknown
        };

        self.skip_newlines();

        if self.cur_token.kind != TokenKind::LBrace { return None; }
        self.next_token();
        let mut body = Vec::new();
        while self.cur_token.kind != TokenKind::RBrace && self.cur_token.kind != TokenKind::EOF {
            if let Some(stmt) = self.parse_statement() { body.push(stmt); }
            else { self.next_token(); }
        }
        self.next_token(); // skip '}'

        Some(Statement::Function { name, params, return_type, body, is_main, is_extern, is_inline })
    }

    fn parse_extern_function(&mut self) -> Option<Statement> {
        let name = if let TokenKind::Identifier(id) = &self.cur_token.kind { id.clone() } else { return None; };
        self.next_token();
        let params = self.parse_param_list();
        let return_type = if self.cur_token.kind == TokenKind::Arrow {
            self.next_token();
            self.parse_type_annotation()
        } else { VajraType::Unknown };
        Some(Statement::Function { name, params, return_type, body: Vec::new(), is_main: false, is_extern: true, is_inline: false })
    }

    fn parse_param_list(&mut self) -> Vec<Param> {
        let mut params = Vec::new();
        if self.cur_token.kind != TokenKind::LParen { return params; }
        self.next_token(); // skip '('
        while self.cur_token.kind != TokenKind::RParen && self.cur_token.kind != TokenKind::EOF {
            if let TokenKind::Identifier(id) = &self.cur_token.kind {
                let pname = id.clone();
                self.next_token();
                let ty = if self.cur_token.kind == TokenKind::Colon {
                    self.next_token();
                    self.parse_type_annotation()
                } else { VajraType::Unknown };
                params.push(Param { name: pname, ty });
            } else {
                self.next_token();
            }
            if self.cur_token.kind == TokenKind::Comma { self.next_token(); }
        }
        self.next_token(); // skip ')'
        params
    }

    fn parse_type_annotation(&mut self) -> VajraType {
        match &self.cur_token.kind {
            TokenKind::Identifier(s) => {
                let ty = match s.as_str() {
                    "i64" | "int" | "Int" | "integer" => VajraType::I64,
                    "f64" | "float" | "Float" | "double" => VajraType::F64,
                    "bool" | "Bool" | "boolean" => VajraType::Bool,
                    "str" | "String" | "string" => VajraType::Str,
                    "void" | "Void" => VajraType::Void,
                    _ => VajraType::Unknown,
                };
                self.next_token();
                ty
            }
            TokenKind::Star => {
                self.next_token();
                VajraType::Ptr(Box::new(self.parse_type_annotation()))
            }
            _ => VajraType::Unknown,
        }
    }

    fn parse_let_statement(&mut self) -> Option<Statement> {
        self.next_token(); // skip let/var/const
        let name = if let TokenKind::Identifier(id) = &self.cur_token.kind { id.clone() } else { return None; };
        self.next_token();

        // Optional type annotation: name: Type
        let ty = if self.cur_token.kind == TokenKind::Colon {
            self.next_token();
            self.parse_type_annotation()
        } else { VajraType::Unknown };

        self.skip_newlines();
        if self.cur_token.kind != TokenKind::Assign { return None; }
        self.next_token();

        let value = self.parse_expression(0);
        Some(Statement::Let { name, value, ty })
    }

    fn parse_expression(&mut self, _precedence: i32) -> Expression {
        self.skip_newlines();

        let mut left = self.parse_primary();

        // Postfix/infix operators
        loop {
            match &self.cur_token.kind {
                TokenKind::Newline | TokenKind::Semicolon | TokenKind::EOF
                | TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => break,

                TokenKind::LParen => {
                    // Function call
                    self.next_token();
                    let mut args = Vec::new();
                    while self.cur_token.kind != TokenKind::RParen && self.cur_token.kind != TokenKind::EOF {
                        args.push(self.parse_expression(0));
                        if self.cur_token.kind == TokenKind::Comma { self.next_token(); }
                    }
                    self.next_token(); // skip ')'

                    left = match left {
                        Expression::Identifier(name) => {
                            if is_print_name(&name) {
                                Expression::Intrinsic(Intrinsic::Print(args))
                            } else {
                                Expression::FunctionCall { name, args }
                            }
                        }
                        Expression::PropertyAccess { object, property } => {
                            // obj.method(args)
                            let is_print_call = match &*object {
                                Expression::Identifier(n) => {
                                    (n == "console" && property == "log")
                                    || (n == "System" && (property == "out" || property == "println" || property == "print"))
                                }
                                Expression::PropertyAccess { object: inner, property: p } => {
                                    if let Expression::Identifier(n) = inner.as_ref() {
                                        n == "System" && p == "out" && (property == "println" || property == "print")
                                    } else { false }
                                }
                                _ => false,
                            };
                            if is_print_call || is_print_name(&property) {
                                Expression::Intrinsic(Intrinsic::Print(args))
                            } else {
                                Expression::MethodCall { receiver: object, method: property, args }
                            }
                        }
                        _other => {
                            // Calling result of expression (e.g., factory()()) — treat as intrinsic void
                            Expression::FunctionCall { name: "__call__".into(), args }
                        },
                    };
                }

                TokenKind::LBracket => {
                    // Index operation: expr[idx]
                    self.next_token();
                    let idx = self.parse_expression(0);
                    if self.cur_token.kind == TokenKind::RBracket { self.next_token(); }
                    left = Expression::Index { object: Box::new(left), index: Box::new(idx) };
                }

                TokenKind::Dot => {
                    self.next_token();
                    let prop = if let TokenKind::Identifier(id) = &self.cur_token.kind {
                        id.clone()
                    } else { break; };
                    self.next_token();
                    left = Expression::PropertyAccess { object: Box::new(left), property: prop };
                }

                TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash
                | TokenKind::Percent | TokenKind::LessThan | TokenKind::GreaterThan
                | TokenKind::Equal | TokenKind::NotEqual | TokenKind::LessEqual
                | TokenKind::GreaterEqual | TokenKind::AndAnd | TokenKind::OrOr
                | TokenKind::Ampersand | TokenKind::Pipe | TokenKind::Caret => {
                    let op = match &self.cur_token.kind {
                        TokenKind::Plus => "+", TokenKind::Minus => "-",
                        TokenKind::Star => "*", TokenKind::Slash => "/",
                        TokenKind::Percent => "%",
                        TokenKind::LessThan => "<", TokenKind::GreaterThan => ">",
                        TokenKind::Equal => "==", TokenKind::NotEqual => "!=",
                        TokenKind::LessEqual => "<=", TokenKind::GreaterEqual => ">=",
                        TokenKind::AndAnd => "&&", TokenKind::OrOr => "||",
                        TokenKind::Ampersand => "&", TokenKind::Pipe => "|",
                        TokenKind::Caret => "^",
                        _ => unreachable!(),
                    }.to_string();
                    self.next_token();
                    let right = self.parse_primary();
                    left = Expression::BinaryOp { left: Box::new(left), op, right: Box::new(right) };
                }

                _ => break,
            }
        }

        left
    }

    fn parse_primary(&mut self) -> Expression {
        self.skip_newlines();

        match &self.cur_token.kind.clone() {
            TokenKind::Bang => {
                self.next_token();
                let operand = self.parse_primary();
                Expression::UnaryOp { op: "!".into(), operand: Box::new(operand) }
            }
            TokenKind::Minus => {
                self.next_token();
                match &self.cur_token.kind {
                    TokenKind::Integer(i) => {
                        let val = -*i;
                        self.next_token();
                        Expression::Literal(Literal::Integer(val))
                    }
                    TokenKind::Float(f) => {
                        let val = -*f;
                        self.next_token();
                        Expression::Literal(Literal::Float(val))
                    }
                    _ => {
                        let operand = self.parse_primary();
                        Expression::UnaryOp { op: "-".into(), operand: Box::new(operand) }
                    }
                }
            }
            TokenKind::Integer(i) => {
                let v = *i;
                self.next_token();
                Expression::Literal(Literal::Integer(v))
            }
            TokenKind::Float(f) => {
                let v = *f;
                self.next_token();
                Expression::Literal(Literal::Float(v))
            }
            TokenKind::String(s) => {
                let v = s.clone();
                self.next_token();
                Expression::Literal(Literal::String(v))
            }
            TokenKind::True => { self.next_token(); Expression::Literal(Literal::Bool(true)) }
            TokenKind::False => { self.next_token(); Expression::Literal(Literal::Bool(false)) }
            TokenKind::Null => { self.next_token(); Expression::Literal(Literal::Null) }
            TokenKind::This => { self.next_token(); Expression::Identifier("this".into()) }
            TokenKind::New => {
                self.next_token();
                self.parse_expression(0)
            }
            TokenKind::Await => {
                self.next_token();
                self.parse_expression(0)
            }
            TokenKind::LParen => {
                self.next_token();
                let expr = self.parse_expression(0);
                if self.cur_token.kind == TokenKind::RParen { self.next_token(); }
                expr
            }
            TokenKind::Identifier(id) => {
                let name = id.clone();
                self.next_token();
                Expression::Identifier(name)
            }
            // SpawnKeyword used as expression
            TokenKind::SpawnKeyword => {
                self.next_token();
                let task = self.parse_expression(0);
                Expression::Spawn { task: Box::new(task) }
            }
            _ => {
                // Produce a null literal for unrecognized tokens to avoid panics
                Expression::Literal(Literal::Null)
            }
        }
    }

    fn parse_while(&mut self) -> Option<Statement> {
        self.next_token(); // skip 'while'
        let has_paren = self.cur_token.kind == TokenKind::LParen;
        if has_paren { self.next_token(); }
        let condition = self.parse_expression(0);
        if has_paren && self.cur_token.kind == TokenKind::RParen { self.next_token(); }
        self.skip_newlines();
        let body = self.parse_block();
        Some(Statement::While { condition, body })
    }

    fn parse_for(&mut self) -> Option<Statement> {
        self.next_token(); // skip 'for'
        let has_paren = self.cur_token.kind == TokenKind::LParen;
        if has_paren { self.next_token(); }

        let var_name = if let TokenKind::Identifier(id) = &self.cur_token.kind {
            id.clone()
        } else { return None; };
        self.next_token();

        if self.cur_token.kind != TokenKind::In { return None; }
        self.next_token(); // skip 'in'

        let iterable = self.parse_expression(0);
        if has_paren && self.cur_token.kind == TokenKind::RParen { self.next_token(); }
        self.skip_newlines();
        let body = self.parse_block();
        Some(Statement::For { var_name, iterable, body })
    }

    fn parse_if(&mut self) -> Option<Statement> {
        self.next_token(); // skip 'if'
        let condition = if self.cur_token.kind == TokenKind::LParen {
            self.next_token();
            let c = self.parse_expression(0);
            if self.cur_token.kind == TokenKind::RParen { self.next_token(); }
            c
        } else {
            self.parse_expression(0)
        };

        self.skip_newlines();
        let then_body = self.parse_block();

        // Check for else / elif
        self.skip_newlines();
        let else_body = if self.cur_token.kind == TokenKind::Else {
            self.next_token();
            self.skip_newlines();
            // elif = else if
            if self.cur_token.kind == TokenKind::If {
                if let Some(elif_stmt) = self.parse_if() {
                    Some(vec![elif_stmt])
                } else { None }
            } else {
                Some(self.parse_block())
            }
        } else { None };

        Some(Statement::If { condition, then_body, else_body })
    }

    fn parse_try_catch(&mut self) -> Option<Statement> {
        self.next_token(); // skip 'try'
        self.skip_newlines();
        let try_body = self.parse_block();

        self.skip_newlines();
        if self.cur_token.kind != TokenKind::Catch { return Some(Statement::TryCatch { try_body, catch_var: "e".into(), catch_body: vec![] }); }
        self.next_token(); // skip 'catch'

        let mut catch_var = "e".to_string();
        if self.cur_token.kind == TokenKind::LParen {
            self.next_token();
            if let TokenKind::Identifier(id) = &self.cur_token.kind { catch_var = id.clone(); self.next_token(); }
            if self.cur_token.kind == TokenKind::RParen { self.next_token(); }
        }

        self.skip_newlines();
        let catch_body = self.parse_block();
        Some(Statement::TryCatch { try_body, catch_var, catch_body })
    }

    fn parse_throw(&mut self) -> Option<Statement> {
        self.next_token();
        let exception = self.parse_expression(0);
        Some(Statement::Throw { exception })
    }

    fn parse_block(&mut self) -> Vec<Statement> {
        let mut body = Vec::new();
        if self.cur_token.kind != TokenKind::LBrace { return body; }
        self.next_token();
        while self.cur_token.kind != TokenKind::RBrace && self.cur_token.kind != TokenKind::EOF {
            if let Some(stmt) = self.parse_statement() {
                body.push(stmt);
            } else {
                self.next_token();
            }
        }
        if self.cur_token.kind == TokenKind::RBrace { self.next_token(); }
        body
    }
}

fn is_print_name(name: &str) -> bool {
    matches!(name,
        "print" | "println" | "लिखो" | "मुद्रित" | "लेखन" | "likho" | "mudrit"
        | "अच्सिडु" | "அச்சிடு" | "طباعة" | "打印" | "imprimir" | "afficher"
        | "drucken" | "표시" | "छापा" | "ముద్రించు" | "ಮುದ್ರಿಸು" | "मुद्रण"
        | "मुद्रिसु" | "печать" | "출력" | "表示" | "Console.log"
        | "console_log"
    )
}

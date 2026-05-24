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

        let mut base = None;
        // Support 'class Child : Parent' or 'class Child extends Parent'
        if self.cur_token.kind == TokenKind::Colon || self.cur_token.kind == TokenKind::DoubleColon {
            self.next_token();
            if let TokenKind::Identifier(base_name) = &self.cur_token.kind {
                base = Some(base_name.clone());
                self.next_token();
            }
        } else if let TokenKind::Identifier(ext) = &self.cur_token.kind {
            if ext == "extends" {
                self.next_token();
                if let TokenKind::Identifier(base_name) = &self.cur_token.kind {
                    base = Some(base_name.clone());
                    self.next_token();
                }
            }
        }

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
        Some(Statement::Class { name, base, fields, methods })
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

    fn get_precedence(kind: &TokenKind) -> i32 {
        match kind {
            TokenKind::Dot | TokenKind::LParen | TokenKind::LBracket | TokenKind::PlusPlus | TokenKind::MinusMinus => 90,
            TokenKind::Pow => 50,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => 40,
            TokenKind::Plus | TokenKind::Minus => 35,
            TokenKind::LessThan | TokenKind::GreaterThan | TokenKind::LessEqual | TokenKind::GreaterEqual => 30,
            TokenKind::Equal | TokenKind::NotEqual => 25,
            TokenKind::Ampersand => 20,
            TokenKind::Caret => 18,
            TokenKind::Pipe => 16,
            TokenKind::AndAnd => 10,
            TokenKind::OrOr => 5,
            TokenKind::Question => 3,
            TokenKind::Assign
            | TokenKind::PlusAssign
            | TokenKind::MinusAssign
            | TokenKind::MulAssign
            | TokenKind::DivAssign => 2,
            _ => 0,
        }
    }

    fn parse_expression(&mut self, precedence: i32) -> Expression {
        self.skip_newlines();

        let mut left = self.parse_primary();

        loop {
            let next_prec = Self::get_precedence(&self.cur_token.kind);
            if next_prec <= precedence {
                break;
            }

            match &self.cur_token.kind {
                TokenKind::PlusPlus => {
                    self.next_token();
                    left = match left {
                        Expression::Identifier(name) => {
                            Expression::Assign {
                                name: name.clone(),
                                value: Box::new(Expression::BinaryOp {
                                    left: Box::new(Expression::Identifier(name)),
                                    op: "+".to_string(),
                                    right: Box::new(Expression::Literal(Literal::Integer(1)))
                                })
                            }
                        }
                        Expression::PropertyAccess { object, property } => {
                            Expression::PropertyAssign {
                                object: object.clone(),
                                property: property.clone(),
                                value: Box::new(Expression::BinaryOp {
                                    left: Box::new(Expression::PropertyAccess { object: object.clone(), property: property.clone() }),
                                    op: "+".to_string(),
                                    right: Box::new(Expression::Literal(Literal::Integer(1)))
                                })
                            }
                        }
                        Expression::Index { object, index } => {
                            Expression::IndexAssign {
                                object: object.clone(),
                                index: index.clone(),
                                value: Box::new(Expression::BinaryOp {
                                    left: Box::new(Expression::Index { object: object.clone(), index: index.clone() }),
                                    op: "+".to_string(),
                                    right: Box::new(Expression::Literal(Literal::Integer(1)))
                                })
                            }
                        }
                        _ => left,
                    };
                }
                TokenKind::MinusMinus => {
                    self.next_token();
                    left = match left {
                        Expression::Identifier(name) => {
                            Expression::Assign {
                                name: name.clone(),
                                value: Box::new(Expression::BinaryOp {
                                    left: Box::new(Expression::Identifier(name)),
                                    op: "-".to_string(),
                                    right: Box::new(Expression::Literal(Literal::Integer(1)))
                                })
                            }
                        }
                        Expression::PropertyAccess { object, property } => {
                            Expression::PropertyAssign {
                                object: object.clone(),
                                property: property.clone(),
                                value: Box::new(Expression::BinaryOp {
                                    left: Box::new(Expression::PropertyAccess { object: object.clone(), property: property.clone() }),
                                    op: "-".to_string(),
                                    right: Box::new(Expression::Literal(Literal::Integer(1)))
                                })
                            }
                        }
                        Expression::Index { object, index } => {
                            Expression::IndexAssign {
                                object: object.clone(),
                                index: index.clone(),
                                value: Box::new(Expression::BinaryOp {
                                    left: Box::new(Expression::Index { object: object.clone(), index: index.clone() }),
                                    op: "-".to_string(),
                                    right: Box::new(Expression::Literal(Literal::Integer(1)))
                                })
                            }
                        }
                        _ => left,
                    };
                }
                TokenKind::Pow => {
                    self.next_token();
                    let right = self.parse_expression(next_prec - 1); // Right-associative!
                    left = Expression::BinaryOp { left: Box::new(left), op: "**".to_string(), right: Box::new(right) };
                }
                TokenKind::Question => {
                    self.next_token();
                    let then_expr = self.parse_expression(0);
                    if self.cur_token.kind != TokenKind::Colon {
                        panic!("Syntax Error: Mismatched ':' for ternary operator '?' on line {}", self.cur_token.line);
                    }
                    self.next_token(); // skip ':'
                    let else_expr = self.parse_expression(next_prec - 1);
                    left = Expression::Ternary {
                        condition: Box::new(left),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                    };
                }

                TokenKind::LParen => {
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
                                if name.ends_with("ln") || name == "लिखो" || name == "छापा" || name == "मुद्रित" {
                                    Expression::Intrinsic(Intrinsic::PrintLn(args))
                                } else {
                                    Expression::Intrinsic(Intrinsic::Print(args))
                                }
                            } else {
                                Expression::FunctionCall { name, args }
                            }
                        }
                        Expression::PropertyAccess { object, property } => {
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
                                if property.ends_with("ln") || property == "लिखो" || property == "छापा" || property == "मुद्रित" || property == "log" {
                                    Expression::Intrinsic(Intrinsic::PrintLn(args))
                                } else {
                                    Expression::Intrinsic(Intrinsic::Print(args))
                                }
                            } else {
                                Expression::MethodCall { receiver: object, method: property, args }
                            }
                        }
                        other => {
                            let mut call_args = Vec::with_capacity(args.len() + 1);
                            call_args.push(other);
                            call_args.extend(args);
                            Expression::FunctionCall { name: "__call__".into(), args: call_args }
                        }
                    };
                }

                TokenKind::LBracket => {
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
                    let right = self.parse_expression(next_prec);
                    left = Expression::BinaryOp { left: Box::new(left), op, right: Box::new(right) };
                }

                TokenKind::Assign => {
                    self.next_token();
                    let right = self.parse_expression(next_prec - 1);
                    left = match left {
                        Expression::Identifier(name) => {
                            Expression::Assign { name, value: Box::new(right) }
                        }
                        Expression::PropertyAccess { object, property } => {
                            Expression::PropertyAssign { object, property, value: Box::new(right) }
                        }
                        Expression::Index { object, index } => {
                            Expression::IndexAssign { object, index, value: Box::new(right) }
                        }
                        _ => left,
                    };
                }

                TokenKind::PlusAssign | TokenKind::MinusAssign | TokenKind::MulAssign | TokenKind::DivAssign => {
                    let op_kind = self.cur_token.kind.clone();
                    self.next_token();
                    let right = self.parse_expression(next_prec - 1);
                    let op_str = match op_kind {
                        TokenKind::PlusAssign => "+",
                        TokenKind::MinusAssign => "-",
                        TokenKind::MulAssign => "*",
                        TokenKind::DivAssign => "/",
                        _ => unreachable!(),
                    }.to_string();

                    left = match left {
                        Expression::Identifier(name) => {
                            let value = Expression::BinaryOp {
                                left: Box::new(Expression::Identifier(name.clone())),
                                op: op_str,
                                right: Box::new(right),
                            };
                            Expression::Assign { name, value: Box::new(value) }
                        }
                        Expression::PropertyAccess { object, property } => {
                            let value = Expression::BinaryOp {
                                left: Box::new(Expression::PropertyAccess { object: object.clone(), property: property.clone() }),
                                op: op_str,
                                right: Box::new(right),
                            };
                            Expression::PropertyAssign { object, property, value: Box::new(value) }
                        }
                        Expression::Index { object, index } => {
                            let value = Expression::BinaryOp {
                                left: Box::new(Expression::Index { object: object.clone(), index: index.clone() }),
                                op: op_str,
                                right: Box::new(right),
                            };
                            Expression::IndexAssign { object, index, value: Box::new(value) }
                        }
                        _ => left,
                    };
                }

                _ => break,
            }
        }

        left
    }

    fn parse_primary(&mut self) -> Expression {
        self.skip_newlines();

        match &self.cur_token.kind.clone() {
            TokenKind::PlusPlus => {
                self.next_token();
                let operand = self.parse_primary();
                match operand {
                    Expression::Identifier(name) => {
                        Expression::Assign {
                            name: name.clone(),
                            value: Box::new(Expression::BinaryOp {
                                left: Box::new(Expression::Identifier(name)),
                                op: "+".to_string(),
                                right: Box::new(Expression::Literal(Literal::Integer(1)))
                            })
                        }
                    }
                    Expression::PropertyAccess { object, property } => {
                        Expression::PropertyAssign {
                            object: object.clone(),
                            property: property.clone(),
                            value: Box::new(Expression::BinaryOp {
                                left: Box::new(Expression::PropertyAccess { object: object.clone(), property: property.clone() }),
                                op: "+".to_string(),
                                right: Box::new(Expression::Literal(Literal::Integer(1)))
                            })
                        }
                    }
                    Expression::Index { object, index } => {
                        Expression::IndexAssign {
                            object: object.clone(),
                            index: index.clone(),
                            value: Box::new(Expression::BinaryOp {
                                left: Box::new(Expression::Index { object: object.clone(), index: index.clone() }),
                                op: "+".to_string(),
                                right: Box::new(Expression::Literal(Literal::Integer(1)))
                            })
                        }
                    }
                    _ => operand,
                }
            }
            TokenKind::MinusMinus => {
                self.next_token();
                let operand = self.parse_primary();
                match operand {
                    Expression::Identifier(name) => {
                        Expression::Assign {
                            name: name.clone(),
                            value: Box::new(Expression::BinaryOp {
                                left: Box::new(Expression::Identifier(name)),
                                op: "-".to_string(),
                                right: Box::new(Expression::Literal(Literal::Integer(1)))
                            })
                        }
                    }
                    Expression::PropertyAccess { object, property } => {
                        Expression::PropertyAssign {
                            object: object.clone(),
                            property: property.clone(),
                            value: Box::new(Expression::BinaryOp {
                                left: Box::new(Expression::PropertyAccess { object: object.clone(), property: property.clone() }),
                                op: "-".to_string(),
                                right: Box::new(Expression::Literal(Literal::Integer(1)))
                            })
                        }
                    }
                    Expression::Index { object, index } => {
                        Expression::IndexAssign {
                            object: object.clone(),
                            index: index.clone(),
                            value: Box::new(Expression::BinaryOp {
                                left: Box::new(Expression::Index { object: object.clone(), index: index.clone() }),
                                op: "-".to_string(),
                                right: Box::new(Expression::Literal(Literal::Integer(1)))
                            })
                        }
                    }
                    _ => operand,
                }
            }
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
            TokenKind::BigInt(s) => {
                let v = s.clone();
                self.next_token();
                Expression::Literal(Literal::BigInt(v))
            }
            TokenKind::Float(f) => {
                let v = *f;
                self.next_token();
                Expression::Literal(Literal::Float(v))
            }
            TokenKind::String(s) => {
                let v = s.clone();
                self.next_token();
                if v.contains('{') {
                    self.parse_interpolated_string(&v)
                } else {
                    Expression::Literal(Literal::String(v))
                }
            }
            TokenKind::True => { self.next_token(); Expression::Literal(Literal::Bool(true)) }
            TokenKind::False => { self.next_token(); Expression::Literal(Literal::Bool(false)) }
            TokenKind::Null => { self.next_token(); Expression::Literal(Literal::Null) }
            TokenKind::This => { self.next_token(); Expression::Identifier("this".into()) }
            TokenKind::New => {
                self.next_token();
                let class_name = if let TokenKind::Identifier(id) = &self.cur_token.kind {
                    id.clone()
                } else {
                    return Expression::Literal(Literal::Null);
                };
                self.next_token();
                let mut args = Vec::new();
                if self.cur_token.kind == TokenKind::LParen {
                    self.next_token();
                    while self.cur_token.kind != TokenKind::RParen && self.cur_token.kind != TokenKind::EOF {
                        args.push(self.parse_expression(0));
                        if self.cur_token.kind == TokenKind::Comma { self.next_token(); }
                    }
                    if self.cur_token.kind == TokenKind::RParen { self.next_token(); }
                }
                Expression::ObjectInstantiation { class_name, args }
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
            TokenKind::LBracket => {
                self.next_token();
                let expr = self.parse_expression(0);
                if self.cur_token.kind == TokenKind::RBracket { self.next_token(); }
                expr
            }
            TokenKind::Identifier(id) => {
                let name = id.clone();
                if name == "f" && matches!(self.peek_token.kind, TokenKind::String(_)) {
                    self.next_token(); // skip "f"
                    if let TokenKind::String(s) = &self.cur_token.kind {
                        let s_val = s.clone();
                        self.next_token(); // skip string
                        self.parse_interpolated_string(&s_val)
                    } else {
                        unreachable!()
                    }
                } else {
                    self.next_token();
                    Expression::Identifier(name)
                }
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

        // Determine if standard for-in loop by checking if the next token is 'in' after the identifier
        let is_for_in = if let TokenKind::Identifier(_) = &self.cur_token.kind {
            self.peek_token.kind == TokenKind::In
        } else {
            false
        };

        if is_for_in {
            let var_name = if let TokenKind::Identifier(id) = &self.cur_token.kind {
                id.clone()
            } else { return None; };
            self.next_token();
            self.next_token(); // skip 'in'

            let iterable = self.parse_expression(0);
            if has_paren && self.cur_token.kind == TokenKind::RParen { self.next_token(); }
            self.skip_newlines();
            let body = self.parse_block();
            Some(Statement::For { var_name, iterable, body })
        } else {
            // C-style loop: for (init; cond; step) or for (init, cond, step)
            let init = if matches!(self.cur_token.kind, TokenKind::Let | TokenKind::Var | TokenKind::Const) {
                self.parse_let_statement()
            } else {
                let expr = self.parse_expression(0);
                Some(Statement::Expression(expr))
            };

            // Skip separator (semicolon or comma)
            if self.cur_token.kind == TokenKind::Semicolon || self.cur_token.kind == TokenKind::Comma {
                self.next_token();
            }

            // Parse condition
            let cond = self.parse_expression(0);

            // Skip separator
            if self.cur_token.kind == TokenKind::Semicolon || self.cur_token.kind == TokenKind::Comma {
                self.next_token();
            }

            // Parse step
            let step_expr = self.parse_expression(0);
            let step = Statement::Expression(step_expr);

            // Skip closing parenthesis if open parenthesis was used
            if has_paren && self.cur_token.kind == TokenKind::RParen {
                self.next_token();
            }

            self.skip_newlines();

            // Parse body block
            let mut body = self.parse_block();

            // Setup continue rewriter helper to recursively prepend step to continue statements
            fn rewrite_continues(stmts: &mut Vec<Statement>, step: &Statement) {
                let mut i = 0;
                while i < stmts.len() {
                    match &mut stmts[i] {
                        Statement::Continue => {
                            stmts.insert(i, step.clone());
                            i += 2;
                            continue;
                        }
                        Statement::If { then_body, else_body, .. } => {
                            rewrite_continues(then_body, step);
                            if let Some(eb) = else_body {
                                rewrite_continues(eb, step);
                            }
                        }
                        Statement::TryCatch { try_body, catch_body, .. } => {
                            rewrite_continues(try_body, step);
                            rewrite_continues(catch_body, step);
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }

            // Rewrite continue statements in loop body
            rewrite_continues(&mut body, &step);

            // Append step to body block
            body.push(step);

            // Desugar to: if true { init; while cond { body } }
            let while_loop = Statement::While { condition: cond, body };
            let mut wrapper_body = Vec::new();
            if let Some(i_stmt) = init {
                wrapper_body.push(i_stmt);
            }
            wrapper_body.push(while_loop);

            Some(Statement::If {
                condition: Expression::Literal(Literal::Bool(true)),
                then_body: wrapper_body,
                else_body: None,
            })
        }
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

    fn parse_interpolated_string(&mut self, s: &str) -> Expression {
        let mut left: Option<Expression> = None;
        let mut i = 0;
        let bytes = s.as_bytes();
        let len = bytes.len();
        
        while i < len {
            let mut next_start = None;
            let mut is_js_style = false;
            let mut j = i;
            while j < len {
                if j + 2 <= len && &bytes[j..j+2] == b"${" {
                    next_start = Some(j);
                    is_js_style = true;
                    break;
                } else if bytes[j] == b'{' {
                    next_start = Some(j);
                    is_js_style = false;
                    break;
                }
                j += 1;
            }
            
            if let Some(start) = next_start {
                if start > i {
                    let literal_str = String::from_utf8_lossy(&bytes[i..start]).into_owned();
                    let literal_expr = Expression::Literal(Literal::String(literal_str));
                    left = Some(match left {
                        Some(prev) => Expression::BinaryOp {
                            left: Box::new(prev),
                            op: "+".to_string(),
                            right: Box::new(literal_expr),
                        },
                        None => literal_expr,
                    });
                }
                
                let mut brace_count = 1;
                let expr_start = if is_js_style { start + 2 } else { start + 1 };
                let mut expr_end = expr_start;
                while expr_end < len {
                    if bytes[expr_end] == b'{' {
                        brace_count += 1;
                    } else if bytes[expr_end] == b'}' {
                        brace_count -= 1;
                        if brace_count == 0 {
                            break;
                        }
                    }
                    expr_end += 1;
                }
                
                if expr_end < len {
                    let expr_str = String::from_utf8_lossy(&bytes[expr_start..expr_end]).into_owned();
                    let sub_lexer = Lexer::new(&expr_str);
                    let mut sub_parser = Parser::new(sub_lexer);
                    let sub_expr = sub_parser.parse_expression(0);
                    
                    left = Some(match left {
                        Some(prev) => Expression::BinaryOp {
                            left: Box::new(prev),
                            op: "+".to_string(),
                            right: Box::new(sub_expr),
                        },
                        None => sub_expr,
                    });
                    
                    i = expr_end + 1;
                } else {
                    panic!("Syntax Error: Mismatched '}}' in string interpolation");
                }
            } else {
                let literal_str = String::from_utf8_lossy(&bytes[i..]).into_owned();
                let literal_expr = Expression::Literal(Literal::String(literal_str));
                left = Some(match left {
                    Some(prev) => Expression::BinaryOp {
                        left: Box::new(prev),
                        op: "+".to_string(),
                        right: Box::new(literal_expr),
                    },
                    None => literal_expr,
                });
                break;
            }
        }
        
        left.unwrap_or(Expression::Literal(Literal::String(String::new())))
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

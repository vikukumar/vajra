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
        Self {
            lexer,
            cur_token,
            peek_token,
        }
    }

    fn next_token(&mut self) {
        self.cur_token = self.peek_token.clone();
        self.peek_token = self.lexer.next_token();
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
        match &self.cur_token.kind {
            TokenKind::MainDecorator => {
                self.next_token(); // skip @main
                while self.cur_token.kind == TokenKind::Newline {
                    self.next_token();
                }
                if self.cur_token.kind == TokenKind::Function {
                    self.next_token(); // skip function keyword
                }
                self.parse_function(true)
            }
            TokenKind::ExternDecorator => {
                self.next_token(); // skip @extern
                while self.cur_token.kind == TokenKind::Newline {
                    self.next_token();
                }
                if self.cur_token.kind == TokenKind::Function {
                    self.next_token(); // skip function keyword
                }
                self.parse_extern_function()
            }
            TokenKind::Import => {
                self.next_token(); // skip import / आयात
                if let TokenKind::String(path) = &self.cur_token.kind {
                    let path_str = path.clone();
                    self.next_token();
                    Some(Statement::Import(path_str))
                } else {
                    panic!("Expected string path after import statement");
                }
            }
            TokenKind::Function => {
                self.next_token();
                self.parse_function(false)
            }
            TokenKind::Public | TokenKind::Private | TokenKind::Protected => {
                let access = match &self.cur_token.kind {
                    TokenKind::Public => AccessModifier::Public,
                    TokenKind::Private => AccessModifier::Private,
                    TokenKind::Protected => AccessModifier::Protected,
                    _ => unreachable!(),
                };
                self.next_token(); // skip modifier
                
                while self.cur_token.kind == TokenKind::Newline {
                    self.next_token();
                }
                
                if self.cur_token.kind == TokenKind::Function {
                    self.next_token(); // skip function keyword
                }
                
                let func_stmt = self.parse_function(false)?;
                if let Statement::Function { name, params, body, .. } = func_stmt {
                    Some(Statement::Method { access, name, params, body })
                } else {
                    None
                }
            }
            TokenKind::Class => {
                self.parse_class()
            }
            TokenKind::Let => self.parse_let_statement(),
            TokenKind::Identifier(name) if self.peek_token.kind == TokenKind::Assign => {
                let var_name = name.clone();
                self.next_token(); // skip identifier
                self.next_token(); // skip '='
                let value = self.parse_expression(0);
                Some(Statement::Let { name: var_name, value })
            }
            TokenKind::While => self.parse_while(),
            TokenKind::If => self.parse_if(),
            TokenKind::Try => self.parse_try_catch(),
            TokenKind::Throw => self.parse_throw(),
            TokenKind::Return => {
                self.next_token();
                let expr = self.parse_expression(0);
                Some(Statement::Return(expr))
            }
            TokenKind::Newline | TokenKind::Semicolon | TokenKind::EOF => None,
            _ => {
                let expr = self.parse_expression(0);
                Some(Statement::Expression(expr))
            }
        }
    }

    fn parse_class(&mut self) -> Option<Statement> {
        self.next_token(); // skip 'class'
        let name = if let TokenKind::Identifier(id) = &self.cur_token.kind {
            id.clone()
        } else {
            return None;
        };

        self.next_token(); // skip name
        
        while self.cur_token.kind == TokenKind::Newline {
            self.next_token();
        }

        if self.cur_token.kind == TokenKind::LBrace {
            self.next_token();
            let mut methods = Vec::new();
            while self.cur_token.kind != TokenKind::RBrace && self.cur_token.kind != TokenKind::EOF {
                while self.cur_token.kind == TokenKind::Newline {
                    self.next_token();
                }
                if self.cur_token.kind == TokenKind::RBrace {
                    break;
                }
                if let Some(stmt) = self.parse_statement() {
                    // Collect methods
                    match &stmt {
                        Statement::Function { .. } | Statement::Method { .. } => {
                            methods.push(stmt);
                        }
                        _ => {}
                    }
                } else {
                    self.next_token();
                }
            }
            self.next_token(); // skip '}'
            Some(Statement::Class { name, methods })
        } else {
            None
        }
    }

    fn parse_function(&mut self, is_main: bool) -> Option<Statement> {
        let name = if let TokenKind::Identifier(id) = &self.cur_token.kind {
            id.clone()
        } else {
            return None;
        };

        self.next_token(); // skip name

        let mut params = Vec::new();
        if self.cur_token.kind == TokenKind::LParen {
            self.next_token();
            while self.cur_token.kind != TokenKind::RParen && self.cur_token.kind != TokenKind::EOF {
                if let TokenKind::Identifier(id) = &self.cur_token.kind {
                    params.push(id.clone());
                }
                self.next_token();
                if self.cur_token.kind == TokenKind::Comma {
                    self.next_token();
                }
            }
            self.next_token(); // skip ')'
        }

        // skip newlines before brace if any
        while self.cur_token.kind == TokenKind::Newline {
            self.next_token();
        }

        if self.cur_token.kind == TokenKind::LBrace {
            self.next_token();
            let mut body = Vec::new();
            while self.cur_token.kind != TokenKind::RBrace && self.cur_token.kind != TokenKind::EOF {
                if let Some(stmt) = self.parse_statement() {
                    body.push(stmt);
                } else {
                    self.next_token();
                }
            }
            self.next_token(); // skip '}'
            Some(Statement::Function {
                name,
                params,
                body,
                is_main,
                is_extern: false,
            })
        } else {
            None
        }
    }

    fn parse_extern_function(&mut self) -> Option<Statement> {
        let name = if let TokenKind::Identifier(id) = &self.cur_token.kind {
            id.clone()
        } else {
            return None;
        };

        self.next_token(); // skip name

        let mut params = Vec::new();
        if self.cur_token.kind == TokenKind::LParen {
            self.next_token();
            while self.cur_token.kind != TokenKind::RParen && self.cur_token.kind != TokenKind::EOF {
                if let TokenKind::Identifier(id) = &self.cur_token.kind {
                    params.push(id.clone());
                }
                self.next_token();
                if self.cur_token.kind == TokenKind::Comma {
                    self.next_token();
                }
            }
            self.next_token(); // skip ')'
        }

        Some(Statement::Function {
            name,
            params,
            body: Vec::new(),
            is_main: false,
            is_extern: true,
        })
    }

    fn parse_let_statement(&mut self) -> Option<Statement> {
        self.next_token(); // skip Let
        let name = if let TokenKind::Identifier(id) = &self.cur_token.kind {
            id.clone()
        } else {
            return None;
        };
        self.next_token(); // skip name
        
        // skip newlines
        while self.cur_token.kind == TokenKind::Newline {
            self.next_token();
        }

        if self.cur_token.kind != TokenKind::Assign {
            return None;
        }
        self.next_token(); // skip =
        
        let value = self.parse_expression(0);
        Some(Statement::Let { name, value })
    }

    fn parse_expression(&mut self, _precedence: i32) -> Expression {
        // Skip leading newlines if any
        while self.cur_token.kind == TokenKind::Newline {
            self.next_token();
        }

        let mut left = match &self.cur_token.kind {
            TokenKind::Minus | TokenKind::Plus => {
                let is_negative = self.cur_token.kind == TokenKind::Minus;
                self.next_token(); // skip sign
                match &self.cur_token.kind {
                    TokenKind::Integer(i) => {
                        let val = if is_negative { -*i } else { *i };
                        let expr = Expression::Literal(Literal::Integer(val));
                        self.next_token();
                        expr
                    }
                    TokenKind::Float(f) => {
                        let val = if is_negative { -*f } else { *f };
                        let expr = Expression::Literal(Literal::Float(val));
                        self.next_token();
                        expr
                    }
                    _ => panic!("Expected integer or float after unary sign"),
                }
            }
            TokenKind::Integer(i) => {
                let expr = Expression::Literal(Literal::Integer(*i));
                self.next_token();
                expr
            }
            TokenKind::Float(f) => {
                let expr = Expression::Literal(Literal::Float(*f));
                self.next_token();
                expr
            }
            TokenKind::String(s) => {
                let expr = Expression::Literal(Literal::String(s.clone()));
                self.next_token();
                expr
            }
            TokenKind::Identifier(id) => {
                let expr = Expression::Identifier(id.clone());
                self.next_token();
                expr
            }
            _ => panic!("Unexpected token in expression: {:?}", self.cur_token),
        };

        while self.cur_token.kind != TokenKind::Semicolon && 
              self.cur_token.kind != TokenKind::Newline &&
              self.cur_token.kind != TokenKind::EOF &&
              self.cur_token.kind != TokenKind::RBrace &&
              self.cur_token.kind != TokenKind::RParen {
            
            match &self.cur_token.kind {
                TokenKind::LParen => {
                    self.next_token(); // skip '('
                    let mut args = Vec::new();
                    while self.cur_token.kind != TokenKind::RParen && self.cur_token.kind != TokenKind::EOF {
                        args.push(self.parse_expression(0));
                        if self.cur_token.kind == TokenKind::Comma {
                            self.next_token(); // skip ','
                        }
                    }
                    self.next_token(); // skip ')'
                    if let Expression::Identifier(name) = left {
                        if name == "print" || name == "लिखो" || name == "मुद्रित" || name == "लेखन" || name == "likho" || name == "mudrit" {
                            left = Expression::Intrinsic(Intrinsic::Print(args));
                        } else {
                            left = Expression::FunctionCall { name, args };
                        }
                    } else {
                        panic!("Can only call identifiers as functions");
                    }
                }
                TokenKind::Dot => {
                    self.next_token(); // skip '.'
                    let method = if let TokenKind::Identifier(id) = &self.cur_token.kind {
                        id.clone()
                    } else {
                        panic!("Expected method name after .");
                    };
                    self.next_token(); // skip method name
                    
                    let mut args = Vec::new();
                    if self.cur_token.kind == TokenKind::LParen {
                        self.next_token(); // skip '('
                        while self.cur_token.kind != TokenKind::RParen && self.cur_token.kind != TokenKind::EOF {
                            args.push(self.parse_expression(0));
                            if self.cur_token.kind == TokenKind::Comma {
                                self.next_token(); // skip ','
                            }
                        }
                        self.next_token(); // skip ')'
                        left = Expression::MethodCall {
                            receiver: Box::new(left),
                            method,
                            args,
                        };
                    } else {
                        left = Expression::PropertyAccess {
                            object: Box::new(left),
                            property: method,
                        };
                    }
                }
                TokenKind::Plus | TokenKind::Minus | TokenKind::Star | TokenKind::Slash |
                TokenKind::LessThan | TokenKind::GreaterThan | TokenKind::Equal | TokenKind::LessEqual | TokenKind::GreaterEqual => {
                    let op = match &self.cur_token.kind {
                        TokenKind::Plus => "+",
                        TokenKind::Minus => "-",
                        TokenKind::Star => "*",
                        TokenKind::Slash => "/",
                        TokenKind::LessThan => "<",
                        TokenKind::GreaterThan => ">",
                        TokenKind::Equal => "==",
                        TokenKind::LessEqual => "<=",
                        TokenKind::GreaterEqual => ">=",
                        _ => unreachable!(),
                    }.to_string();
                    self.next_token(); // skip operator
                    let right = self.parse_expression(0);
                    left = Expression::BinaryOp {
                        left: Box::new(left),
                        op,
                        right: Box::new(right),
                    };
                }
                _ => break,
            }
        }

        left
    }

    fn parse_while(&mut self) -> Option<Statement> {
        self.next_token(); // skip 'while' / 'यावत्' / 'जबतक'
        
        let has_paren = self.cur_token.kind == TokenKind::LParen;
        if has_paren {
            self.next_token();
        }
        let condition = self.parse_expression(0);
        if has_paren && self.cur_token.kind == TokenKind::RParen {
            self.next_token();
        }
        
        while self.cur_token.kind == TokenKind::Newline {
            self.next_token();
        }
        
        let mut body = Vec::new();
        if self.cur_token.kind == TokenKind::LBrace {
            self.next_token(); // skip '{'
            while self.cur_token.kind != TokenKind::RBrace && self.cur_token.kind != TokenKind::EOF {
                if let Some(stmt) = self.parse_statement() {
                    body.push(stmt);
                } else {
                    self.next_token();
                }
            }
            self.next_token(); // skip '}'
        }
        
        Some(Statement::While { condition, body })
    }

    fn parse_try_catch(&mut self) -> Option<Statement> {
        self.next_token(); // skip 'try' / 'प्रयत्न' / 'प्रयास'
        
        while self.cur_token.kind == TokenKind::Newline {
            self.next_token();
        }
        
        let mut try_body = Vec::new();
        if self.cur_token.kind == TokenKind::LBrace {
            self.next_token(); // skip '{'
            while self.cur_token.kind != TokenKind::RBrace && self.cur_token.kind != TokenKind::EOF {
                if let Some(stmt) = self.parse_statement() {
                    try_body.push(stmt);
                } else {
                    self.next_token();
                }
            }
            self.next_token(); // skip '}'
        }
        
        while self.cur_token.kind == TokenKind::Newline {
            self.next_token();
        }
        
        if self.cur_token.kind != TokenKind::Catch {
            panic!("Expected catch block after try block");
        }
        self.next_token(); // skip 'catch'
        
        let mut catch_var = "error".to_string();
        if self.cur_token.kind == TokenKind::LParen {
            self.next_token(); // skip '('
            if let TokenKind::Identifier(id) = &self.cur_token.kind {
                catch_var = id.clone();
                self.next_token();
            }
            if self.cur_token.kind == TokenKind::RParen {
                self.next_token(); // skip ')'
            }
        }
        
        while self.cur_token.kind == TokenKind::Newline {
            self.next_token();
        }
        
        let mut catch_body = Vec::new();
        if self.cur_token.kind == TokenKind::LBrace {
            self.next_token(); // skip '{'
            while self.cur_token.kind != TokenKind::RBrace && self.cur_token.kind != TokenKind::EOF {
                if let Some(stmt) = self.parse_statement() {
                    catch_body.push(stmt);
                } else {
                    self.next_token();
                }
            }
            self.next_token(); // skip '}'
        }
        
        Some(Statement::TryCatch { try_body, catch_var, catch_body })
    }

    fn parse_if(&mut self) -> Option<Statement> {
        self.next_token(); // skip if / यदि / चेत्
        
        let condition = if self.cur_token.kind == TokenKind::LParen {
            self.next_token(); // skip '('
            let cond = self.parse_expression(0);
            if self.cur_token.kind == TokenKind::RParen {
                self.next_token(); // skip ')'
            }
            cond
        } else {
            self.parse_expression(0)
        };

        // Skip newlines
        while self.cur_token.kind == TokenKind::Newline {
            self.next_token();
        }

        if self.cur_token.kind != TokenKind::LBrace {
            panic!("Expected '{{' after if condition, got {:?}", self.cur_token.kind);
        }
        self.next_token(); // skip '{'

        let mut then_body = Vec::new();
        while self.cur_token.kind != TokenKind::RBrace && self.cur_token.kind != TokenKind::EOF {
            if self.cur_token.kind == TokenKind::Newline || self.cur_token.kind == TokenKind::Semicolon {
                self.next_token();
                continue;
            }
            if let Some(stmt) = self.parse_statement() {
                then_body.push(stmt);
            } else {
                self.next_token();
            }
        }
        
        if self.cur_token.kind == TokenKind::RBrace {
            self.next_token(); // skip '}'
        }

        // Check for else / नोचेत् / अन्यथा
        while self.cur_token.kind == TokenKind::Newline {
            self.next_token();
        }

        let mut else_body = None;
        if self.cur_token.kind == TokenKind::Else {
            self.next_token(); // skip else / नोचेत् / अन्यथा
            
            // Skip newlines
            while self.cur_token.kind == TokenKind::Newline {
                self.next_token();
            }

            if self.cur_token.kind != TokenKind::LBrace {
                panic!("Expected '{{' after else keyword, got {:?}", self.cur_token.kind);
            }
            self.next_token(); // skip '{'

            let mut body = Vec::new();
            while self.cur_token.kind != TokenKind::RBrace && self.cur_token.kind != TokenKind::EOF {
                if self.cur_token.kind == TokenKind::Newline || self.cur_token.kind == TokenKind::Semicolon {
                    self.next_token();
                    continue;
                }
                if let Some(stmt) = self.parse_statement() {
                    body.push(stmt);
                } else {
                    self.next_token();
                }
            }

            if self.cur_token.kind == TokenKind::RBrace {
                self.next_token(); // skip '}'
            }
            else_body = Some(body);
        }

        Some(Statement::If { condition, then_body, else_body })
    }

    fn parse_throw(&mut self) -> Option<Statement> {
        self.next_token(); // skip 'throw' / 'त्यज' / 'फेंकें'
        let exception = self.parse_expression(0);
        Some(Statement::Throw { exception })
    }
}

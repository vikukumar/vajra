use unicode_segmentation::UnicodeSegmentation;
use std::collections::HashMap;
use lazy_static::lazy_static;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Function, // fn, कार्या, कार्य
    Let,      // let, अस्तु, मान
    Class,    // class, वर्ग, श्रेणी
    If,       // if, यदि, चेत्
    Else,     // else, अन्यथा, नोचेत्
    Return,   // return, प्रतिफल, निवर्तय
    Public,   // public, सार्वजनिक
    Private,  // private, निजी, रहस्य
    Protected, // protected, सुरक्षित
    While,    // while, यावत्, जबतक
    Try,      // try, प्रयत्न, प्रयास
    Catch,    // catch, ग्रहण, पकड़ें
    Throw,    // throw, त्यज, फेंकें
    Import,   // import, आयात
    For,      // for, कृते, चक्र
    In,       // in, अन्तः, मध्ये, में
    Const,    // const
    Var,      // var
    Async,    // async
    Await,    // await
    New,      // new
    This,     // this, self
    True,     // true, True, सत्यम्
    False,    // false, False, असत्यम्
    Null,     // null, None, void, शून्य
    SpawnKeyword, // spawn, thread, parallel
    
    
    
    // Decorators
    MainDecorator,   // @main
    ExternDecorator, // @extern
    
    // Identifiers & Literals
    Identifier(String),
    Integer(i64),
    Float(f64),
    String(String),
    
    // Operators & Punctuation
    Dot,        // .
    Comma,      // ,
    Semicolon,  // ;
    LParen,     // (
    RParen,     // )
    LBrace,     // {
    RBrace,     // }
    Assign,     // =
    Plus,       // +
    Minus,      // -
    Star,       // *
    Slash,      // /
    LessThan,   // <
    GreaterThan,// >
    Equal,      // ==
    LessEqual,  // <=
    GreaterEqual,// >=
    
    // Whitespace/Flow
    Newline,
    EOF,
}

lazy_static! {
    static ref KEYWORDS: HashMap<&'static str, TokenKind> = {
        let mut m = HashMap::new();
        // English
        m.insert("fn", TokenKind::Function);
        m.insert("let", TokenKind::Let);
        m.insert("class", TokenKind::Class);
        m.insert("if", TokenKind::If);
        m.insert("else", TokenKind::Else);
        m.insert("return", TokenKind::Return);
        m.insert("public", TokenKind::Public);
        m.insert("private", TokenKind::Private);
        m.insert("protected", TokenKind::Protected);
        m.insert("while", TokenKind::While);
        m.insert("try", TokenKind::Try);
        m.insert("catch", TokenKind::Catch);
        m.insert("throw", TokenKind::Throw);
        m.insert("import", TokenKind::Import);
        m.insert("for", TokenKind::For);
        m.insert("in", TokenKind::In);
        m.insert("const", TokenKind::Const);
        m.insert("var", TokenKind::Var);
        m.insert("async", TokenKind::Async);
        m.insert("await", TokenKind::Await);
        m.insert("new", TokenKind::New);
        m.insert("this", TokenKind::This);
        m.insert("self", TokenKind::This);
        m.insert("true", TokenKind::True);
        m.insert("True", TokenKind::True);
        m.insert("false", TokenKind::False);
        m.insert("False", TokenKind::False);
        m.insert("null", TokenKind::Null);
        m.insert("None", TokenKind::Null);
        m.insert("void", TokenKind::Null);
        m.insert("spawn", TokenKind::SpawnKeyword);
        m.insert("thread", TokenKind::SpawnKeyword);
        m.insert("parallel", TokenKind::SpawnKeyword);
        m.insert("def", TokenKind::Function);
        m.insert("function", TokenKind::Function);
        m.insert("require", TokenKind::Import);
        m.insert("elif", TokenKind::Else);
        
        // Sanskrit
        m.insert("कार्या", TokenKind::Function);
        m.insert("अस्तु", TokenKind::Let);
        m.insert("वर्ग", TokenKind::Class);
        m.insert("चेत्", TokenKind::If);
        m.insert("नोचेत्", TokenKind::Else);
        m.insert("निवर्तय", TokenKind::Return);
        m.insert("सार्वजनिक", TokenKind::Public);
        m.insert("रहस्य", TokenKind::Private);
        m.insert("वैयक्तिक", TokenKind::Private);
        m.insert("सुरक्षित", TokenKind::Protected);
        m.insert("यावत्", TokenKind::While);
        m.insert("प्रयत्न", TokenKind::Try);
        m.insert("ग्रहण", TokenKind::Catch);
        m.insert("त्यज", TokenKind::Throw);
        m.insert("आयात", TokenKind::Import);
        m.insert("कृते", TokenKind::For);
        m.insert("अन्तः", TokenKind::In);
        m.insert("मध्ये", TokenKind::In);
        m.insert("सत्यम्", TokenKind::True);
        m.insert("असत्यम्", TokenKind::False);
        m.insert("शून्य", TokenKind::Null);
        
        // Hindi
        m.insert("कार्य", TokenKind::Function);
        m.insert("मान", TokenKind::Let);
        m.insert("श्रेणी", TokenKind::Class);
        m.insert("यदि", TokenKind::If);
        m.insert("अन्यथा", TokenKind::Else);
        m.insert("प्रतिफल", TokenKind::Return);
        m.insert("निजी", TokenKind::Private);
        m.insert("जबतक", TokenKind::While);
        m.insert("प्रयास", TokenKind::Try);
        m.insert("पकड़ें", TokenKind::Catch);
        m.insert("फेंकें", TokenKind::Throw);
        m.insert("चक्र", TokenKind::For);
        m.insert("में", TokenKind::In);
        m.insert("अन्दर", TokenKind::In);
        m.insert("सत्य", TokenKind::True);
        m.insert("असत्य", TokenKind::False);
        
        m
    };
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

pub struct Lexer<'a> {
    input: Vec<&'a str>,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let graphemes = UnicodeSegmentation::graphemes(input, true).collect();
        Self {
            input: graphemes,
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<&&'a str> {
        self.input.get(self.pos)
    }

    fn advance(&mut self) -> Option<&&'a str> {
        let res = self.input.get(self.pos);
        if let Some(g) = res {
            self.pos += 1;
            if *g == "\n" {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        res
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let line = self.line;
        let col = self.col;

        let g = match self.advance() {
            Some(g) => *g,
            None => return Token { kind: TokenKind::EOF, line, col },
        };

        let kind = match g {
            "@" => {
                let id = self.read_identifier();
                if id == "main" {
                    TokenKind::MainDecorator
                } else if id == "extern" {
                    TokenKind::ExternDecorator
                } else {
                    panic!("Unknown decorator @{}", id);
                }
            }
            "." => TokenKind::Dot,
            "," => TokenKind::Comma,
            ";" => TokenKind::Semicolon,
            "(" => TokenKind::LParen,
            ")" => TokenKind::RParen,
            "{" => TokenKind::LBrace,
            "}" => TokenKind::RBrace,
            "=" => {
                if let Some(&"=") = self.peek() {
                    self.advance();
                    TokenKind::Equal
                } else {
                    TokenKind::Assign
                }
            }
            "<" => {
                if let Some(&"=") = self.peek() {
                    self.advance();
                    TokenKind::LessEqual
                } else {
                    TokenKind::LessThan
                }
            }
            ">" => {
                if let Some(&"=") = self.peek() {
                    self.advance();
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::GreaterThan
                }
            }
            "+" => TokenKind::Plus,
            "-" => TokenKind::Minus,
            "*" => TokenKind::Star,
            "/" => TokenKind::Slash,
            "\n" => TokenKind::Newline,
            _ if g.chars().next().unwrap().is_alphabetic() || g == "_" => {
                let mut id = g.to_string();
                id.push_str(&self.read_identifier());
                if let Some(kw) = KEYWORDS.get(id.as_str()) {
                    kw.clone()
                } else {
                    TokenKind::Identifier(id)
                }
            }
            _ if g.chars().next().unwrap().is_numeric() => {
                let mut num = g.to_string();
                num.push_str(&self.read_number());
                if num.contains('.') {
                    TokenKind::Float(num.parse().unwrap_or(0.0))
                } else {
                    match num.parse::<i64>() {
                        Ok(val) => TokenKind::Integer(val),
                        Err(_) => TokenKind::Float(num.parse::<f64>().unwrap_or(0.0)),
                    }
                }
            }
            "\"" => TokenKind::String(self.read_string()),
            _ => panic!("Unexpected character: {} at line {}, col {}", g, line, col),
        };

        Token { kind, line, col }
    }

    fn read_identifier(&mut self) -> String {
        let mut id = String::new();
        while let Some(g) = self.peek() {
            let c = g.chars().next().unwrap();
            if c.is_alphanumeric() || *g == "_" {
                id.push_str(self.advance().unwrap());
            } else {
                break;
            }
        }
        id
    }

    fn read_number(&mut self) -> String {
        let mut num = String::new();
        while let Some(g) = self.peek() {
            let c = g.chars().next().unwrap();
            if c.is_numeric() || *g == "." {
                num.push_str(self.advance().unwrap());
            } else {
                break;
            }
        }
        num
    }

    fn read_string(&mut self) -> String {
        let mut s = String::new();
        while let Some(g) = self.advance() {
            if *g == "\"" {
                break;
            }
            s.push_str(g);
        }
        s
    }

    fn skip_whitespace(&mut self) {
        loop {
            let mut skipped = false;
            while let Some(g) = self.peek() {
                if *g == " " || *g == "\t" || *g == "\r" {
                    self.advance();
                    skipped = true;
                } else {
                    break;
                }
            }
            if let Some(&"#") = self.peek() {
                self.advance(); // consume '#'
                while let Some(g) = self.peek() {
                    if *g == "\n" {
                        break;
                    }
                    self.advance();
                }
                skipped = true;
            }
            if !skipped {
                break;
            }
        }
    }
}

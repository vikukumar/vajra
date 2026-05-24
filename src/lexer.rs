use unicode_segmentation::UnicodeSegmentation;
use std::collections::HashMap;
use lazy_static::lazy_static;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Function,    // fn, कार्या, कार्य, def, function, செயல்பாடு, دالة, 函数, función
    Let,         // let, अस्तु, मान, var, const, மாறி, متغير, 变量, variable
    Class,       // class, वर्ग, श्रेणी, வகுப்பு, فئة, 类, clase
    If,          // if, यदि, चेत्, என்றால், إذا, 如果, si
    Else,        // else, अन्यथा, नोचेत्, இல்லையெனில், وإلا, 否则, sino
    Return,      // return, प्रतिफल, निवर्तय, திரும்பு, إرجاع, 返回, retorno
    Public,      // public, सार्वजनिक
    Private,     // private, निजी, रहस्य
    Protected,   // protected, सुरक्षित
    While,       // while, यावत्, जबतक, போது, بينما, 当, mientras
    Try,         // try, प्रयत्न, प्रयास
    Catch,       // catch, ग्रहण, पकड़ें
    Throw,       // throw, त्यज, फेंकें
    Import,      // import, आयात, இறக்கு, استيراد, 导入, importar
    For,         // for, कृते, चक्र, க்கான, من_أجل, 为了, para
    In,          // in, अन्तः, मध्ये, में
    Const,       // const
    Var,         // var
    Async,       // async
    Await,       // await
    New,         // new
    This,        // this, self
    True,        // true, True, सत्यम्, सत्य
    False,       // false, False, असत्यम्, असत्य
    Null,        // null, None, void, शून्य
    SpawnKeyword,// spawn, thread, parallel
    Break,       // break, विराम
    Continue,    // continue, जारी
    Extern,      // extern (for FFI declarations without @extern decorator)
    Inline,      // inline

    // Decorators
    MainDecorator,    // @main
    ExternDecorator,  // @extern
    InlineDecorator,  // @inline

    // Identifiers & Literals
    Identifier(String),
    Integer(i64),
    BigInt(String),
    Float(f64),
    String(String),

    // Operators & Punctuation
    Dot,          // .
    Comma,        // ,
    Semicolon,    // ;
    Colon,        // :
    DoubleColon,  // ::
    LParen,       // (
    RParen,       // )
    LBrace,       // {
    RBrace,       // }
    LBracket,     // [
    RBracket,     // ]
    Assign,       // =
    Plus,         // +
    Minus,        // -
    Star,         // *
    Slash,        // /
    Percent,      // %
    Ampersand,    // &
    Pipe,         // |
    Caret,        // ^
    Bang,         // !
    Tilde,        // ~
    LessThan,     // <
    GreaterThan,  // >
    Equal,        // ==
    NotEqual,     // !=
    LessEqual,    // <=
    GreaterEqual, // >=
    Arrow,        // ->
    FatArrow,     // =>
    AndAnd,       // &&
    OrOr,         // ||
    PlusAssign,   // +=
    MinusAssign,  // -=
    MulAssign,    // *=
    DivAssign,    // /=
    At,           // @ (for decorators)
    Pow,          // **
    Question,     // ?
    PlusPlus,     // ++
    MinusMinus,   // --

    // Whitespace/Flow
    Newline,
    EOF,
}

lazy_static! {
    static ref KEYWORDS: HashMap<&'static str, TokenKind> = {
        let mut m = HashMap::new();

        // ── English ─────────────────────────────────────────────────────────
        m.insert("fn",        TokenKind::Function);
        m.insert("def",       TokenKind::Function);
        m.insert("function",  TokenKind::Function);
        m.insert("fun",       TokenKind::Function);
        m.insert("func",      TokenKind::Function);
        m.insert("method",    TokenKind::Function);
        m.insert("let",       TokenKind::Let);
        m.insert("var",       TokenKind::Var);
        m.insert("const",     TokenKind::Const);
        m.insert("class",     TokenKind::Class);
        m.insert("struct",    TokenKind::Class);
        m.insert("if",        TokenKind::If);
        m.insert("elif",      TokenKind::Else);
        m.insert("else",      TokenKind::Else);
        m.insert("return",    TokenKind::Return);
        m.insert("ret",       TokenKind::Return);
        m.insert("public",    TokenKind::Public);
        m.insert("pub",       TokenKind::Public);
        m.insert("private",   TokenKind::Private);
        m.insert("priv",      TokenKind::Private);
        m.insert("protected", TokenKind::Protected);
        m.insert("while",     TokenKind::While);
        m.insert("loop",      TokenKind::While);
        m.insert("try",       TokenKind::Try);
        m.insert("catch",     TokenKind::Catch);
        m.insert("except",    TokenKind::Catch);
        m.insert("throw",     TokenKind::Throw);
        m.insert("raise",     TokenKind::Throw);
        m.insert("import",    TokenKind::Import);
        m.insert("require",   TokenKind::Import);
        m.insert("include",   TokenKind::Import);
        m.insert("use",       TokenKind::Import);
        m.insert("for",       TokenKind::For);
        m.insert("in",        TokenKind::In);
        m.insert("async",     TokenKind::Async);
        m.insert("await",     TokenKind::Await);
        m.insert("new",       TokenKind::New);
        m.insert("this",      TokenKind::This);
        m.insert("self",      TokenKind::This);
        m.insert("current",   TokenKind::This);
        m.insert("true",      TokenKind::True);
        m.insert("True",      TokenKind::True);
        m.insert("false",     TokenKind::False);
        m.insert("False",     TokenKind::False);
        m.insert("null",      TokenKind::Null);
        m.insert("None",      TokenKind::Null);
        m.insert("nil",       TokenKind::Null);
        m.insert("void",      TokenKind::Null);
        m.insert("spawn",     TokenKind::SpawnKeyword);
        m.insert("thread",    TokenKind::SpawnKeyword);
        m.insert("parallel",  TokenKind::SpawnKeyword);
        m.insert("break",     TokenKind::Break);
        m.insert("continue",  TokenKind::Continue);
        m.insert("extern",    TokenKind::Extern);
        m.insert("inline",    TokenKind::Inline);

        // ── Sanskrit ─────────────────────────────────────────────────────────
        m.insert("कार्या",      TokenKind::Function);
        m.insert("क्रिया",     TokenKind::Function);
        m.insert("अस्तु",       TokenKind::Let);
        m.insert("वर्ग",        TokenKind::Class);
        m.insert("चेत्",        TokenKind::If);
        m.insert("स्व",        TokenKind::This);
        m.insert("अयम्",      TokenKind::This);
        m.insert("इदम्",      TokenKind::This);
        m.insert("नोचेत्",      TokenKind::Else);
        m.insert("निवर्तय",     TokenKind::Return);
        m.insert("सार्वजनिक",   TokenKind::Public);
        m.insert("रहस्य",       TokenKind::Private);
        m.insert("वैयक्तिक",    TokenKind::Private);
        m.insert("सुरक्षित",    TokenKind::Protected);
        m.insert("यावत्",       TokenKind::While);
        m.insert("प्रयत्न",     TokenKind::Try);
        m.insert("ग्रहण",       TokenKind::Catch);
        m.insert("त्यज",        TokenKind::Throw);
        m.insert("आयात",        TokenKind::Import);
        m.insert("कृते",        TokenKind::For);
        m.insert("अन्तः",       TokenKind::In);
        m.insert("मध्ये",       TokenKind::In);
        m.insert("सत्यम्",      TokenKind::True);
        m.insert("असत्यम्",     TokenKind::False);
        m.insert("शून्य",       TokenKind::Null);
        m.insert("विराम",       TokenKind::Break);
        m.insert("बाह्य",       TokenKind::Extern);

        // ── Hindi ─────────────────────────────────────────────────────────────
        m.insert("कार्य",       TokenKind::Function);
        m.insert("विधि",       TokenKind::Function);
        m.insert("तरीका",      TokenKind::Function);
        m.insert("प्रक्रिया",   TokenKind::Function);
        m.insert("मान",         TokenKind::Let);
        m.insert("श्रेणी",      TokenKind::Class);
        m.insert("ढांचा",      TokenKind::Class);
        m.insert("यदि",         TokenKind::If);
        m.insert("अन्यथा",      TokenKind::Else);
        m.insert("प्रतिफल",     TokenKind::Return);
        m.insert("लौटाएं",     TokenKind::Return);
        m.insert("लौटाओ",      TokenKind::Return);
        m.insert("स्वयं",      TokenKind::This);
        m.insert("यह",         TokenKind::This);
        m.insert("अपना",       TokenKind::This);
        m.insert("निजी",        TokenKind::Private);
        m.insert("जबतक",        TokenKind::While);
        m.insert("प्रयास",      TokenKind::Try);
        m.insert("पकड़ें",      TokenKind::Catch);
        m.insert("फेंकें",      TokenKind::Throw);
        m.insert("चक्र",        TokenKind::For);
        m.insert("में",         TokenKind::In);
        m.insert("अन्दर",       TokenKind::In);
        m.insert("सत्य",        TokenKind::True);
        m.insert("असत्य",       TokenKind::False);
        m.insert("लिखो",        TokenKind::Identifier("print".to_string())); // treated as print
        m.insert("प्रिंट",      TokenKind::Identifier("print".to_string())); // also treated as print
        m.insert("पढ़ो",        TokenKind::Identifier("readline".to_string()));
        m.insert("जारी",        TokenKind::Continue);
        m.insert("निर्गम",      TokenKind::Identifier("exit".to_string()));

        // ── Tamil (தமிழ்) ──────────────────────────────────────────────────
        m.insert("செயல்பாடு",   TokenKind::Function);
        m.insert("மாறி",        TokenKind::Let);
        m.insert("வகுப்பு",     TokenKind::Class);
        m.insert("என்றால்",     TokenKind::If);
        m.insert("இல்லையெனில்", TokenKind::Else);
        m.insert("திரும்பு",    TokenKind::Return);
        m.insert("போது",        TokenKind::While);
        m.insert("க்கான",       TokenKind::For);
        m.insert("இல்",         TokenKind::In);
        m.insert("இறக்கு",      TokenKind::Import);
        m.insert("அச்சிடு",     TokenKind::Identifier("print".to_string()));

        // ── Arabic (العربية) ───────────────────────────────────────────────
        m.insert("دالة",        TokenKind::Function);
        m.insert("متغير",       TokenKind::Let);
        m.insert("فئة",         TokenKind::Class);
        m.insert("إذا",         TokenKind::If);
        m.insert("وإلا",        TokenKind::Else);
        m.insert("إرجاع",       TokenKind::Return);
        m.insert("بينما",       TokenKind::While);
        m.insert("من_أجل",      TokenKind::For);
        m.insert("في",          TokenKind::In);
        m.insert("استيراد",     TokenKind::Import);
        m.insert("طباعة",       TokenKind::Identifier("print".to_string()));

        // ── Chinese Simplified (中文) ──────────────────────────────────────
        m.insert("函数",        TokenKind::Function);
        m.insert("变量",        TokenKind::Let);
        m.insert("类",          TokenKind::Class);
        m.insert("如果",        TokenKind::If);
        m.insert("否则",        TokenKind::Else);
        m.insert("返回",        TokenKind::Return);
        m.insert("当",          TokenKind::While);
        m.insert("为了",        TokenKind::For);
        m.insert("导入",        TokenKind::Import);
        m.insert("打印",        TokenKind::Identifier("print".to_string()));

        // ── Spanish (Español) ──────────────────────────────────────────────
        m.insert("función",     TokenKind::Function);
        m.insert("funcion",     TokenKind::Function);
        m.insert("variable",    TokenKind::Let);
        m.insert("clase",       TokenKind::Class);
        m.insert("si",          TokenKind::If);
        m.insert("sino",        TokenKind::Else);
        m.insert("retorno",     TokenKind::Return);
        m.insert("mientras",    TokenKind::While);
        m.insert("para",        TokenKind::For);
        m.insert("en",          TokenKind::In);
        m.insert("importar",    TokenKind::Import);
        m.insert("imprimir",    TokenKind::Identifier("print".to_string()));
        m.insert("verdadero",   TokenKind::True);
        m.insert("falso",       TokenKind::False);
        m.insert("nulo",        TokenKind::Null);

        // ── Marathi (मराठी) ────────────────────────────────────────────────
        m.insert("कार्यपद",    TokenKind::Function);
        m.insert("चल",         TokenKind::Let);
        m.insert("वर्ग",       TokenKind::Class); // shared with Sanskrit
        m.insert("जर",         TokenKind::If);
        m.insert("नाहीतर",    TokenKind::Else);
        m.insert("परत",        TokenKind::Return);
        m.insert("जोपर्यंत",  TokenKind::While);
        m.insert("साठी",       TokenKind::For);
        m.insert("मधे",        TokenKind::In);
        m.insert("छापा",       TokenKind::Identifier("print".to_string()));

        // ── Bengali (বাংলা) ────────────────────────────────────────────────
        m.insert("ফাংশন",      TokenKind::Function);
        m.insert("ধ্রুবক",     TokenKind::Let);
        m.insert("শ্রেণী",     TokenKind::Class);
        m.insert("যদি",        TokenKind::If);
        m.insert("অন্যথায়",   TokenKind::Else);
        m.insert("ফেরত",       TokenKind::Return);
        m.insert("যখন",        TokenKind::While);
        m.insert("জন্য",       TokenKind::For);
        m.insert("মুদ্রণ",     TokenKind::Identifier("print".to_string()));

        // ── Telugu (తెలుగు) ────────────────────────────────────────────────
        m.insert("ఫంక్షన్",   TokenKind::Function);
        m.insert("చరరాశి",    TokenKind::Let);
        m.insert("తరగతి",     TokenKind::Class);
        m.insert("ఒకవేళ",     TokenKind::If);
        m.insert("లేదా",      TokenKind::Else);
        m.insert("తిరిగి",    TokenKind::Return);
        m.insert("ముద్రించు", TokenKind::Identifier("print".to_string()));

        // ── Kannada (ಕನ್ನಡ) ───────────────────────────────────────────────
        m.insert("ಕಾರ್ಯ",     TokenKind::Function);
        m.insert("ಅಸ್ಥಿರ",   TokenKind::Let);
        m.insert("ವರ್ಗ",      TokenKind::Class);
        m.insert("ಅಗರ",       TokenKind::If);
        m.insert("ಇಲ್ಲದಿದ್ದರೆ", TokenKind::Else);
        m.insert("ಮರಳಿಸು",  TokenKind::Return);
        m.insert("ಮುದ್ರಿಸು", TokenKind::Identifier("print".to_string()));

        // ── Gujarati (ગુજરાતી) ─────────────────────────────────────────────
        m.insert("કાર્ય",     TokenKind::Function);
        m.insert("ચલ",        TokenKind::Let);
        m.insert("વર્ગ",      TokenKind::Class);
        m.insert("જો",        TokenKind::If);
        m.insert("નહિં",     TokenKind::Else);
        m.insert("પ્રિન્ટ",  TokenKind::Identifier("print".to_string()));

        // ── Russian (Русский) ──────────────────────────────────────────────
        m.insert("функция",   TokenKind::Function);
        m.insert("пусть",     TokenKind::Let);
        m.insert("класс",     TokenKind::Class);
        m.insert("если",      TokenKind::If);
        m.insert("иначе",     TokenKind::Else);
        m.insert("вернуть",   TokenKind::Return);
        m.insert("пока",      TokenKind::While);
        m.insert("для",       TokenKind::For);
        m.insert("печать",    TokenKind::Identifier("print".to_string()));

        // ── French (Français) ──────────────────────────────────────────────
        m.insert("fonction",  TokenKind::Function);
        m.insert("soit",      TokenKind::Let);
        m.insert("classe",    TokenKind::Class);
        m.insert("si",        TokenKind::If); // overlaps Spanish — fine, same meaning
        m.insert("sinon",     TokenKind::Else);
        m.insert("retourner", TokenKind::Return);
        m.insert("tant_que",  TokenKind::While);
        m.insert("pour",      TokenKind::For);
        m.insert("afficher",  TokenKind::Identifier("print".to_string()));

        // ── German (Deutsch) ───────────────────────────────────────────────
        m.insert("funktion",  TokenKind::Function);
        m.insert("sei",       TokenKind::Let);
        m.insert("klasse",    TokenKind::Class);
        m.insert("wenn",      TokenKind::If);
        m.insert("sonst",     TokenKind::Else);
        m.insert("rückgabe",  TokenKind::Return);
        m.insert("solange",   TokenKind::While);
        m.insert("drucken",   TokenKind::Identifier("print".to_string()));

        // ── Japanese (日本語) ──────────────────────────────────────────────
        m.insert("関数",       TokenKind::Function);
        m.insert("変数",       TokenKind::Let);
        m.insert("クラス",     TokenKind::Class);
        m.insert("もし",       TokenKind::If);
        m.insert("そうでなければ", TokenKind::Else);
        m.insert("戻る",       TokenKind::Return);
        m.insert("表示",       TokenKind::Identifier("print".to_string()));

        // ── Korean (한국어) ────────────────────────────────────────────────
        m.insert("함수",       TokenKind::Function);
        m.insert("변수",       TokenKind::Let);
        m.insert("클래스",     TokenKind::Class);
        m.insert("만약",       TokenKind::If);
        m.insert("그렇지_않으면", TokenKind::Else);
        m.insert("반환",       TokenKind::Return);
        m.insert("출력",       TokenKind::Identifier("print".to_string()));

        // ── Hinglish ────────────────────────────────────────────────────────
        m.insert("rakho",     TokenKind::Let);
        m.insert("lelo",      TokenKind::Let);
        m.insert("maan",      TokenKind::Let);
        m.insert("dharo",     TokenKind::Let);
        m.insert("astu",      TokenKind::Let);
        m.insert("rakh",      TokenKind::Let);
        m.insert("agar",      TokenKind::If);
        m.insert("warna",     TokenKind::Else);
        m.insert("nahi_to",   TokenKind::Else);
        m.insert("bhejo",     TokenKind::Return);
        m.insert("de_do",     TokenKind::Return);
        m.insert("jab_tak",   TokenKind::While);
        m.insert("dikhao",    TokenKind::Identifier("print".to_string()));
        m.insert("likho",     TokenKind::Identifier("print".to_string()));
        m.insert("class",     TokenKind::Class);
        m.insert("ye",        TokenKind::This);
        m.insert("yeh",       TokenKind::This);
        m.insert("apna",      TokenKind::This);
        m.insert("apne",      TokenKind::This);
        m.insert("khud",      TokenKind::This);
        m.insert("swayam",    TokenKind::This);
        m.insert("mera",      TokenKind::This);
        m.insert("khud_ka",   TokenKind::This);
        m.insert("karya",     TokenKind::Function);
        m.insert("kriya",     TokenKind::Function);
        m.insert("kam",       TokenKind::Function);
        m.insert("kam_karo",  TokenKind::Function);
        m.insert("vidhi",     TokenKind::Function);
        m.insert("tarika",    TokenKind::Function);
        m.insert("dhancha",   TokenKind::Class);


        // ── Bhojpuri (भोजपुरी) ────────────────────────────────────────────────
        m.insert("कारज",      TokenKind::Function);
        m.insert("करम",       TokenKind::Function);
        m.insert("धरऽ",       TokenKind::Let);
        m.insert("मानऽ",      TokenKind::Let);
        m.insert("जात",       TokenKind::Class);
        m.insert("जदि",       TokenKind::If);
        m.insert("ना_त",      TokenKind::Else);
        m.insert("लौटावऽ",    TokenKind::Return);
        m.insert("लिखऽ",      TokenKind::Identifier("print".to_string()));
        m.insert("देखावऽ",    TokenKind::Identifier("print".to_string()));
        m.insert("एह",        TokenKind::This);
        m.insert("ई",         TokenKind::This);
        m.insert("अपन",       TokenKind::This);

        // ── Haryanvi (हरियाणवी) ──────────────────────────────────────────────
        m.insert("काम",       TokenKind::Function);
        m.insert("धरदे",      TokenKind::Let);
        m.insert("इब",        TokenKind::Let);
        m.insert("जे",        TokenKind::If);
        m.insert("ना_त",      TokenKind::Else);
        m.insert("फेर_दे",    TokenKind::Return);
        m.insert("छाप",       TokenKind::Identifier("print".to_string()));
        m.insert("यो",        TokenKind::This);
        m.insert("खुद",       TokenKind::This);
        m.insert("आपणा",      TokenKind::This);
        m.insert("दे_दे",     TokenKind::Return);

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
        Self { input: graphemes, pos: 0, line: 1, col: 1 }
    }

    fn peek(&self) -> Option<&&'a str> {
        self.input.get(self.pos)
    }

    #[allow(dead_code)]
    fn peek_nth(&self, n: usize) -> Option<&&'a str> {
        self.input.get(self.pos + n)
    }

    fn advance(&mut self) -> Option<&&'a str> {
        let res = self.input.get(self.pos);
        if let Some(g) = res {
            self.pos += 1;
            if *g == "\n" || *g == "\r\n" {
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
                match id.as_str() {
                    "main"   => TokenKind::MainDecorator,
                    "extern" => TokenKind::ExternDecorator,
                    "inline" => TokenKind::InlineDecorator,
                    _        => TokenKind::At,
                }
            }
            "." => TokenKind::Dot,
            "," => TokenKind::Comma,
            ";" => TokenKind::Semicolon,
            ":" => {
                if let Some(&":") = self.peek() {
                    self.advance();
                    TokenKind::DoubleColon
                } else {
                    TokenKind::Colon
                }
            }
            "?" => TokenKind::Question,
            "(" => TokenKind::LParen,
            ")" => TokenKind::RParen,
            "{" => TokenKind::LBrace,
            "}" => TokenKind::RBrace,
            "[" => TokenKind::LBracket,
            "]" => TokenKind::RBracket,
            "~" => TokenKind::Tilde,
            "^" => TokenKind::Caret,
            "=" => {
                if let Some(&"=") = self.peek() { self.advance(); TokenKind::Equal }
                else if let Some(&">") = self.peek() { self.advance(); TokenKind::FatArrow }
                else { TokenKind::Assign }
            }
            "<" => {
                if let Some(&"=") = self.peek() { self.advance(); TokenKind::LessEqual }
                else { TokenKind::LessThan }
            }
            ">" => {
                if let Some(&"=") = self.peek() { self.advance(); TokenKind::GreaterEqual }
                else { TokenKind::GreaterThan }
            }
            "!" => {
                if let Some(&"=") = self.peek() { self.advance(); TokenKind::NotEqual }
                else { TokenKind::Bang }
            }
            "+" => {
                if let Some(&"=") = self.peek() { self.advance(); TokenKind::PlusAssign }
                else if let Some(&"+") = self.peek() { self.advance(); TokenKind::PlusPlus }
                else { TokenKind::Plus }
            }
            "-" => {
                if let Some(&">") = self.peek() { self.advance(); TokenKind::Arrow }
                else if let Some(&"=") = self.peek() { self.advance(); TokenKind::MinusAssign }
                else if let Some(&"-") = self.peek() { self.advance(); TokenKind::MinusMinus }
                else { TokenKind::Minus }
            }
            "*" => {
                if let Some(&"*") = self.peek() { self.advance(); TokenKind::Pow }
                else if let Some(&"=") = self.peek() { self.advance(); TokenKind::MulAssign }
                else { TokenKind::Star }
            }
            "/" => {
                // Check for // comment
                if let Some(&"/") = self.peek() {
                    self.advance();
                    while let Some(g) = self.peek() {
                        if *g == "\n" || *g == "\r\n" { break; }
                        self.advance();
                    }
                    return self.next_token();
                }
                if let Some(&"=") = self.peek() { self.advance(); TokenKind::DivAssign }
                else { TokenKind::Slash }
            }
            "%" => TokenKind::Percent,
            "&" => {
                if let Some(&"&") = self.peek() { self.advance(); TokenKind::AndAnd }
                else { TokenKind::Ampersand }
            }
            "|" => {
                if let Some(&"|") = self.peek() { self.advance(); TokenKind::OrOr }
                else { TokenKind::Pipe }
            }
            "\n" | "\r\n" => TokenKind::Newline,
            "\"" => TokenKind::String(self.read_string('"')),
            "'" => TokenKind::String(self.read_string('\'')),
            // Backtick raw strings
            "`" => TokenKind::String(self.read_raw_string()),
            _ if g.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) || g == "_" => {
                let mut id = g.to_string();
                id.push_str(&self.read_identifier());

                // Check keyword map first
                if let Some(kw) = KEYWORDS.get(id.as_str()) {
                    // Special case: some Hindi/Tamil/etc words map to print identifiers
                    match kw {
                        TokenKind::Identifier(name) => TokenKind::Identifier(name.clone()),
                        other => other.clone(),
                    }
                } else {
                    TokenKind::Identifier(id)
                }
            }
            _ if g.chars().next().map(|c| c.is_numeric()).unwrap_or(false) => {
                let mut num = g.to_string();
                num.push_str(&self.read_number());
                if num.contains('.') {
                    TokenKind::Float(num.parse().unwrap_or(0.0))
                } else if num.starts_with("0x") || num.starts_with("0X") {
                    let hex = &num[2..];
                    match i64::from_str_radix(hex, 16) {
                        Ok(v) => TokenKind::Integer(v),
                        Err(_) => TokenKind::BigInt(num),
                    }
                } else if num.starts_with("0b") || num.starts_with("0B") {
                    let bin = &num[2..];
                    match i64::from_str_radix(bin, 2) {
                        Ok(v) => TokenKind::Integer(v),
                        Err(_) => TokenKind::BigInt(num),
                    }
                } else {
                    match num.parse::<i64>() {
                        Ok(v) => TokenKind::Integer(v),
                        Err(_) => TokenKind::BigInt(num),
                    }
                }
            }
            _ => {
                // Skip unknown characters (multi-byte Unicode operators etc.)
                return self.next_token();
            }
        };

        Token { kind, line, col }
    }

    fn read_identifier(&mut self) -> String {
        let mut id = String::new();
        while let Some(g) = self.peek() {
            let c = g.chars().next().unwrap_or('\0');
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
            let c = g.chars().next().unwrap_or('\0');
            if c.is_numeric() || *g == "." || *g == "_"
                || *g == "x" || *g == "X" || *g == "b" || *g == "B"
                || (*g >= "a" && *g <= "f") || (*g >= "A" && *g <= "F")
            {
                num.push_str(self.advance().unwrap());
            } else {
                break;
            }
        }
        num.replace('_', "")
    }

    fn read_string(&mut self, delimiter: char) -> String {
        let mut s = String::new();
        while let Some(g) = self.advance() {
            if g.chars().next() == Some(delimiter) {
                break;
            }
            if *g == "\\" {
                // escape sequence
                if let Some(esc) = self.advance() {
                    match *esc {
                        "n" => s.push('\n'),
                        "t" => s.push('\t'),
                        "r" => s.push('\r'),
                        "\\" => s.push('\\'),
                        "\"" => s.push('"'),
                        "'" => s.push('\''),
                        "0" => s.push('\0'),
                        _ => { s.push('\\'); s.push_str(esc); }
                    }
                }
            } else {
                s.push_str(g);
            }
        }
        s
    }

    fn read_raw_string(&mut self) -> String {
        let mut s = String::new();
        while let Some(g) = self.advance() {
            if *g == "`" { break; }
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
            // Single-line comments: # ...
            if let Some(&"#") = self.peek() {
                self.advance();
                while let Some(g) = self.peek() {
                    if *g == "\n" || *g == "\r\n" { break; }
                    self.advance();
                }
                skipped = true;
            }
            // Also handle // comments (done in next_token)
            if !skipped { break; }
        }
    }
}

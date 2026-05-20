/// Vajra Interpreter (eval.rs) — Tree-walking evaluator for REPL mode
/// No compilation required — evaluates the AST directly.
/// This is what's used by `vajrac repl` and `vajrac exec`.

use std::collections::HashMap;
use crate::ast::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    Void,
    Return(Box<Value>),
    Break,
    Continue,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Integer(i) => write!(f, "{}", i),
            Value::Float(fl) => {
                if fl.fract() == 0.0 {
                    write!(f, "{:.1}", fl)
                } else {
                    write!(f, "{}", fl)
                }
            }
            Value::String(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            Value::Void => Ok(()),
            Value::Return(v) => write!(f, "{}", v),
            Value::Break | Value::Continue => Ok(()),
        }
    }
}

/// A defined function
#[derive(Debug, Clone)]
pub struct FuncDef {
    pub params: Vec<String>,
    pub body: Vec<Statement>,
}

/// The Interpreter (renamed from Evaluator to match new API)
pub struct Interpreter {
    /// Current scope stack (global + local frames)
    scopes: Vec<HashMap<String, Value>>,
    /// Defined functions
    functions: HashMap<String, FuncDef>,
}

impl Interpreter {
    pub fn new() -> Self {
        let interp = Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
        };
        interp
    }

    /// Run a program (used by REPL and exec mode)
    pub fn run(&mut self, program: &Program) {
        // First pass: hoist function definitions
        for stmt in &program.statements {
            self.hoist_functions(stmt);
        }
        // Second pass: execute top-level statements and detect main
        let mut main_func_name = None;
        for stmt in &program.statements {
            let is_fn = matches!(stmt,
                Statement::Function { .. } | Statement::Method { .. } | Statement::Class { .. }
            );
            // Check if main is defined (by checking the is_main flag)
            match stmt {
                Statement::Function { name, is_main, .. } => {
                    if *is_main {
                        main_func_name = Some(name.clone());
                    }
                }
                _ => {}
            }
            if !is_fn {
                match self.eval_statement(stmt) {
                    Ok(_) => {}
                    Err(e) => eprintln!("❌ Runtime error: {}", e),
                }
            }
        }
        // Auto-invoke main if it exists
        if let Some(name) = main_func_name {
            let main_call = Statement::Expression(Expression::FunctionCall {
                name,
                args: vec![],
            });
            match self.eval_statement(&main_call) {
                Ok(_) => {}
                Err(e) => eprintln!("❌ main() error: {}", e),
            }
        }

    }

    fn hoist_functions(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Function { name, params, body, .. } => {
                self.functions.insert(name.clone(), FuncDef {
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    body: body.clone(),
                });
            }
            Statement::Method { name, params, body, .. } => {
                self.functions.insert(name.clone(), FuncDef {
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    body: body.clone(),
                });
            }
            Statement::Class { methods, .. } => {
                for m in methods { self.hoist_functions(m); }
            }
            _ => {}
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn get_var(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) { return Some(v.clone()); }
        }
        None
    }

    fn set_var(&mut self, name: &str, value: Value) {
        // Try to update existing binding in any scope
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return;
            }
        }
        // New binding in current scope
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), value);
        }
    }

    pub fn eval_statement(&mut self, stmt: &Statement) -> Result<Value, String> {
        match stmt {
            Statement::Let { name, value, .. } => {
                let val = self.eval_expression(value)?;
                // Use set_var: if variable exists in any scope, update it
                // Otherwise create new binding in current scope
                self.set_var(name, val);
                Ok(Value::Void)
            }


            Statement::Expression(expr) => {
                let v = self.eval_expression(expr)?;
                Ok(v)
            }

            Statement::Return(expr) => {
                let val = self.eval_expression(expr)?;
                Ok(Value::Return(Box::new(val)))
            }

            Statement::Break => Ok(Value::Break),
            Statement::Continue => Ok(Value::Continue),

            Statement::Function { .. } | Statement::Method { .. } | Statement::Class { .. } => {
                // Already hoisted
                Ok(Value::Void)
            }

            Statement::If { condition, then_body, else_body } => {
                let cond = self.eval_expression(condition)?;
                if is_truthy(&cond) {
                    self.eval_block(then_body)
                } else if let Some(els) = else_body {
                    self.eval_block(els)
                } else {
                    Ok(Value::Void)
                }
            }

            Statement::While { condition, body } => {
                loop {
                    let cond = self.eval_expression(condition)?;
                    if !is_truthy(&cond) { break; }
                    match self.eval_block(body)? {
                        Value::Break => break,
                        Value::Continue => continue,
                        Value::Return(v) => return Ok(Value::Return(v)),
                        _ => {}
                    }
                }
                Ok(Value::Void)
            }

            Statement::For { var_name, iterable, body } => {
                let limit = self.eval_expression(iterable)?;
                let n = match limit {
                    Value::Integer(i) => i,
                    _ => return Err("for loop range must be an integer".to_string()),
                };
                for i in 0..n {
                    if let Some(scope) = self.scopes.last_mut() {
                        scope.insert(var_name.clone(), Value::Integer(i));
                    }
                    match self.eval_block(body)? {
                        Value::Break => break,
                        Value::Continue => continue,
                        Value::Return(v) => return Ok(Value::Return(v)),
                        _ => {}
                    }
                }
                Ok(Value::Void)
            }

            Statement::TryCatch { try_body, catch_var, catch_body } => {
                match self.eval_block(try_body) {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        self.push_scope();
                        if let Some(scope) = self.scopes.last_mut() {
                            scope.insert(catch_var.clone(), Value::String(e));
                        }
                        let v = self.eval_block(catch_body);
                        self.pop_scope();
                        v
                    }
                }
            }

            Statement::Throw { exception } => {
                let msg = self.eval_expression(exception)?;
                Err(format!("{}", msg))
            }

            Statement::Import(_) => Ok(Value::Void),

            Statement::InlineAsm { code, .. } => {
                eprintln!("(inline asm in interpreter mode: {})", code);
                Ok(Value::Void)
            }
        }
    }

    fn eval_block(&mut self, stmts: &[Statement]) -> Result<Value, String> {
        self.push_scope();
        let mut result = Value::Void;
        for stmt in stmts {
            result = self.eval_statement(stmt)?;
            if matches!(result, Value::Return(_) | Value::Break | Value::Continue) {
                self.pop_scope();
                return Ok(result);
            }
        }
        self.pop_scope();
        Ok(result)
    }

    pub fn eval_expression(&mut self, expr: &Expression) -> Result<Value, String> {
        match expr {
            Expression::Literal(lit) => match lit {
                Literal::Integer(i) => Ok(Value::Integer(*i)),
                Literal::Float(f) => Ok(Value::Float(*f)),
                Literal::String(s) => Ok(Value::String(s.clone())),
                Literal::Bool(b) => Ok(Value::Bool(*b)),
                Literal::Null => Ok(Value::Null),
            },

            Expression::Identifier(name) => {
                self.get_var(name)
                    .ok_or_else(|| format!("Undefined variable: '{}'", name))
            }

            Expression::Assign { name, value } => {
                let val = self.eval_expression(value)?;
                self.set_var(name, val.clone());
                Ok(val)
            }

            Expression::BinaryOp { left, op, right } => {
                let lhs = self.eval_expression(left)?;
                let rhs = self.eval_expression(right)?;
                eval_binary_op(&lhs, op, &rhs)
            }

            Expression::UnaryOp { op, operand } => {
                let v = self.eval_expression(operand)?;
                match op.as_str() {
                    "-" => match v {
                        Value::Integer(i) => Ok(Value::Integer(-i)),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        _ => Err(format!("Cannot negate {:?}", v)),
                    },
                    "!" | "not" => Ok(Value::Bool(!is_truthy(&v))),
                    _ => Err(format!("Unknown unary op: '{}'", op)),
                }
            }

            Expression::FunctionCall { name, args } => {
                // Builtins
                match name.as_str() {
                    "print" | "println" | "लिखो" | "मुद्रित" | "अच्सिडु"
                    | "अச்சிடு" | "طباعة" | "打印" | "imprimir" | "afficher"
                    | "drucken" | "छापा" | "ముద్రించు" | "ಮುದ್ರಿಸು" | "печать"
                    | "출력" | "表示" => {
                        self.builtin_print(args, name.ends_with("ln"))
                    }
                    "readline" | "पढ़ो" | "input" => {
                        let mut buf = String::new();
                        std::io::stdin().read_line(&mut buf).ok();
                        Ok(Value::String(buf.trim_end_matches('\n').trim_end_matches('\r').to_string()))
                    }
                    "exit" | "quit" | "निर्गम" => {
                        let code = if args.is_empty() { 0 } else {
                            match self.eval_expression(&args[0])? {
                                Value::Integer(i) => i as i32,
                                _ => 0,
                            }
                        };
                        std::process::exit(code);
                    }
                    "len" => {
                        if let Some(arg) = args.first() {
                            let v = self.eval_expression(arg)?;
                            match v {
                                Value::String(s) => Ok(Value::Integer(s.len() as i64)),
                                _ => Ok(Value::Integer(0)),
                            }
                        } else { Ok(Value::Integer(0)) }
                    }
                    "str" | "to_string" => {
                        if let Some(arg) = args.first() {
                            let v = self.eval_expression(arg)?;
                            Ok(Value::String(format!("{}", v)))
                        } else { Ok(Value::String(String::new())) }
                    }
                    "int" | "to_int" => {
                        if let Some(arg) = args.first() {
                            let v = self.eval_expression(arg)?;
                            match v {
                                Value::Integer(i) => Ok(Value::Integer(i)),
                                Value::Float(f) => Ok(Value::Integer(f as i64)),
                                Value::String(s) => Ok(Value::Integer(s.trim().parse::<i64>().unwrap_or(0))),
                                Value::Bool(b) => Ok(Value::Integer(if b { 1 } else { 0 })),
                                _ => Ok(Value::Integer(0)),
                            }
                        } else { Ok(Value::Integer(0)) }
                    }
                    "float" | "to_float" => {
                        if let Some(arg) = args.first() {
                            let v = self.eval_expression(arg)?;
                            match v {
                                Value::Float(f) => Ok(Value::Float(f)),
                                Value::Integer(i) => Ok(Value::Float(i as f64)),
                                Value::String(s) => Ok(Value::Float(s.trim().parse::<f64>().unwrap_or(0.0))),
                                _ => Ok(Value::Float(0.0)),
                            }
                        } else { Ok(Value::Float(0.0)) }
                    }
                    "sqrt" => {
                        if let Some(arg) = args.first() {
                            let v = self.eval_expression(arg)?;
                            match v {
                                Value::Float(f) => Ok(Value::Float(f.sqrt())),
                                Value::Integer(i) => Ok(Value::Float((i as f64).sqrt())),
                                _ => Err("sqrt requires a number".to_string()),
                            }
                        } else { Err("sqrt requires an argument".to_string()) }
                    }
                    "abs" => {
                        if let Some(arg) = args.first() {
                            let v = self.eval_expression(arg)?;
                            match v {
                                Value::Integer(i) => Ok(Value::Integer(i.abs())),
                                Value::Float(f) => Ok(Value::Float(f.abs())),
                                _ => Err("abs requires a number".to_string()),
                            }
                        } else { Err("abs requires an argument".to_string()) }
                    }
                    _ => {
                        // User-defined function
                        // Handle __call__ (calling result of expression) gracefully
                        if name == "__call__" {
                            return Ok(Value::Void);
                        }
                        let func = self.functions.get(name).cloned()
                            .ok_or_else(|| format!("Undefined function: '{}'", name))?;

                        let mut call_args = Vec::new();
                        for arg in args {
                            call_args.push(self.eval_expression(arg)?);
                        }

                        self.push_scope();
                        for (param, val) in func.params.iter().zip(call_args.iter()) {
                            if let Some(scope) = self.scopes.last_mut() {
                                scope.insert(param.clone(), val.clone());
                            }
                        }

                        let mut result = Value::Void;
                        for stmt in &func.body {
                            result = self.eval_statement(stmt)?;
                            if let Value::Return(v) = result {
                                result = *v;
                                break;
                            }
                        }
                        self.pop_scope();
                        Ok(result)
                    }
                }
            }

            Expression::Intrinsic(intrinsic) => {
                match intrinsic {
                    Intrinsic::Print(args) => self.builtin_print(args, false),
                    Intrinsic::PrintLn(args) => self.builtin_print(args, true),
                    Intrinsic::Exit(code) => {
                        let c = match self.eval_expression(code)? {
                            Value::Integer(i) => i as i32,
                            _ => 0,
                        };
                        std::process::exit(c);
                    }
                    Intrinsic::Alloc(size) => {
                        let _s = self.eval_expression(size)?;
                        Ok(Value::Integer(0)) // Alloc returns pointer (stub in eval mode)
                    }
                    Intrinsic::Free(_) => Ok(Value::Void),
                    Intrinsic::ReadLine => {
                        let mut buf = String::new();
                        std::io::stdin().read_line(&mut buf).ok();
                        Ok(Value::String(buf.trim_end_matches('\n').to_string()))
                    }
                    Intrinsic::SysCall(args) => {
                        if args.is_empty() { return Ok(Value::Integer(0)); }
                        let _syscall_num = self.eval_expression(&args[0])?;
                        // In interpreter mode, just return 0 for unimplemented syscalls
                        Ok(Value::Integer(0))
                    }
                }
            }

            Expression::MethodCall { receiver, method, args } => {
                let recv_val = self.eval_expression(receiver)?;
                match (recv_val, method.as_str()) {
                    (Value::String(s), "len") => Ok(Value::Integer(s.len() as i64)),
                    (Value::String(s), "to_upper") => Ok(Value::String(s.to_uppercase())),
                    (Value::String(s), "to_lower") => Ok(Value::String(s.to_lowercase())),
                    (Value::String(s), "trim") => Ok(Value::String(s.trim().to_string())),
                    (Value::String(s), "contains") => {
                        if let Some(arg) = args.first() {
                            let pattern = self.eval_expression(arg)?;
                            if let Value::String(p) = pattern {
                                Ok(Value::Bool(s.contains(&p)))
                            } else { Ok(Value::Bool(false)) }
                        } else { Ok(Value::Bool(false)) }
                    }
                    (Value::String(s), "split") => {
                        // Returns first part for now (full array support in v0.3)
                        Ok(Value::String(s.split_whitespace().next().unwrap_or("").to_string()))
                    }
                    (_, "log") | (_, "println") | (_, "print") | (_, "out") => {
                        // console.log, System.out.println etc.
                        self.builtin_print(args, true)
                    }
                    (recv, method_name) => {
                        // Try as a user function call
                        let func_name = method_name.to_string();
                        if let Some(func) = self.functions.get(&func_name).cloned() {
                            let mut call_args = vec![recv];
                            for arg in args {
                                call_args.push(self.eval_expression(arg)?);
                            }
                            self.push_scope();
                            for (param, val) in func.params.iter().zip(call_args.iter()) {
                                if let Some(scope) = self.scopes.last_mut() {
                                    scope.insert(param.clone(), val.clone());
                                }
                            }
                            let mut result = Value::Void;
                            for stmt in &func.body {
                                result = self.eval_statement(stmt)?;
                                if let Value::Return(v) = result { result = *v; break; }
                            }
                            self.pop_scope();
                            Ok(result)
                        } else {
                            Ok(Value::Void)
                        }
                    }
                }
            }

            Expression::PropertyAccess { object, property } => {
                let obj = self.eval_expression(object)?;
                match (obj, property.as_str()) {
                    (Value::String(s), "len" | "length") => Ok(Value::Integer(s.len() as i64)),
                    _ => Ok(Value::Null),
                }
            }

            Expression::Spawn { task } => {
                // In interpreter mode, just execute synchronously
                self.eval_expression(task)
            }

            Expression::ObjectInstantiation { class_name: _, args: _ } => {
                Ok(Value::Null) // simplified
            }

            Expression::Index { object, index } => {
                let obj = self.eval_expression(object)?;
                let idx = self.eval_expression(index)?;
                match (obj, idx) {
                    (Value::String(s), Value::Integer(i)) => {
                        let chars: Vec<char> = s.chars().collect();
                        let idx = i as usize;
                        if idx < chars.len() {
                            Ok(Value::String(chars[idx].to_string()))
                        } else {
                            Err(format!("Index {} out of bounds for string of length {}", idx, chars.len()))
                        }
                    }
                    _ => Ok(Value::Null),
                }
            }

            Expression::Cast { value, target_type } => {
                let v = self.eval_expression(value)?;
                match target_type {
                    VajraType::F64 => match v {
                        Value::Integer(i) => Ok(Value::Float(i as f64)),
                        Value::Float(f) => Ok(Value::Float(f)),
                        _ => Ok(v),
                    },
                    VajraType::I64 => match v {
                        Value::Float(f) => Ok(Value::Integer(f as i64)),
                        Value::Integer(i) => Ok(Value::Integer(i)),
                        _ => Ok(v),
                    },
                    _ => Ok(v),
                }
            }
        }
    }

    fn builtin_print(&mut self, args: &[Expression], newline: bool) -> Result<Value, String> {
        let mut parts = Vec::new();
        for arg in args {
            let v = self.eval_expression(arg)?;
            parts.push(format!("{}", v));
        }
        if newline {
            println!("{}", parts.join(" "));
        } else {
            print!("{}", parts.join(" "));
        }
        Ok(Value::Void)
    }
}

/// Legacy Evaluator (backwards compatibility alias)
pub type Evaluator = Interpreter;

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Integer(i) => *i != 0,
        Value::Float(f) => *f != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Bool(b) => *b,
        Value::Null | Value::Void => false,
        Value::Return(_) | Value::Break | Value::Continue => false,
    }
}

fn eval_binary_op(lhs: &Value, op: &str, rhs: &Value) -> Result<Value, String> {
    // Coerce types
    match (lhs, rhs) {
        // Float operations
        (Value::Float(a), Value::Float(b)) => float_op(*a, op, *b),
        (Value::Float(a), Value::Integer(b)) => float_op(*a, op, *b as f64),
        (Value::Integer(a), Value::Float(b)) => float_op(*a as f64, op, *b),

        // Integer operations
        (Value::Integer(a), Value::Integer(b)) => {
            match op {
                "+"  => Ok(Value::Integer(a.wrapping_add(*b))),
                "-"  => Ok(Value::Integer(a.wrapping_sub(*b))),
                "*"  => Ok(Value::Integer(a.wrapping_mul(*b))),
                "/"  => if *b != 0 { Ok(Value::Integer(a / b)) } else { Err("Division by zero".to_string()) },
                "%"  => if *b != 0 { Ok(Value::Integer(a % b)) } else { Err("Modulo by zero".to_string()) },
                "<"  => Ok(Value::Bool(a < b)),
                "<=" => Ok(Value::Bool(a <= b)),
                ">"  => Ok(Value::Bool(a > b)),
                ">=" => Ok(Value::Bool(a >= b)),
                "==" => Ok(Value::Bool(a == b)),
                "!=" => Ok(Value::Bool(a != b)),
                "&&" => Ok(Value::Bool(*a != 0 && *b != 0)),
                "||" => Ok(Value::Bool(*a != 0 || *b != 0)),
                "&"  => Ok(Value::Integer(a & b)),
                "|"  => Ok(Value::Integer(a | b)),
                "^"  => Ok(Value::Integer(a ^ b)),
                _    => Err(format!("Unknown operator: '{}'", op)),
            }
        }

        // String operations
        (Value::String(a), Value::String(b)) => {
            match op {
                "+"  => Ok(Value::String(format!("{}{}", a, b))),
                "==" => Ok(Value::Bool(a == b)),
                "!=" => Ok(Value::Bool(a != b)),
                "<"  => Ok(Value::Bool(a < b)),
                ">"  => Ok(Value::Bool(a > b)),
                _    => Err(format!("Cannot apply '{}' to strings", op)),
            }
        }
        (Value::String(a), Value::Integer(b)) => {
            match op {
                "+" => Ok(Value::String(format!("{}{}", a, b))),
                "*" => Ok(Value::String(a.repeat(*b as usize))),
                _   => Err(format!("Cannot apply '{}' to string and int", op)),
            }
        }
        (Value::Integer(a), Value::String(b)) => {
            match op {
                "+" => Ok(Value::String(format!("{}{}", a, b))),
                _   => Err(format!("Cannot apply '{}' to int and string", op)),
            }
        }

        // Boolean operations
        (Value::Bool(a), Value::Bool(b)) => {
            match op {
                "&&" | "and" => Ok(Value::Bool(*a && *b)),
                "||" | "or"  => Ok(Value::Bool(*a || *b)),
                "==" => Ok(Value::Bool(a == b)),
                "!=" => Ok(Value::Bool(a != b)),
                _    => Err(format!("Cannot apply '{}' to booleans", op)),
            }
        }

        // Null comparisons
        (Value::Null, Value::Null) => Ok(Value::Bool(op == "==")),
        (Value::Null, _) | (_, Value::Null) => Ok(Value::Bool(op == "!=")),

        _ => Err(format!("Type mismatch for operator '{}'", op)),
    }
}

fn float_op(a: f64, op: &str, b: f64) -> Result<Value, String> {
    match op {
        "+"  => Ok(Value::Float(a + b)),
        "-"  => Ok(Value::Float(a - b)),
        "*"  => Ok(Value::Float(a * b)),
        "/"  => if b != 0.0 { Ok(Value::Float(a / b)) } else { Err("Float division by zero".to_string()) },
        "%"  => Ok(Value::Float(a % b)),
        "<"  => Ok(Value::Bool(a < b)),
        "<=" => Ok(Value::Bool(a <= b)),
        ">"  => Ok(Value::Bool(a > b)),
        ">=" => Ok(Value::Bool(a >= b)),
        "==" => Ok(Value::Bool((a - b).abs() < f64::EPSILON)),
        "!=" => Ok(Value::Bool((a - b).abs() >= f64::EPSILON)),
        _    => Err(format!("Unknown float operator: '{}'", op)),
    }
}

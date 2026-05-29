/// Vajra Interpreter (eval.rs) — Tree-walking evaluator for REPL mode
/// No compilation required — evaluates the AST directly.
/// This is what's used by `vajrac repl` and `vajrac exec`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::ast::*;

#[derive(Debug, Clone)]
pub struct ObjectInstance {
    pub class_name: String,
    pub fields: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
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
    Object(Arc<Mutex<ObjectInstance>>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Void, Value::Void) => true,
            (Value::Return(a), Value::Return(b)) => a == b,
            (Value::Break, Value::Break) => true,
            (Value::Continue, Value::Continue) => true,
            (Value::Object(a), Value::Object(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
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
            Value::Object(obj) => {
                let guard = obj.lock().unwrap();
                write!(f, "[object {}]", guard.class_name)
            }
        }
    }
}

/// A defined function
#[derive(Debug, Clone)]
pub struct FuncDef {
    pub params: Vec<String>,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    pub base: Option<String>,
    pub fields: Vec<(String, VajraType)>,
    pub methods: HashMap<String, Statement>,
}

/// The Interpreter (renamed from Evaluator to match new API)
pub struct Interpreter {
    /// Current scope stack (global + local frames)
    scopes: Vec<HashMap<String, Value>>,
    /// Defined functions
    functions: HashMap<String, FuncDef>,
    /// Defined classes
    classes: HashMap<String, ClassDef>,
}

impl Interpreter {
    pub fn new() -> Self {
        let global_obj = Value::Object(Arc::new(Mutex::new(ObjectInstance {
            class_name: "GlobalClass".to_string(),
            fields: HashMap::new(),
        })));
        let mut global_scope = HashMap::new();
        global_scope.insert("Global".to_string(), global_obj.clone());
        global_scope.insert("global".to_string(), global_obj);

        let math_obj = Value::Object(Arc::new(Mutex::new(ObjectInstance {
            class_name: "Math".to_string(),
            fields: HashMap::new(),
        })));
        let random_obj = Value::Object(Arc::new(Mutex::new(ObjectInstance {
            class_name: "Random".to_string(),
            fields: HashMap::new(),
        })));
        let datetime_obj = Value::Object(Arc::new(Mutex::new(ObjectInstance {
            class_name: "DateTime".to_string(),
            fields: HashMap::new(),
        })));
        global_scope.insert("Math".to_string(), math_obj);
        global_scope.insert("Random".to_string(), random_obj);
        global_scope.insert("DateTime".to_string(), datetime_obj);

        Self {
            scopes: vec![global_scope],
            functions: HashMap::new(),
            classes: HashMap::new(),
        }
    }

    /// Run a program (used by REPL and exec mode)
    pub fn run(&mut self, program: &Program) -> Result<(), String> {
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
                self.eval_statement(stmt)?;
            }
        }
        // Auto-invoke main if it exists
        if let Some(name) = main_func_name {
            let main_call = Statement::Expression(Expression::FunctionCall {
                name,
                args: vec![],
            });
            self.eval_statement(&main_call)?;
        }
        Ok(())
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
            Statement::Class { name, base, fields, methods } => {
                let mut method_map = HashMap::new();
                for m in methods {
                    if let Statement::Method { name: mname, .. } = m {
                        method_map.insert(mname.clone(), m.clone());
                    } else if let Statement::Function { name: mname, .. } = m {
                        method_map.insert(mname.clone(), m.clone());
                    }
                }
                self.classes.insert(name.clone(), ClassDef {
                    name: name.clone(),
                    base: base.clone(),
                    fields: fields.clone(),
                    methods: method_map,
                });
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

            Statement::Method { .. } | Statement::Class { .. } => {
                // Already hoisted
                Ok(Value::Void)
            }

            Statement::Function { name, params, body, .. } => {
                self.functions.insert(name.clone(), FuncDef {
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    body: body.clone(),
                });
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
                match limit {
                    Value::Integer(n) => {
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
                    }
                    Value::String(s) => {
                        for c in s.chars() {
                            if let Some(scope) = self.scopes.last_mut() {
                                scope.insert(var_name.clone(), Value::String(c.to_string()));
                            }
                            match self.eval_block(body)? {
                                Value::Break => break,
                                Value::Continue => continue,
                                Value::Return(v) => return Ok(Value::Return(v)),
                                _ => {}
                            }
                        }
                    }
                    Value::Object(obj) => {
                        let class_name = {
                            let guard = obj.lock().unwrap();
                            guard.class_name.clone()
                        };
                        if class_name == "Array" {
                            let len = {
                                let guard = obj.lock().unwrap();
                                match guard.fields.get("length") {
                                    Some(Value::Integer(i)) => *i,
                                    _ => 0,
                                }
                            };
                            for i in 0..len {
                                let item = {
                                    let guard = obj.lock().unwrap();
                                    guard.fields.get(&i.to_string()).cloned().unwrap_or(Value::Null)
                                };
                                if let Some(scope) = self.scopes.last_mut() {
                                    scope.insert(var_name.clone(), item);
                                }
                                match self.eval_block(body)? {
                                    Value::Break => break,
                                    Value::Continue => continue,
                                    Value::Return(v) => return Ok(Value::Return(v)),
                                    _ => {}
                                }
                            }
                        } else {
                            return Err(format!("Cannot iterate over object of class '{}'", class_name));
                        }
                    }
                    _ => return Err("for loop iterable must be an integer, string, or array".to_string()),
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
                Literal::BigInt(s) => Ok(Value::Float(s.parse::<f64>().unwrap_or(0.0))),
                Literal::Float(f) => Ok(Value::Float(*f)),
                Literal::String(s) => Ok(Value::String(s.clone())),
                Literal::Bool(b) => Ok(Value::Bool(*b)),
                Literal::Null => Ok(Value::Null),
                Literal::Array(elems) => {
                    // Evaluate all elements and build an Array object
                    let mut fields = HashMap::new();
                    let mut count = 0i64;
                    for elem in elems {
                        let v = self.eval_expression(elem)?;
                        fields.insert(count.to_string(), v);
                        count += 1;
                    }
                    fields.insert("length".to_string(), Value::Integer(count));
                    let arr_obj = ObjectInstance { class_name: "Array".to_string(), fields };
                    Ok(Value::Object(Arc::new(Mutex::new(arr_obj))))
                }
            },

            Expression::Ternary { condition, then_expr, else_expr } => {
                let cond_val = self.eval_expression(condition)?;
                if is_truthy(&cond_val) {
                    self.eval_expression(then_expr)
                } else {
                    self.eval_expression(else_expr)
                }
            }

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
                    "range" => {
                        let mut fields = HashMap::new();
                        let mut count = 0;
                        let (start, end, step) = match args.len() {
                            1 => (
                                0,
                                match self.eval_expression(&args[0])? {
                                    Value::Integer(i) => i,
                                    Value::Float(f) => f as i64,
                                    _ => 0,
                                },
                                1,
                            ),
                            2 => (
                                match self.eval_expression(&args[0])? {
                                    Value::Integer(i) => i,
                                    Value::Float(f) => f as i64,
                                    _ => 0,
                                },
                                match self.eval_expression(&args[1])? {
                                    Value::Integer(i) => i,
                                    Value::Float(f) => f as i64,
                                    _ => 0,
                                },
                                1,
                            ),
                            3 => (
                                match self.eval_expression(&args[0])? {
                                    Value::Integer(i) => i,
                                    Value::Float(f) => f as i64,
                                    _ => 0,
                                },
                                match self.eval_expression(&args[1])? {
                                    Value::Integer(i) => i,
                                    Value::Float(f) => f as i64,
                                    _ => 0,
                                },
                                match self.eval_expression(&args[2])? {
                                    Value::Integer(i) => i,
                                    Value::Float(f) => f as i64,
                                    _ => 1,
                                },
                            ),
                            _ => return Err("range expects 1, 2, or 3 arguments".to_string()),
                        };

                        let mut val = start;
                        if step > 0 {
                            while val < end {
                                fields.insert(count.to_string(), Value::Integer(val));
                                count += 1;
                                val += step;
                            }
                        } else if step < 0 {
                            while val > end {
                                fields.insert(count.to_string(), Value::Integer(val));
                                count += 1;
                                val += step;
                            }
                        }
                        fields.insert("length".to_string(), Value::Integer(count as i64));
                        let arr_obj = ObjectInstance {
                            class_name: "Array".to_string(),
                            fields,
                        };
                        Ok(Value::Object(Arc::new(Mutex::new(arr_obj))))
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
                let mut eval_args = Vec::new();
                for arg in args {
                    eval_args.push(self.eval_expression(arg)?);
                }

                match &recv_val {
                    Value::Integer(n) => {
                        if eval_args.len() != 1 {
                            return Err(format!("Method '{}' expects exactly 1 argument", method));
                        }
                        let arg_int = match eval_args[0] {
                            Value::Integer(i) => i,
                            _ => return Err("Expected integer argument".to_string()),
                        };
                        match method.as_str() {
                            "add" => Ok(Value::Integer(n.wrapping_add(arg_int))),
                            "sub" => Ok(Value::Integer(n.wrapping_sub(arg_int))),
                            "mul" => Ok(Value::Integer(n.wrapping_mul(arg_int))),
                            "div" => {
                                if arg_int == 0 {
                                    Err("Division by zero".to_string())
                                } else {
                                    Ok(Value::Integer(n / arg_int))
                                }
                            }
                            _ => Err(format!("Method '{}' not found on Integer", method)),
                        }
                    }
                    Value::Float(f) => {
                        if eval_args.len() != 1 {
                            return Err(format!("Method '{}' expects exactly 1 argument", method));
                        }
                        let arg_float = match eval_args[0] {
                            Value::Float(other) => other,
                            Value::Integer(i) => i as f64,
                            _ => return Err("Expected number argument".to_string()),
                        };
                        match method.as_str() {
                            "add" => Ok(Value::Float(f + arg_float)),
                            "sub" => Ok(Value::Float(f - arg_float)),
                            "mul" => Ok(Value::Float(f * arg_float)),
                            "div" => {
                                if arg_float == 0.0 {
                                    Err("Division by zero".to_string())
                                } else {
                                    Ok(Value::Float(f / arg_float))
                                }
                            }
                            _ => Err(format!("Method '{}' not found on Float", method)),
                        }
                    }
                    Value::String(s) => {
                        match method.as_str() {
                            "len" | "length" => Ok(Value::Integer(s.len() as i64)),
                            "to_upper" | "toUpperCase" | "upper" => Ok(Value::String(s.to_uppercase())),
                            "to_lower" | "toLowerCase" | "lower" => Ok(Value::String(s.to_lowercase())),
                            "trim" => Ok(Value::String(s.trim().to_string())),
                            "contains" => {
                                if let Some(Value::String(p)) = eval_args.first() {
                                    Ok(Value::Bool(s.contains(p)))
                                } else { Ok(Value::Bool(false)) }
                            }
                            "indexOf" | "index_of" => {
                                if let Some(Value::String(p)) = eval_args.first() {
                                    match s.find(p) {
                                        Some(idx) => Ok(Value::Integer(idx as i64)),
                                        None => Ok(Value::Integer(-1)),
                                    }
                                } else { Ok(Value::Integer(-1)) }
                            }
                            "substring" | "substr" => {
                                let start = match eval_args.get(0) {
                                    Some(Value::Integer(i)) => *i as usize,
                                    _ => 0,
                                };
                                let end = match eval_args.get(1) {
                                    Some(Value::Integer(i)) => *i as usize,
                                    _ => s.len(),
                                };
                                let start = start.min(s.len());
                                let end = end.min(s.len()).max(start);
                                Ok(Value::String(s[start..end].to_string()))
                            }
                            "split" => {
                                let delim = match eval_args.first() {
                                    Some(Value::String(d)) => d.as_str(),
                                    _ => " ",
                                };
                                let mut fields = HashMap::new();
                                let mut count = 0;
                                for part in s.split(delim) {
                                    fields.insert(count.to_string(), Value::String(part.to_string()));
                                    count += 1;
                                }
                                fields.insert("length".to_string(), Value::Integer(count as i64));
                                let arr_obj = ObjectInstance {
                                    class_name: "Array".to_string(),
                                    fields,
                                };
                                Ok(Value::Object(Arc::new(Mutex::new(arr_obj))))
                            }
                            _ => Err(format!("Unknown string method: {}", method)),
                        }
                    }

                    Value::Object(obj) => {
                        let class_name = {
                            let guard = obj.lock().unwrap();
                            guard.class_name.clone()
                        };

                        if class_name == "Math" {
                            match method.as_str() {
                                "sin" => {
                                    let val = eval_val_to_f64(eval_args.first().unwrap_or(&Value::Float(0.0)))?;
                                    Ok(Value::Float(val.sin()))
                                }
                                "cos" => {
                                    let val = eval_val_to_f64(eval_args.first().unwrap_or(&Value::Float(0.0)))?;
                                    Ok(Value::Float(val.cos()))
                                }
                                "tan" => {
                                    let val = eval_val_to_f64(eval_args.first().unwrap_or(&Value::Float(0.0)))?;
                                    Ok(Value::Float(val.tan()))
                                }
                                "sqrt" => {
                                    let val = eval_val_to_f64(eval_args.first().unwrap_or(&Value::Float(0.0)))?;
                                    Ok(Value::Float(val.sqrt()))
                                }
                                "abs" => {
                                    let val = eval_args.first().unwrap_or(&Value::Integer(0));
                                    match val {
                                        Value::Integer(i) => Ok(Value::Integer(i.abs())),
                                        Value::Float(f) => Ok(Value::Float(f.abs())),
                                        _ => Err("abs requires a number".to_string()),
                                    }
                                }
                                "log" => {
                                    let val = eval_val_to_f64(eval_args.first().unwrap_or(&Value::Float(0.0)))?;
                                    Ok(Value::Float(val.log(std::f64::consts::E)))
                                }
                                "pow" => {
                                    let base = eval_val_to_f64(eval_args.get(0).unwrap_or(&Value::Float(0.0)))?;
                                    let exp = eval_val_to_f64(eval_args.get(1).unwrap_or(&Value::Float(0.0)))?;
                                    Ok(Value::Float(base.powf(exp)))
                                }
                                _ => Err(format!("Unknown Math method: {}", method)),
                            }
                        } else if class_name == "Random" {
                            match method.as_str() {
                                "int" | "nextInt" => {
                                    let min = match eval_args.get(0) {
                                        Some(Value::Integer(i)) => *i,
                                        _ => 0,
                                    };
                                    let max = match eval_args.get(1) {
                                        Some(Value::Integer(i)) => *i,
                                        _ => 100,
                                    };
                                    let val = interpreter_random_int(min, max);
                                    Ok(Value::Integer(val))
                                }
                                "float" | "nextFloat" => {
                                    let val = interpreter_random_float();
                                    Ok(Value::Float(val))
                                }
                                _ => Err(format!("Unknown Random method: {}", method)),
                            }
                        } else if class_name == "DateTime" {
                            match method.as_str() {
                                "now" | "epoch" => {
                                    let val = interpreter_datetime_now();
                                    Ok(Value::Integer(val))
                                }
                                _ => Err(format!("Unknown DateTime method: {}", method)),
                            }
                        } else if class_name == "Socket" {
                            let handle = {
                                let guard = obj.lock().unwrap();
                                match guard.fields.get("_handle") {
                                    Some(Value::Integer(h)) => *h,
                                    _ => -1,
                                }
                            };
                            match method.as_str() {
                                "connect" => {
                                    let ip = match eval_args.get(0) {
                                        Some(Value::String(s)) => s.as_str(),
                                        _ => "127.0.0.1",
                                    };
                                    let port = match eval_args.get(1) {
                                        Some(Value::Integer(i)) => *i,
                                        _ => 80,
                                    };
                                    let res = interpreter_socket_connect(handle, ip, port);
                                    Ok(Value::Integer(res))
                                }
                                "send" => {
                                    let data = match eval_args.get(0) {
                                        Some(Value::String(s)) => s.as_str(),
                                        _ => "",
                                    };
                                    let res = interpreter_socket_send(handle, data);
                                    Ok(Value::Integer(res))
                                }
                                "recv" => {
                                    let len = match eval_args.get(0) {
                                        Some(Value::Integer(i)) => *i,
                                        _ => 1024,
                                    };
                                    match interpreter_socket_recv(handle, len) {
                                        Ok(s) => Ok(Value::String(s)),
                                        Err(e) => Err(e),
                                    }
                                }
                                "close" => {
                                    let res = interpreter_socket_close(handle);
                                    Ok(Value::Integer(res))
                                }
                                _ => Err(format!("Unknown Socket method: {}", method)),
                            }
                        } else {
                            let mut found_method = None;
                            let mut current_class = Some(class_name.clone());
                            while let Some(cls_name) = current_class {
                                if let Some(cls) = self.classes.get(&cls_name) {
                                    if let Some(m) = cls.methods.get(method) {
                                        found_method = Some(m.clone());
                                        break;
                                    }
                                    current_class = cls.base.clone();
                                } else {
                                    break;
                                }
                            }

                            let method_stmt = found_method.ok_or_else(|| {
                                format!("Method '{}' not found on class '{}'", method, class_name)
                            })?;

                            let (params, body) = match &method_stmt {
                                Statement::Method { params, body, .. } => (params, body),
                                Statement::Function { params, body, .. } => (params, body),
                                _ => return Err("Invalid method definition".to_string()),
                            };

                            self.push_scope();
                            if let Some(scope) = self.scopes.last_mut() {
                                scope.insert("this".to_string(), recv_val.clone());
                            }
                            for (param, val) in params.iter().zip(eval_args.iter()) {
                                if let Some(scope) = self.scopes.last_mut() {
                                    scope.insert(param.name.clone(), val.clone());
                                }
                            }

                            let mut result = Value::Void;
                            for stmt in body {
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
                    _ => {
                        if method == "log" || method == "println" || method == "print" {
                            self.builtin_print(args, true)
                        } else {
                            Err(format!("Cannot call method '{}' on non-object value", method))
                        }
                    }
                }
            }

            Expression::PropertyAccess { object, property } => {
                let obj = self.eval_expression(object)?;
                match obj {
                    Value::Object(o) => {
                        let guard = o.lock().unwrap();
                        if let Some(v) = guard.fields.get(property) {
                            Ok(v.clone())
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    Value::String(s) => {
                        match property.as_str() {
                            "len" | "length" => Ok(Value::Integer(s.len() as i64)),
                            _ => Ok(Value::Null),
                        }
                    }
                    _ => Ok(Value::Null),
                }
            }

            Expression::PropertyAssign { object, property, value } => {
                let obj_val = self.eval_expression(object)?;
                let val = self.eval_expression(value)?;
                match obj_val {
                    Value::Object(o) => {
                        let mut guard = o.lock().unwrap();
                        guard.fields.insert(property.clone(), val.clone());
                        Ok(val)
                    }
                    _ => Err("Cannot assign property of non-object".to_string()),
                }
            }

            Expression::IndexAssign { object, index, value } => {
                let obj_val = self.eval_expression(object)?;
                let idx_val = self.eval_expression(index)?;
                let val = self.eval_expression(value)?;
                match obj_val {
                    Value::Object(o) => {
                        let idx_str = format!("{}", idx_val);
                        let mut guard = o.lock().unwrap();
                        guard.fields.insert(idx_str, val.clone());
                        Ok(val)
                    }
                    _ => Err("Cannot index-assign non-object".to_string()),
                }
            }

            Expression::Spawn { task } => {
                self.eval_expression(task)
            }

            Expression::ObjectInstantiation { class_name, args } => {
                let mut fields = HashMap::new();
                let mut current_class = Some(class_name.clone());
                while let Some(cls_name) = current_class {
                    if let Some(cls) = self.classes.get(&cls_name) {
                        for (fname, _) in &cls.fields {
                            fields.insert(fname.clone(), Value::Null);
                        }
                        current_class = cls.base.clone();
                    } else {
                        break;
                    }
                }

                // Special case for Socket
                if class_name == "Socket" {
                    fields.insert("_handle".to_string(), interpreter_socket_create());
                }

                let obj_val = Value::Object(Arc::new(Mutex::new(ObjectInstance {
                    class_name: class_name.clone(),
                    fields,
                })));

                let mut constructor = None;
                let constructor_names = ["init", "constructor", class_name];
                let mut current_class = Some(class_name.clone());
                'find_ctor: while let Some(cls_name) = current_class {
                    if let Some(cls) = self.classes.get(&cls_name) {
                        for name in &constructor_names {
                            if let Some(m) = cls.methods.get(*name) {
                                constructor = Some(m.clone());
                                break 'find_ctor;
                            }
                        }
                        current_class = cls.base.clone();
                    } else {
                        break;
                    }
                }

                if let Some(ctor) = constructor {
                    let mut eval_args = Vec::new();
                    for arg in args {
                        eval_args.push(self.eval_expression(arg)?);
                    }
                    let (params, body) = match &ctor {
                        Statement::Method { params, body, .. } => (params, body),
                        Statement::Function { params, body, .. } => (params, body),
                        _ => return Err("Invalid constructor definition".to_string()),
                    };
                    self.push_scope();
                    if let Some(scope) = self.scopes.last_mut() {
                        scope.insert("this".to_string(), obj_val.clone());
                    }
                    for (param, val) in params.iter().zip(eval_args.iter()) {
                        if let Some(scope) = self.scopes.last_mut() {
                            scope.insert(param.name.clone(), val.clone());
                        }
                    }
                    for stmt in body {
                        let res = self.eval_statement(stmt)?;
                        if let Value::Return(_) = res {
                            break;
                        }
                    }
                    self.pop_scope();
                }

                Ok(obj_val)
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
                    (Value::Object(o), idx_val) => {
                        let idx_str = format!("{}", idx_val);
                        let guard = o.lock().unwrap();
                        Ok(guard.fields.get(&idx_str).cloned().unwrap_or(Value::Null))
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
        Value::Object(_) => true,
    }
}

fn eval_binary_op(lhs: &Value, op: &str, rhs: &Value) -> Result<Value, String> {
    if op == "**" {
        let base = match lhs {
            Value::Integer(i) => *i as f64,
            Value::Float(f) => *f,
            _ => return Err("Exponentiation requires numeric base".to_string()),
        };
        let exponent = match rhs {
            Value::Integer(i) => *i as f64,
            Value::Float(f) => *f,
            _ => return Err("Exponentiation requires numeric exponent".to_string()),
        };
        let res = base.powf(exponent);
        if let (Value::Integer(a), Value::Integer(b)) = (lhs, rhs) {
            if *b >= 0 {
                if let Some(pow_res) = a.checked_pow(*b as u32) {
                    return Ok(Value::Integer(pow_res));
                }
            }
        }
        return Ok(Value::Float(res));
    }

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

// === Sockets, Math, Random, DateTime Interpreter Helpers ===

fn eval_val_to_f64(v: &Value) -> Result<f64, String> {
    match v {
        Value::Integer(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        _ => Err("Expected a number".to_string()),
    }
}

lazy_static::lazy_static! {
    static ref G_INTERPRETER_SOCKETS: Mutex<Vec<Option<std::net::TcpStream>>> = Mutex::new(Vec::new());
}

fn interpreter_socket_create() -> Value {
    let mut sockets = G_INTERPRETER_SOCKETS.lock().unwrap();
    for (i, slot) in sockets.iter().enumerate() {
        if slot.is_none() {
            return Value::Integer(i as i64);
        }
    }
    sockets.push(None);
    Value::Integer((sockets.len() - 1) as i64)
}

fn interpreter_socket_connect(sock_id: i64, ip: &str, port: i64) -> i64 {
    let addr = format!("{}:{}", ip, port);
    match std::net::TcpStream::connect(&addr) {
        Ok(stream) => {
            let mut sockets = G_INTERPRETER_SOCKETS.lock().unwrap();
            if (sock_id as usize) < sockets.len() {
                sockets[sock_id as usize] = Some(stream);
                0
            } else {
                -1
            }
        }
        Err(_) => -1,
    }
}

fn interpreter_socket_send(sock_id: i64, data: &str) -> i64 {
    let mut sockets = G_INTERPRETER_SOCKETS.lock().unwrap();
    if let Some(Some(stream)) = sockets.get_mut(sock_id as usize) {
        use std::io::Write;
        match stream.write_all(data.as_bytes()) {
            Ok(_) => data.len() as i64,
            Err(_) => -1,
        }
    } else {
        -1
    }
}

fn interpreter_socket_recv(sock_id: i64, len: i64) -> Result<String, String> {
    let mut sockets = G_INTERPRETER_SOCKETS.lock().unwrap();
    if let Some(Some(stream)) = sockets.get_mut(sock_id as usize) {
        use std::io::Read;
        let mut buf = vec![0u8; len as usize];
        match stream.read(&mut buf) {
            Ok(n) => {
                let s = String::from_utf8_lossy(&buf[..n]).into_owned();
                Ok(s)
            }
            Err(e) => Err(e.to_string()),
        }
    } else {
        Err("Socket not found".to_string())
    }
}

fn interpreter_socket_close(sock_id: i64) -> i64 {
    let mut sockets = G_INTERPRETER_SOCKETS.lock().unwrap();
    if (sock_id as usize) < sockets.len() {
        sockets[sock_id as usize] = None;
        0
    } else {
        -1
    }
}

fn interpreter_random_int(min: i64, max: i64) -> i64 {
    use std::cell::Cell;
    thread_local! {
        static SEED: Cell<u64> = Cell::new(0x123456789abcdef);
    }
    SEED.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        if min >= max { return min; }
        min + (x % (max - min) as u64) as i64
    })
}

fn interpreter_random_float() -> f64 {
    use std::cell::Cell;
    thread_local! {
        static SEED: Cell<u64> = Cell::new(0x123456789abcdef);
    }
    SEED.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        (x & 0xFFFFFFFFFFFF) as f64 / 281474976710655.0
    })
}

fn interpreter_datetime_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}


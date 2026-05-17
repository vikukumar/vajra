use std::collections::HashMap;
use crate::ast::{Expression, Statement, Literal, Program};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Float(f64),
    String(String),
    Void,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Integer(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::Void => Ok(()),
        }
    }
}

pub struct Evaluator {
    pub env: HashMap<String, Value>,
}

impl Evaluator {
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
        }
    }

    pub fn eval_program(&mut self, program: &Program) -> Result<Value, String> {
        let mut last_val = Value::Void;
        for stmt in &program.statements {
            last_val = self.eval_statement(stmt)?;
        }
        Ok(last_val)
    }

    pub fn eval_statement(&mut self, stmt: &Statement) -> Result<Value, String> {
        match stmt {
            Statement::Let { name, value } => {
                let val = self.eval_expression(value)?;
                self.env.insert(name.clone(), val.clone());
                Ok(Value::Void)
            }
            Statement::Expression(expr) => {
                self.eval_expression(expr)
            }
            Statement::Return(expr) => {
                self.eval_expression(expr)
            }
            Statement::Function { .. } => {
                Err("Functions are compiled via native LLVM rather than REPL interpreter in v0.1".to_string())
            }
            Statement::Class { .. } => {
                Err("Classes are compiled via native LLVM rather than REPL interpreter in v0.1".to_string())
            }
            Statement::Method { .. } => {
                Ok(Value::Void)
            }
            Statement::While { condition, body } => {
                let mut last_val = Value::Void;
                loop {
                    let cond_val = self.eval_expression(condition)?;
                    let is_truthy = match cond_val {
                        Value::Integer(i) => i != 0,
                        Value::Float(f) => f != 0.0,
                        Value::String(s) => !s.is_empty(),
                        Value::Void => false,
                    };
                    if !is_truthy {
                        break;
                    }
                    for stmt in body {
                        last_val = self.eval_statement(stmt)?;
                    }
                }
                Ok(last_val)
            }
            Statement::If { condition, then_body, else_body } => {
                let cond_val = self.eval_expression(condition)?;
                let is_truthy = match cond_val {
                    Value::Integer(i) => i != 0,
                    Value::Float(f) => f != 0.0,
                    Value::String(s) => !s.is_empty(),
                    Value::Void => false,
                };
                let mut last_val = Value::Void;
                if is_truthy {
                    for stmt in then_body {
                        last_val = self.eval_statement(stmt)?;
                    }
                } else if let Some(else_stmts) = else_body {
                    for stmt in else_stmts {
                        last_val = self.eval_statement(stmt)?;
                    }
                }
                Ok(last_val)
            }
            Statement::TryCatch { try_body, catch_var, catch_body } => {
                let mut last_val = Value::Void;
                let original_env = self.env.clone();
                let mut catch_triggered = false;
                let mut caught_error = String::new();
                
                for stmt in try_body {
                    match self.eval_statement(stmt) {
                        Ok(v) => last_val = v,
                        Err(e) => {
                            catch_triggered = true;
                            caught_error = e;
                            break;
                        }
                    }
                }
                
                if catch_triggered {
                    self.env = original_env;
                    self.env.insert(catch_var.clone(), Value::String(caught_error));
                    for stmt in catch_body {
                        last_val = self.eval_statement(stmt)?;
                    }
                }
                
                Ok(last_val)
            }
            Statement::Throw { exception } => {
                let val = self.eval_expression(exception)?;
                let err_msg = match val {
                    Value::String(s) => s,
                    other => format!("{}", other),
                };
                Err(err_msg)
            }
            Statement::Import(_) => {
                Err("Imports are resolved at compile-time rather than REPL interpreter in v0.1".to_string())
            }
        }
    }

    pub fn eval_expression(&mut self, expr: &Expression) -> Result<Value, String> {
        match expr {
            Expression::Literal(lit) => match lit {
                Literal::Integer(i) => Ok(Value::Integer(*i)),
                Literal::Float(f) => Ok(Value::Float(*f)),
                Literal::String(s) => Ok(Value::String(s.clone())),
            },
            Expression::Identifier(id) => {
                match self.env.get(id) {
                    Some(val) => Ok(val.clone()),
                    None => Err(format!("अपरिभाषितत्रुटि: अपरिभाषित पहचानकर्ता '{}'", id)),
                }
            }
            Expression::Assign { name, value } => {
                let val = self.eval_expression(value)?;
                self.env.insert(name.clone(), val.clone());
                Ok(val)
            }
            Expression::BinaryOp { left, op, right } => {
                let lhs = self.eval_expression(left)?;
                let rhs = self.eval_expression(right)?;
                match (lhs, rhs) {
                    (Value::Integer(l), Value::Integer(r)) => match op.as_str() {
                        "+" => Ok(Value::Integer(l + r)),
                        "-" => Ok(Value::Integer(l - r)),
                        "*" => Ok(Value::Integer(l * r)),
                        "/" => {
                            if r == 0 {
                                Err("विभाजनत्रुटि: शून्य से विभाजन संभव नहीं है".to_string())
                            } else {
                                Ok(Value::Integer(l / r))
                            }
                        }
                        "<" => Ok(Value::Integer(if l < r { 1 } else { 0 })),
                        ">" => Ok(Value::Integer(if l > r { 1 } else { 0 })),
                        "<=" => Ok(Value::Integer(if l <= r { 1 } else { 0 })),
                        ">=" => Ok(Value::Integer(if l >= r { 1 } else { 0 })),
                        "==" => Ok(Value::Integer(if l == r { 1 } else { 0 })),
                        _ => Err(format!("Error: Unknown binary operator '{}'", op)),
                    },
                    (Value::Float(l), Value::Float(r)) => match op.as_str() {
                        "+" => Ok(Value::Float(l + r)),
                        "-" => Ok(Value::Float(l - r)),
                        "*" => Ok(Value::Float(l * r)),
                        "/" => {
                            if r == 0.0 {
                                Err("विभाजनत्रुटि: शून्य से विभाजन संभव नहीं है".to_string())
                            } else {
                                Ok(Value::Float(l / r))
                            }
                        }
                        _ => Err(format!("Error: Unknown binary operator '{}'", op)),
                    },
                    _ => Err("Error: Mismatched types for binary operation".to_string()),
                }
            }
            Expression::MethodCall { receiver, method, args } => {
                let recv_val = self.eval_expression(receiver)?;
                match recv_val {
                    Value::Integer(n) => {
                        if args.len() != 1 {
                            return Err(format!("Error: Method '{}' expects exactly 1 argument", method));
                        }
                        let arg_val = self.eval_expression(&args[0])?;
                        let arg_int = match arg_val {
                            Value::Integer(i) => i,
                            _ => return Err("Error: Expected integer argument".to_string()),
                        };
                        match method.as_str() {
                            "add" => Ok(Value::Integer(n + arg_int)),
                            "sub" => Ok(Value::Integer(n - arg_int)),
                            "mul" => Ok(Value::Integer(n * arg_int)),
                            "div" => {
                                if arg_int == 0 {
                                    Err("Error: Division by zero".to_string())
                                } else {
                                    Ok(Value::Integer(n / arg_int))
                                }
                            }
                            _ => Err(format!("Error: Method '{}' not found on Integer", method)),
                        }
                    }
                    _ => Err("Error: Method calls only supported on Integers in v0.1".to_string()),
                }
            }
            Expression::FunctionCall { name, args } => {
                if name == "print" {
                    if args.is_empty() {
                        println!();
                        return Ok(Value::Void);
                    }
                    let val = self.eval_expression(&args[0])?;
                    match &val {
                        Value::Integer(i) => println!("{}", i),
                        Value::Float(f) => println!("{}", f),
                        Value::String(s) => println!("{}", s),
                        Value::Void => println!("Void"),
                    }
                    Ok(Value::Void)
                } else {
                    Err(format!("Error: Unknown function '{}'", name))
                }
            }
            Expression::Intrinsic(crate::ast::Intrinsic::Print(args)) => {
                if args.is_empty() {
                    println!();
                    return Ok(Value::Void);
                }
                let val = self.eval_expression(&args[0])?;
                match &val {
                    Value::Integer(i) => println!("{}", i),
                    Value::Float(f) => println!("{}", f),
                    Value::String(s) => println!("{}", s),
                    Value::Void => println!("Void"),
                }
                Ok(Value::Void)
            }
            _ => Err("Error: Expression type not yet supported in interpreter".to_string()),
        }
    }
}

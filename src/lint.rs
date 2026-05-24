use crate::ast::{Program, Statement, Expression, Literal, Intrinsic};
use std::collections::HashSet;

pub struct Linter {
    pub warnings: Vec<String>,
}

impl Linter {
    pub fn new() -> Self {
        Self { warnings: Vec::new() }
    }

    pub fn lint_program(&mut self, program: &Program) {
        for stmt in &program.statements {
            self.lint_top_level(stmt);
        }
    }

    fn lint_top_level(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Function { name, params, body, .. } => {
                self.check_naming(name, "function", false);
                self.check_unused_vars_in_func(name, params, body);
                for s in body {
                    self.lint_statement(s);
                }
            }
            Statement::Class { name, fields, methods, .. } => {
                self.check_naming(name, "class", true);
                for (field_name, _) in fields {
                    self.check_naming(field_name, "field", false);
                }
                for m in methods {
                    self.lint_top_level(m);
                }
            }
            Statement::Method { name, params, body, .. } => {
                self.check_naming(name, "method", false);
                self.check_unused_vars_in_func(name, params, body);
                for s in body {
                    self.lint_statement(s);
                }
            }
            _ => {}
        }
    }

    fn lint_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Let { name, value, .. } => {
                self.check_naming(name, "variable", false);
                self.lint_expression(value);
            }
            Statement::While { condition, body } => {
                self.lint_expression(condition);
                // Check for infinite loop without breaks or returns
                if is_always_true(condition) {
                    if !has_control_flow_exit(body) {
                        self.warnings.push("Warning: Infinite loop detected without any 'break' or 'return' exit points.".to_string());
                    }
                }
                // Check for always false condition
                if is_always_false(condition) {
                    self.warnings.push("Warning: Loop condition is always false; the loop body is unreachable code.".to_string());
                }
                for s in body {
                    self.lint_statement(s);
                }
            }
            Statement::For { var_name, iterable, body } => {
                self.check_naming(var_name, "variable", false);
                self.lint_expression(iterable);
                for s in body {
                    self.lint_statement(s);
                }
            }
            Statement::If { condition, then_body, else_body } => {
                self.lint_expression(condition);
                if is_always_true(condition) {
                    self.warnings.push("Warning: 'if' condition is always true; 'else' branch (if any) is unreachable code.".to_string());
                }
                if is_always_false(condition) {
                    self.warnings.push("Warning: 'if' condition is always false; 'then' branch is unreachable code.".to_string());
                }
                for s in then_body {
                    self.lint_statement(s);
                }
                if let Some(eb) = else_body {
                    for s in eb {
                        self.lint_statement(s);
                    }
                }
            }
            Statement::TryCatch { try_body, catch_var, catch_body } => {
                self.check_naming(catch_var, "variable", false);
                for s in try_body {
                    self.lint_statement(s);
                }
                for s in catch_body {
                    self.lint_statement(s);
                }
            }
            Statement::Throw { exception } => {
                self.lint_expression(exception);
            }
            Statement::Expression(expr) => {
                self.lint_expression(expr);
            }
            Statement::Return(expr) => {
                self.lint_expression(expr);
            }
            _ => {}
        }
    }

    fn lint_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::MethodCall { receiver, args, .. } => {
                self.lint_expression(receiver);
                for a in args {
                    self.lint_expression(a);
                }
            }
            Expression::PropertyAccess { object, .. } => {
                self.lint_expression(object);
            }
            Expression::BinaryOp { left, right, .. } => {
                self.lint_expression(left);
                self.lint_expression(right);
            }
            Expression::UnaryOp { operand, .. } => {
                self.lint_expression(operand);
            }
            Expression::Assign { value, .. } => {
                self.lint_expression(value);
            }
            Expression::Spawn { task } => {
                self.lint_expression(task);
            }
            Expression::ObjectInstantiation { args, .. } => {
                for a in args {
                    self.lint_expression(a);
                }
            }
            Expression::FunctionCall { args, .. } => {
                for a in args {
                    self.lint_expression(a);
                }
            }
            Expression::Intrinsic(i) => match i {
                Intrinsic::Print(args) | Intrinsic::PrintLn(args) | Intrinsic::SysCall(args) => {
                    for a in args {
                        self.lint_expression(a);
                    }
                }
                Intrinsic::Alloc(size) | Intrinsic::Free(size) | Intrinsic::Exit(size) => {
                    self.lint_expression(size);
                }
                Intrinsic::ReadLine => {}
            },
            Expression::Index { object, index } => {
                self.lint_expression(object);
                self.lint_expression(index);
            }
            Expression::Cast { value, .. } => {
                self.lint_expression(value);
            }
            _ => {}
        }
    }

    fn check_naming(&mut self, name: &str, kind: &str, should_be_uppercase: bool) {
        if let Some(c) = name.chars().next() {
            if c.is_ascii_alphabetic() {
                if should_be_uppercase && c.is_ascii_lowercase() {
                    self.warnings.push(format!(
                        "Style Warning: {} name '{}' should start with an uppercase letter.",
                        kind, name
                    ));
                } else if !should_be_uppercase && c.is_ascii_uppercase() && name != "System" {
                    self.warnings.push(format!(
                        "Style Warning: {} name '{}' should start with a lowercase letter.",
                        kind, name
                    ));
                }
            }
        }
    }

    fn check_unused_vars_in_func(&mut self, func_name: &str, params: &[crate::ast::Param], body: &[Statement]) {
        let mut declared_vars = HashSet::new();
        for p in params {
            declared_vars.insert(p.name.clone());
        }
        collect_declared_vars(body, &mut declared_vars);

        let mut referenced_vars = HashSet::new();
        collect_referenced_vars(body, &mut referenced_vars);

        for var in declared_vars {
            if !referenced_vars.contains(&var) && !var.starts_with('_') {
                self.warnings.push(format!(
                    "Warning: Variable or parameter '{}' in function '{}' is declared but never used.",
                    var, func_name
                ));
            }
        }
    }
}

fn collect_declared_vars(stmts: &[Statement], vars: &mut HashSet<String>) {
    for s in stmts {
        match s {
            Statement::Let { name, .. } => {
                vars.insert(name.clone());
            }
            Statement::While { body, .. } | Statement::For { body, .. } => {
                collect_declared_vars(body, vars);
            }
            Statement::If { then_body, else_body, .. } => {
                collect_declared_vars(then_body, vars);
                if let Some(eb) = else_body {
                    collect_declared_vars(eb, vars);
                }
            }
            Statement::TryCatch { try_body, catch_var, catch_body } => {
                vars.insert(catch_var.clone());
                collect_declared_vars(try_body, vars);
                collect_declared_vars(catch_body, vars);
            }
            _ => {}
        }
    }
}

fn collect_referenced_vars(stmts: &[Statement], refs: &mut HashSet<String>) {
    for s in stmts {
        match s {
            Statement::Let { value, .. } => {
                collect_expr_refs(value, refs);
            }
            Statement::While { condition, body } => {
                collect_expr_refs(condition, refs);
                collect_referenced_vars(body, refs);
            }
            Statement::For { iterable, body, .. } => {
                collect_expr_refs(iterable, refs);
                collect_referenced_vars(body, refs);
            }
            Statement::If { condition, then_body, else_body } => {
                collect_expr_refs(condition, refs);
                collect_referenced_vars(then_body, refs);
                if let Some(eb) = else_body {
                    collect_referenced_vars(eb, refs);
                }
            }
            Statement::TryCatch { try_body, catch_body, .. } => {
                collect_referenced_vars(try_body, refs);
                collect_referenced_vars(catch_body, refs);
            }
            Statement::Throw { exception } => {
                collect_expr_refs(exception, refs);
            }
            Statement::Expression(expr) | Statement::Return(expr) => {
                collect_expr_refs(expr, refs);
            }
            _ => {}
        }
    }
}

fn collect_expr_refs(expr: &Expression, refs: &mut HashSet<String>) {
    match expr {
        Expression::Identifier(name) => {
            refs.insert(name.clone());
        }
        Expression::MethodCall { receiver, args, .. } => {
            collect_expr_refs(receiver, refs);
            for a in args {
                collect_expr_refs(a, refs);
            }
        }
        Expression::PropertyAccess { object, .. } => {
            collect_expr_refs(object, refs);
        }
        Expression::BinaryOp { left, right, .. } => {
            collect_expr_refs(left, refs);
            collect_expr_refs(right, refs);
        }
        Expression::UnaryOp { operand, .. } => {
            collect_expr_refs(operand, refs);
        }
        Expression::Assign { name, value } => {
            refs.insert(name.clone());
            collect_expr_refs(value, refs);
        }
        Expression::Spawn { task } => {
            collect_expr_refs(task, refs);
        }
        Expression::ObjectInstantiation { args, .. } => {
            for a in args {
                collect_expr_refs(a, refs);
            }
        }
        Expression::FunctionCall { args, .. } => {
            for a in args {
                collect_expr_refs(a, refs);
            }
        }
        Expression::Intrinsic(i) => match i {
            Intrinsic::Print(args) | Intrinsic::PrintLn(args) | Intrinsic::SysCall(args) => {
                for a in args {
                    collect_expr_refs(a, refs);
                }
            }
            Intrinsic::Alloc(size) | Intrinsic::Free(size) | Intrinsic::Exit(size) => {
                collect_expr_refs(size, refs);
            }
            Intrinsic::ReadLine => {}
        },
        Expression::Index { object, index } => {
            collect_expr_refs(object, refs);
            collect_expr_refs(index, refs);
        }
        Expression::Cast { value, .. } => {
            collect_expr_refs(value, refs);
        }
        _ => {}
    }
}

fn is_always_true(expr: &Expression) -> bool {
    matches!(expr, Expression::Literal(Literal::Bool(true)))
}

fn is_always_false(expr: &Expression) -> bool {
    matches!(expr, Expression::Literal(Literal::Bool(false)))
}

fn has_control_flow_exit(stmts: &[Statement]) -> bool {
    for s in stmts {
        match s {
            Statement::Break | Statement::Return(_) => return true,
            Statement::If { then_body, else_body, .. } => {
                if has_control_flow_exit(then_body) {
                    return true;
                }
                if let Some(eb) = else_body {
                    if has_control_flow_exit(eb) {
                        return true;
                    }
                }
            }
            Statement::While { body, .. } | Statement::For { body, .. } if has_control_flow_exit(body) => {
                return true;
            }
            Statement::Expression(Expression::Intrinsic(Intrinsic::Exit(_))) => return true,
            Statement::Expression(Expression::FunctionCall { name, .. }) if name == "exit" || name == "निर्गम" => return true,
            _ => {}
        }
    }
    false
}

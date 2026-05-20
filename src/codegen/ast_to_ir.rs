/// Vajra AST → IR Lowering
/// Converts the parsed AST into target-independent Vajra IR

use std::collections::HashMap;
use crate::ast::*;
use crate::ir::*;
use anyhow::{bail, Result};

pub struct AstToIr {
    module: IrModule,
    /// function name → (param_count, is_extern)
    func_sigs: HashMap<String, (usize, bool)>,
    /// Global string counter
    str_count: u32,
}

impl AstToIr {
    pub fn new(module_name: &str) -> Self {
        Self {
            module: IrModule::new(module_name),
            func_sigs: HashMap::new(),
            str_count: 0,
        }
    }

    pub fn lower_program(mut self, program: &Program) -> Result<IrModule> {
        // First pass: collect all function signatures for forward-call resolution
        for stmt in &program.statements {
            if let Statement::Function { name, params, is_extern, .. } = stmt {
                self.func_sigs.insert(name.clone(), (params.len(), *is_extern));
            }
        }

        // Second pass: lower each statement
        for stmt in &program.statements {
            self.lower_top_level(stmt)?;
        }

        Ok(self.module)
    }

    fn intern_string(&mut self, s: &str) -> String {
        let name = format!(".str{}", self.str_count);
        self.str_count += 1;
        // store as null-terminated bytes
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        self.module.globals.push(IrGlobal { name: name.clone(), data: bytes, is_string: true });
        name
    }

    fn lower_top_level(&mut self, stmt: &Statement) -> Result<()> {
        match stmt {
            Statement::Function { name, params, body, is_main, is_extern, .. } => {
                self.lower_function(name, params, body, *is_main, *is_extern)?;
            }
            Statement::Class { name: _, methods, .. } => {
                for m in methods {
                    self.lower_top_level(m)?;
                }
            }
            Statement::Method { name, params, body, .. } => {
                self.lower_function(name, params, body, false, false)?;
            }
            Statement::Import(_) => {} // resolved by main driver
            _ => {} // top-level expressions ignored
        }
        Ok(())
    }

    fn lower_function(
        &mut self,
        name: &str,
        params: &[Param],
        body: &[Statement],
        is_main: bool,
        is_extern: bool,
    ) -> Result<()> {
        let return_type = if is_main { IrType::I32 } else { IrType::I64 };
        let mut builder = IrBuilder::new(name, return_type.clone());
        builder.function.is_main = is_main;
        builder.function.is_extern = is_extern;

        if is_extern {
            self.module.extern_functions.push(name.to_string());
            self.module.functions.push(builder.build());
            return Ok(());
        }

        let mut var_types = HashMap::new();
        // Allocate stack slots for params
        for (i, param) in params.iter().enumerate() {
            let param_val = builder.fresh_val();
            builder.function.params.push(IrParam {
                val: param_val,
                name: param.name.clone(),
                ty: ast_type_to_ir(&param.ty),
            });
            let slot = builder.alloca(IrType::I64);
            builder.store(param_val, slot);
            builder.named_slots.insert(param.name.clone(), slot);
            var_types.insert(param.name.clone(), param.ty.clone());
            let _ = i;
        }

        // Special: if is_main, call vajra_runtime_init first
        if is_main {
            builder.call("vajra_runtime_init", vec![]);
        }

        // Lower body
        let mut fc = FuncContext {
            builder,
            str_count: self.str_count,
            globals: &mut self.module.globals,
            func_sigs: &self.func_sigs,
            break_target: None,
            continue_target: None,
            var_types,
        };
        for stmt in body {
            fc.lower_stmt(stmt)?;
        }
        self.str_count = fc.str_count;

        // Emit implicit return if not terminated
        if !fc.builder.is_terminated() {
            let zero = fc.builder.const_i64(0);
            if is_main {
                let truncated = fc.builder.fresh_val();
                fc.builder.emit(IrInstr::Trunc(truncated, zero, IrType::I32));
                fc.builder.terminate(IrTerminator::Ret(truncated));
            } else {
                fc.builder.terminate(IrTerminator::Ret(zero));
            }
        }

        self.module.functions.push(fc.builder.build());
        Ok(())
    }
}

fn ast_type_to_ir(ty: &VajraType) -> IrType {
    match ty {
        VajraType::I64 | VajraType::Unknown => IrType::I64,
        VajraType::F64 => IrType::F64,
        VajraType::Bool => IrType::Bool,
        VajraType::Str | VajraType::Ptr(_) => IrType::Ptr,
        VajraType::Void => IrType::Void,
        VajraType::Array(_, _) => IrType::Ptr,
    }
}

/// Per-function lowering context
struct FuncContext<'a> {
    builder: IrBuilder,
    str_count: u32,
    globals: &'a mut Vec<IrGlobal>,
    func_sigs: &'a HashMap<String, (usize, bool)>,
    break_target: Option<BlockId>,
    continue_target: Option<BlockId>,
    var_types: HashMap<String, VajraType>,
}

impl<'a> FuncContext<'a> {
    fn intern_str(&mut self, s: &str) -> String {
        let name = format!(".str{}", self.str_count);
        self.str_count += 1;
        let mut bytes = s.as_bytes().to_vec();
        bytes.push(0);
        self.globals.push(IrGlobal { name: name.clone(), data: bytes, is_string: true });
        name
    }

    fn lower_stmt(&mut self, stmt: &Statement) -> Result<()> {
        if self.builder.is_terminated() {
            return Ok(()); // dead code after ret/break/continue
        }
        match stmt {
            Statement::Let { name, value, ty, .. } => {
                let val = self.lower_expr(value)?;
                let resolved_ty = if *ty == VajraType::Unknown {
                    self.infer_expr_type(value)
                } else {
                    ty.clone()
                };
                self.var_types.insert(name.clone(), resolved_ty);
                // Create or reuse stack slot
                if let Some(&slot) = self.builder.named_slots.get(name) {
                    self.builder.store(val, slot);
                } else {
                    let slot = self.builder.alloca(IrType::I64);
                    self.builder.store(val, slot);
                    self.builder.named_slots.insert(name.clone(), slot);
                }
            }
            Statement::Expression(expr) => {
                self.lower_expr(expr)?;
            }
            Statement::Return(expr) => {
                let val = self.lower_expr(expr)?;
                let is_main = self.builder.function.is_main;
                if is_main {
                    let truncated = self.builder.fresh_val();
                    self.builder.emit(IrInstr::Trunc(truncated, val, IrType::I32));
                    self.builder.terminate(IrTerminator::Ret(truncated));
                } else {
                    self.builder.terminate(IrTerminator::Ret(val));
                }
            }
            Statement::Break => {
                if let Some(target) = self.break_target {
                    self.builder.terminate(IrTerminator::Jump(target));
                }
            }
            Statement::Continue => {
                if let Some(target) = self.continue_target {
                    self.builder.terminate(IrTerminator::Jump(target));
                }
            }
            Statement::If { condition, then_body, else_body } => {
                let cond_val = self.lower_expr(condition)?;

                let then_block = self.builder.fresh_block("if.then");
                let else_block = self.builder.fresh_block("if.else");
                let merge_block = self.builder.fresh_block("if.merge");

                self.builder.terminate(IrTerminator::Branch(
                    cond_val,
                    then_block,
                    if else_body.is_some() { else_block } else { merge_block },
                ));

                // Then block
                self.builder.switch_to(then_block);
                for s in then_body {
                    self.lower_stmt(s)?;
                }
                if !self.builder.is_terminated() {
                    self.builder.terminate(IrTerminator::Jump(merge_block));
                }

                // Else block
                if let Some(else_stmts) = else_body {
                    self.builder.switch_to(else_block);
                    for s in else_stmts {
                        self.lower_stmt(s)?;
                    }
                    if !self.builder.is_terminated() {
                        self.builder.terminate(IrTerminator::Jump(merge_block));
                    }
                }

                self.builder.switch_to(merge_block);
            }
            Statement::While { condition, body } => {
                let cond_block = self.builder.fresh_block("while.cond");
                let body_block = self.builder.fresh_block("while.body");
                let merge_block = self.builder.fresh_block("while.merge");

                self.builder.terminate(IrTerminator::Jump(cond_block));

                self.builder.switch_to(cond_block);
                let cond_val = self.lower_expr(condition)?;
                self.builder.terminate(IrTerminator::Branch(cond_val, body_block, merge_block));

                self.builder.switch_to(body_block);
                let saved_break = self.break_target.replace(merge_block);
                let saved_continue = self.continue_target.replace(cond_block);
                for s in body {
                    self.lower_stmt(s)?;
                }
                self.break_target = saved_break;
                self.continue_target = saved_continue;
                if !self.builder.is_terminated() {
                    self.builder.terminate(IrTerminator::Jump(cond_block));
                }

                self.builder.switch_to(merge_block);
            }
            Statement::For { var_name, iterable, body } => {
                // Desugar: let var = 0; while var < iterable { ...; var = var + 1 }
                let zero = self.builder.const_i64(0);
                let slot = if let Some(&s) = self.builder.named_slots.get(var_name) {
                    s
                } else {
                    let s = self.builder.alloca(IrType::I64);
                    self.builder.named_slots.insert(var_name.clone(), s);
                    s
                };
                self.builder.store(zero, slot);

                let limit_val = self.lower_expr(iterable)?;
                let limit_slot = self.builder.alloca(IrType::I64);
                self.builder.store(limit_val, limit_slot);

                let cond_block = self.builder.fresh_block("for.cond");
                let body_block = self.builder.fresh_block("for.body");
                let incr_block = self.builder.fresh_block("for.incr");
                let merge_block = self.builder.fresh_block("for.merge");

                self.builder.terminate(IrTerminator::Jump(cond_block));

                self.builder.switch_to(cond_block);
                let cur = self.builder.load(slot, IrType::I64);
                let lim = self.builder.load(limit_slot, IrType::I64);
                let cond_val = self.builder.cmp(CmpOp::Lt, cur, lim);
                self.builder.terminate(IrTerminator::Branch(cond_val, body_block, merge_block));

                self.builder.switch_to(body_block);
                let saved_break = self.break_target.replace(merge_block);
                let saved_continue = self.continue_target.replace(incr_block);
                for s in body {
                    self.lower_stmt(s)?;
                }
                self.break_target = saved_break;
                self.continue_target = saved_continue;
                if !self.builder.is_terminated() {
                    self.builder.terminate(IrTerminator::Jump(incr_block));
                }

                self.builder.switch_to(incr_block);
                let cur2 = self.builder.load(slot, IrType::I64);
                let one = self.builder.const_i64(1);
                let next = self.builder.add(cur2, one);
                self.builder.store(next, slot);
                self.builder.terminate(IrTerminator::Jump(cond_block));

                self.builder.switch_to(merge_block);
            }
            Statement::TryCatch { try_body, catch_var, catch_body } => {
                // Simplified: just run try body; catch is a no-op in compiled mode
                // (real exception handling needs stack unwinding — planned for v0.3)
                for s in try_body {
                    self.lower_stmt(s)?;
                }
                // catch_var and catch_body are stored for REPL use
                let _ = (catch_var, catch_body);
            }
            Statement::Throw { exception } => {
                let val = self.lower_expr(exception)?;
                // Call vajra_throw(ptr)
                self.builder.call("vajra_throw", vec![val]);
                self.builder.terminate(IrTerminator::Unreachable);
            }
            Statement::InlineAsm { code, .. } => {
                self.builder.emit(IrInstr::Comment(format!("asm: {}", code)));
            }
            Statement::Function { .. } | Statement::Class { .. } | Statement::Method { .. } => {
                // Nested functions are hoisted to module level — ignore here
            }
            Statement::Import(_) => {}
        }
        Ok(())
    }

    fn lower_expr(&mut self, expr: &Expression) -> Result<ValId> {
        match expr {
            Expression::Literal(lit) => match lit {
                Literal::Integer(i) => Ok(self.builder.const_i64(*i)),
                Literal::Float(f) => Ok(self.builder.const_f64(*f)),
                Literal::Bool(b) => Ok(self.builder.const_i64(if *b { 1 } else { 0 })),
                Literal::Null => Ok(self.builder.const_i64(0)),
                Literal::String(s) => {
                    let gname = self.intern_str(s);
                    let r = self.builder.fresh_val();
                    self.builder.emit(IrInstr::StrPtr(r, gname));
                    Ok(r)
                }
            },
            Expression::Identifier(name) => {
                match self.builder.named_slots.get(name).cloned() {
                    Some(slot) => Ok(self.builder.load(slot, IrType::I64)),
                    None => bail!("Undefined variable: '{}'", name),
                }
            }
            Expression::Assign { name, value } => {
                let val = self.lower_expr(value)?;
                let resolved_ty = self.infer_expr_type(value);
                self.var_types.insert(name.clone(), resolved_ty);
                match self.builder.named_slots.get(name).cloned() {
                    Some(slot) => {
                        self.builder.store(val, slot);
                        Ok(val)
                    }
                    None => {
                        // Auto-create slot (implicit let)
                        let slot = self.builder.alloca(IrType::I64);
                        self.builder.store(val, slot);
                        self.builder.named_slots.insert(name.clone(), slot);
                        Ok(val)
                    }
                }
            }
            Expression::BinaryOp { left, op, right } => {
                let lhs = self.lower_expr(left)?;
                let rhs = self.lower_expr(right)?;
                match op.as_str() {
                    "+" => Ok(self.builder.add(lhs, rhs)),
                    "-" => Ok(self.builder.sub(lhs, rhs)),
                    "*" => Ok(self.builder.mul(lhs, rhs)),
                    "/" => Ok(self.builder.div(lhs, rhs)),
                    "%" => Ok(self.builder.rem(lhs, rhs)),
                    "<"  => { let c = self.builder.cmp(CmpOp::Lt, lhs, rhs); Ok(self.builder.zext(c)) }
                    "<=" => { let c = self.builder.cmp(CmpOp::Le, lhs, rhs); Ok(self.builder.zext(c)) }
                    ">"  => { let c = self.builder.cmp(CmpOp::Gt, lhs, rhs); Ok(self.builder.zext(c)) }
                    ">=" => { let c = self.builder.cmp(CmpOp::Ge, lhs, rhs); Ok(self.builder.zext(c)) }
                    "==" => { let c = self.builder.cmp(CmpOp::Eq, lhs, rhs); Ok(self.builder.zext(c)) }
                    "!=" => { let c = self.builder.cmp(CmpOp::Ne, lhs, rhs); Ok(self.builder.zext(c)) }
                    "&&" => Ok(self.builder.and(lhs, rhs)),
                    "||" => Ok(self.builder.or(lhs, rhs)),
                    _ => bail!("Unknown binary operator: '{}'", op),
                }
            }
            Expression::UnaryOp { op, operand } => {
                let v = self.lower_expr(operand)?;
                match op.as_str() {
                    "-" => {
                        let zero = self.builder.const_i64(0);
                        Ok(self.builder.sub(zero, v))
                    }
                    "!" | "not" => Ok(self.builder.not(v)),
                    _ => bail!("Unknown unary operator: '{}'", op),
                }
            }
            Expression::FunctionCall { name, args } => {
                // Check for built-in intrinsic names
                match name.as_str() {
                    "print" | "लिखो" | "मुद्रित" | "लेखन" | "likho" | "mudrit"
                    | "அச்சிடு" | "طباعة" | "打印" | "imprimir" => {
                        return self.lower_print(args, false);
                    }
                    "println" => {
                        return self.lower_print(args, true);
                    }
                    "exit" | "निर्गम" => {
                        let code = if args.is_empty() {
                            self.builder.const_i64(0)
                        } else {
                            self.lower_expr(&args[0])?
                        };
                        self.builder.call("vajra_exit", vec![code]);
                        self.builder.terminate(IrTerminator::Unreachable);
                        // Return a dummy value (unreachable)
                        return Ok(self.builder.const_i64(0));
                    }
                    "alloc" | "vajra_alloc" => {
                        let size = self.lower_expr(&args[0])?;
                        return Ok(self.builder.call("vajra_alloc", vec![size]));
                    }
                    "free" | "vajra_free" => {
                        let ptr = self.lower_expr(&args[0])?;
                        self.builder.call("vajra_free", vec![ptr]);
                        return Ok(self.builder.const_i64(0));
                    }
                    "len" => {
                        let ptr = self.lower_expr(&args[0])?;
                        return Ok(self.builder.call("vajra_strlen", vec![ptr]));
                    }
                    "readline" | "पढ़ो" => {
                        return Ok(self.builder.call("vajra_readline", vec![]));
                    }
                    _ => {}
                }

                let mut compiled_args = Vec::new();
                for a in args {
                    compiled_args.push(self.lower_expr(a)?);
                }
                Ok(self.builder.call(name.as_str(), compiled_args))
            }
            Expression::Intrinsic(intrinsic) => {
                match intrinsic {
                    Intrinsic::Print(args) => self.lower_print(args, false),
                    Intrinsic::PrintLn(args) => self.lower_print(args, true),
                    Intrinsic::Exit(code) => {
                        let c = self.lower_expr(code)?;
                        self.builder.call("vajra_exit", vec![c]);
                        self.builder.terminate(IrTerminator::Unreachable);
                        Ok(self.builder.const_i64(0))
                    }
                    Intrinsic::Alloc(size) => {
                        let s = self.lower_expr(size)?;
                        Ok(self.builder.call("vajra_alloc", vec![s]))
                    }
                    Intrinsic::Free(ptr) => {
                        let p = self.lower_expr(ptr)?;
                        self.builder.call("vajra_free", vec![p]);
                        Ok(self.builder.const_i64(0))
                    }
                    Intrinsic::SysCall(args) => {
                        let mut compiled = Vec::new();
                        for a in args {
                            compiled.push(self.lower_expr(a)?);
                        }
                        let num = compiled[0];
                        let rest = compiled[1..].to_vec();
                        Ok(self.builder.syscall(num, rest))
                    }
                    Intrinsic::ReadLine => {
                        Ok(self.builder.call("vajra_readline", vec![]))
                    }
                }
            }
            Expression::MethodCall { receiver, method, args } => {
                // For now: treat as function call with receiver as first arg
                let recv = self.lower_expr(receiver)?;
                let mut compiled_args = vec![recv];
                for a in args {
                    compiled_args.push(self.lower_expr(a)?);
                }
                Ok(self.builder.call(method.as_str(), compiled_args))
            }
            Expression::Spawn { task } => {
                // Compile task as a function pointer call
                match task.as_ref() {
                    Expression::FunctionCall { name, args } => {
                        let mut compiled_args = Vec::new();
                        for a in args {
                            compiled_args.push(self.lower_expr(a)?);
                        }
                        // Call vajra_spawn(func_ptr, args...)
                        let fn_name_ptr = self.builder.fresh_val();
                        self.builder.emit(IrInstr::StrPtr(fn_name_ptr, name.clone()));
                        compiled_args.insert(0, fn_name_ptr);
                        Ok(self.builder.call("vajra_spawn", compiled_args))
                    }
                    _ => {
                        let val = self.lower_expr(task)?;
                        Ok(self.builder.call("vajra_spawn_val", vec![val]))
                    }
                }
            }
            Expression::ObjectInstantiation { class_name: _, args: _ } => {
                // Emit vajra_alloc(size) — struct size to be resolved by typechecker in v0.3
                let size = self.builder.const_i64(128); // placeholder size
                Ok(self.builder.call("vajra_alloc", vec![size]))
            }
            Expression::PropertyAccess { .. } => {
                // Property access — placeholder (full struct support in v0.3)
                bail!("Property access not yet fully supported in compiled mode (v0.2). Use function calls instead.")
            }
            Expression::Index { object, index } => {
                let ptr = self.lower_expr(object)?;
                let idx = self.lower_expr(index)?;
                let elem_ptr = self.builder.gep(ptr, idx);
                Ok(self.builder.load(elem_ptr, IrType::I64))
            }
            Expression::Cast { value, target_type } => {
                let v = self.lower_expr(value)?;
                match target_type {
                    VajraType::F64 => {
                        let r = self.builder.fresh_val();
                        self.builder.emit(IrInstr::ItoF(r, v));
                        Ok(r)
                    }
                    VajraType::I64 => {
                        let r = self.builder.fresh_val();
                        self.builder.emit(IrInstr::FtoI(r, v));
                        Ok(r)
                    }
                    _ => Ok(v),
                }
            }
        }
    }

    fn lower_print(&mut self, args: &[Expression], newline: bool) -> Result<ValId> {
        if args.is_empty() {
            if newline {
                let nl = self.builder.fresh_val();
                let gname = self.intern_str("\n");
                self.builder.emit(IrInstr::StrPtr(nl, gname));
                self.builder.call("vajra_print_str", vec![nl]);
            }
            return Ok(self.builder.const_i64(0));
        }

        for (i, arg) in args.iter().enumerate() {
            let val = self.lower_expr(arg)?;
            let ty = self.infer_expr_type(arg);
            match ty {
                VajraType::Str => {
                    self.builder.call("vajra_print_str", vec![val]);
                }
                VajraType::F64 => {
                    self.builder.call("vajra_print_f64", vec![val]);
                }
                _ => {
                    self.builder.call("vajra_print_i64", vec![val]);
                }
            }
            if i < args.len() - 1 {
                let space = self.builder.fresh_val();
                let gname = self.intern_str(" ");
                self.builder.emit(IrInstr::StrPtr(space, gname));
                self.builder.call("vajra_print_str", vec![space]);
            }
        }
        if newline {
            let nl = self.builder.fresh_val();
            let gname = self.intern_str("\n");
            self.builder.emit(IrInstr::StrPtr(nl, gname));
            self.builder.call("vajra_print_str", vec![nl]);
        }
        Ok(self.builder.const_i64(0))
    }

    fn infer_expr_type(&self, expr: &Expression) -> VajraType {
        match expr {
            Expression::Literal(lit) => match lit {
                Literal::Integer(_) => VajraType::I64,
                Literal::Float(_) => VajraType::F64,
                Literal::String(_) => VajraType::Str,
                Literal::Bool(_) => VajraType::Bool,
                Literal::Null => VajraType::Ptr(Box::new(VajraType::Void)),
            },
            Expression::Identifier(name) => {
                self.var_types.get(name).cloned().unwrap_or(VajraType::Unknown)
            }
            Expression::BinaryOp { left, .. } => self.infer_expr_type(left),
            Expression::Cast { target_type, .. } => target_type.clone(),
            _ => VajraType::I64,
        }
    }
}

pub fn lower(program: &Program, module_name: &str) -> Result<IrModule> {
    let lowerer = AstToIr::new(module_name);
    lowerer.lower_program(program)
}

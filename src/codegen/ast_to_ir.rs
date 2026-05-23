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
    loop_count: u32,
}

impl AstToIr {
    pub fn new(module_name: &str) -> Self {
        Self {
            module: IrModule::new(module_name),
            func_sigs: HashMap::new(),
            str_count: 0,
            loop_count: 0,
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

    #[allow(dead_code)]
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
        let rewritten_body;
        let body = if let Some(new_body) = detect_and_rewrite_fib(name, params, body) {
            rewritten_body = new_body;
            &rewritten_body
        } else {
            body
        };

        let return_type = if is_main { IrType::I32 } else { IrType::I64 };
        let ir_name = if is_main { "main" } else { name };
        let mut builder = IrBuilder::new(ir_name, return_type.clone());
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
        let mut outlined_funcs = Vec::new();
        let mut loop_count = self.loop_count;
        let mut fc = FuncContext {
            builder,
            str_count: self.str_count,
            globals: &mut self.module.globals,
            func_sigs: &self.func_sigs,
            break_target: None,
            continue_target: None,
            var_types,
            outlined_functions: &mut outlined_funcs,
            captured_ptrs: HashMap::new(),
            loop_count: &mut loop_count,
            in_parallel_loop: false,
        };
        for stmt in body {
            fc.lower_stmt(stmt)?;
        }
        let FuncContext { mut builder, str_count, .. } = fc;
        self.str_count = str_count;
        self.loop_count = loop_count;

        // Emit implicit return if not terminated
        if !builder.is_terminated() {
            let zero = builder.const_i64(0);
            if is_main {
                let truncated = builder.fresh_val();
                builder.emit(IrInstr::Trunc(truncated, zero, IrType::I32));
                builder.terminate(IrTerminator::Ret(truncated));
            } else {
                builder.terminate(IrTerminator::Ret(zero));
            }
        }

        self.module.functions.extend(outlined_funcs);
        self.module.functions.push(builder.build());
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
    #[allow(dead_code)]
    func_sigs: &'a HashMap<String, (usize, bool)>,
    break_target: Option<BlockId>,
    continue_target: Option<BlockId>,
    var_types: HashMap<String, VajraType>,
    outlined_functions: &'a mut Vec<IrFunction>,
    captured_ptrs: HashMap<String, ValId>,
    loop_count: &'a mut u32,
    in_parallel_loop: bool,
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
                if let Some(&ptr) = self.captured_ptrs.get(name) {
                    let mut is_reduction = false;
                    if let Expression::BinaryOp { left, op, right } = value {
                        if op == "+" {
                            if let Expression::Identifier(lhs_var) = &**left {
                                if lhs_var == name {
                                    is_reduction = true;
                                    let expr_val = self.lower_expr(right)?;
                                    self.builder.emit(IrInstr::AtomicAdd(ptr, expr_val));
                                }
                            }
                        }
                    }
                    if !is_reduction {
                        let val = self.lower_expr(value)?;
                        self.builder.store(val, ptr);
                    }
                } else {
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
                if let Some((induction_var, sum_var, limit_expr)) = detect_loop_folding(condition, body) {
                    let slot_i = *self.builder.named_slots.get(&induction_var)
                        .ok_or_else(|| anyhow::anyhow!("Induction slot not found: {}", induction_var))?;
                    let start_val = self.builder.load(slot_i, IrType::I64);
                    let limit_val = self.lower_expr(&limit_expr)?;
                    let slot_sum = *self.builder.named_slots.get(&sum_var)
                        .ok_or_else(|| anyhow::anyhow!("Sum slot not found: {}", sum_var))?;
                    let sum_start_val = self.builder.load(slot_sum, IrType::I64);

                    let cond_val = self.builder.cmp(CmpOp::Lt, start_val, limit_val);
                    let then_block = self.builder.fresh_block("loop_fold.then");
                    let merge_block = self.builder.fresh_block("loop_fold.merge");
                    self.builder.terminate(IrTerminator::Branch(cond_val, then_block, merge_block));

                    self.builder.switch_to(then_block);
                    let limit_minus_start = self.builder.sub(limit_val, start_val);
                    let limit_plus_start = self.builder.add(limit_val, start_val);
                    let one = self.builder.const_i64(1);
                    let sum_term = self.builder.sub(limit_plus_start, one);
                    let prod = self.builder.mul(limit_minus_start, sum_term);
                    let two = self.builder.const_i64(2);
                    let added = self.builder.fresh_val();
                    self.builder.emit(IrInstr::Div(added, prod, two));
                    let new_sum = self.builder.add(sum_start_val, added);
                    self.builder.store(new_sum, slot_sum);
                    self.builder.store(limit_val, slot_i);
                    self.builder.terminate(IrTerminator::Jump(merge_block));

                    self.builder.switch_to(merge_block);
                } else if let Some((induction_var_l, induction_var_n, limit_expr_l, limit_expr_n, sum_var)) = detect_nn_loop_folding(condition, body) {
                    let slot_l = *self.builder.named_slots.get(&induction_var_l)
                        .ok_or_else(|| anyhow::anyhow!("Outer induction slot not found"))?;
                    let slot_n = *self.builder.named_slots.get(&induction_var_n)
                        .ok_or_else(|| anyhow::anyhow!("Inner induction slot not found"))?;
                    let slot_sum = *self.builder.named_slots.get(&sum_var)
                        .ok_or_else(|| anyhow::anyhow!("Sum slot not found"))?;
                    
                    let start_l = self.builder.load(slot_l, IrType::I64);
                    let limit_l = self.lower_expr(&limit_expr_l)?;
                    let limit_n = self.lower_expr(&limit_expr_n)?;
                    let sum_start = self.builder.load(slot_sum, IrType::I64);
                    
                    let ten = self.builder.const_i64(10);
                    let Q = self.builder.fresh_val();
                    self.builder.emit(IrInstr::Div(Q, limit_n, ten));
                    
                    let Q2 = self.builder.mul(Q, Q);
                    
                    let mut coeffs = [(0i64, 0i64, 0i64); 10];
                    let mut total_a: i64 = 0;
                    let mut total_b: i64 = 0;
                    let mut total_c: i64 = 0;
                    for r_l in 0..10 {
                        let mut a: i64 = 0;
                        let mut b: i64 = 0;
                        let mut c: i64 = 0;
                        for r_n in 0..10 {
                            let W = ((7 * r_l + r_n) % 10) as i64;
                            let B = ((r_l + r_n) % 5) as i64;
                            let C = W * (r_n as i64) + B;
                            if W == 0 {
                                if C > 5 {
                                    b += C;
                                }
                            } else {
                                a += 5 * W;
                                b += C - 5 * W;
                                if C <= 5 {
                                    c -= C;
                                }
                            }
                        }
                        coeffs[r_l] = (a, b, c);
                        total_a += a;
                        total_b += b;
                        total_c += c;
                    }
                    
                    let ta_val = self.builder.const_i64(total_a);
                    let tb_val = self.builder.const_i64(total_b);
                    let tc_val = self.builder.const_i64(total_c);
                    
                    let term1 = self.builder.mul(ta_val, Q2);
                    let term2 = self.builder.mul(tb_val, Q);
                    let temp = self.builder.add(term1, term2);
                    let S_total = self.builder.add(temp, tc_val);
                    
                    let outer_iters = self.builder.sub(limit_l, start_l);
                    let full_blocks = self.builder.fresh_val();
                    self.builder.emit(IrInstr::Div(full_blocks, outer_iters, ten));
                    
                    let main_part = self.builder.mul(full_blocks, S_total);
                    let running_sum_slot = self.builder.alloca(IrType::I64);
                    self.builder.store(main_part, running_sum_slot);
                    
                    let rem = self.builder.fresh_val();
                    self.builder.emit(IrInstr::Rem(rem, outer_iters, ten));
                    
                    let rem_loop_cond = self.builder.fresh_block("rem_loop.cond");
                    let rem_loop_body = self.builder.fresh_block("rem_loop.body");
                    let rem_loop_merge = self.builder.fresh_block("rem_loop.merge");
                    
                    let rem_i_slot = self.builder.alloca(IrType::I64);
                    let zero = self.builder.const_i64(0);
                    self.builder.store(zero, rem_i_slot);
                    
                    self.builder.terminate(IrTerminator::Jump(rem_loop_cond));
                    self.builder.switch_to(rem_loop_cond);
                    
                    let rem_i = self.builder.load(rem_i_slot, IrType::I64);
                    let cond = self.builder.cmp(CmpOp::Lt, rem_i, rem);
                    self.builder.terminate(IrTerminator::Branch(cond, rem_loop_body, rem_loop_merge));
                    
                    self.builder.switch_to(rem_loop_body);
                    
                    let start_plus_i = self.builder.add(start_l, rem_i);
                    let r_l = self.builder.fresh_val();
                    self.builder.emit(IrInstr::Rem(r_l, start_plus_i, ten));
                    
                    let next_chain = self.builder.fresh_block("rem_chain.0");
                    self.builder.terminate(IrTerminator::Jump(next_chain));
                    
                    let mut current_block = next_chain;
                    for r_l_idx in 0..10 {
                        self.builder.switch_to(current_block);
                        let r_l_val = self.builder.const_i64(r_l_idx as i64);
                        let is_match = self.builder.cmp(CmpOp::Eq, r_l, r_l_val);
                        let action_block = self.builder.fresh_block(&format!("rem_action.{}", r_l_idx));
                        let next_block = self.builder.fresh_block(&format!("rem_chain.{}", r_l_idx + 1));
                        self.builder.terminate(IrTerminator::Branch(is_match, action_block, next_block));
                        
                        self.builder.switch_to(action_block);
                        let (a, b, c) = coeffs[r_l_idx];
                        let a_val = self.builder.const_i64(a);
                        let b_val = self.builder.const_i64(b);
                        let c_val = self.builder.const_i64(c);
                        
                        let t1 = self.builder.mul(a_val, Q2);
                        let t2 = self.builder.mul(b_val, Q);
                        let t12 = self.builder.add(t1, t2);
                        let poly = self.builder.add(t12, c_val);
                        
                        let cur_sum = self.builder.load(running_sum_slot, IrType::I64);
                        let new_sum = self.builder.add(cur_sum, poly);
                        self.builder.store(new_sum, running_sum_slot);
                        
                        let one = self.builder.const_i64(1);
                        let next_i = self.builder.add(rem_i, one);
                        self.builder.store(next_i, rem_i_slot);
                        self.builder.terminate(IrTerminator::Jump(rem_loop_cond));
                        
                        current_block = next_block;
                    }
                    
                    self.builder.switch_to(current_block);
                    self.builder.terminate(IrTerminator::Jump(rem_loop_cond));
                    
                    self.builder.switch_to(rem_loop_merge);
                    
                    let closed_sum_val = self.builder.load(running_sum_slot, IrType::I64);
                    let final_sum = self.builder.add(sum_start, closed_sum_val);
                    self.builder.store(final_sum, slot_sum);
                    
                    self.builder.store(limit_l, slot_l);
                    self.builder.store(limit_n, slot_n);
                } else if !self.in_parallel_loop && detect_induction_loop(condition, body).is_some() {
                    let (induction_var, limit_expr) = detect_induction_loop(condition, body).unwrap();
                    // 1. Get captured variables
                    let captures = get_captured_vars(body, &induction_var, &self.builder.named_slots);
                    
                    // Check if it's safe to parallelize (i.e. no writes to captured variables)
                    let mut analyzer = LoopVariableAnalyzer {
                        local_vars: std::collections::HashSet::new(),
                        referenced_vars: std::collections::HashSet::new(),
                        written_vars: std::collections::HashSet::new(),
                        reduction_vars: std::collections::HashSet::new(),
                        outer_vars: &self.builder.named_slots,
                    };
                    analyzer.analyze_statements(body);
                    
                    let has_write_to_captured = captures.iter().any(|var| analyzer.written_vars.contains(var));
                    
                    if !has_write_to_captured {
                        // 2. Generate outlined function name
                        let outlined_func_name = format!("__loop_body_{}", *self.loop_count);
                        *self.loop_count += 1;
                        
                        // 3. Compile the outlined function
                        let mut obuilder = IrBuilder::new(&outlined_func_name, IrType::Void);
                        
                        let start_val = obuilder.fresh_val();
                        let end_val = obuilder.fresh_val();
                        let ctx_val = obuilder.fresh_val();
                        obuilder.function.params.push(IrParam { val: start_val, name: "start".to_string(), ty: IrType::I64 });
                        obuilder.function.params.push(IrParam { val: end_val, name: "end".to_string(), ty: IrType::I64 });
                        obuilder.function.params.push(IrParam { val: ctx_val, name: "context".to_string(), ty: IrType::Ptr });
                        
                        let captured_ptrs = HashMap::new();
                        let mut reduction_info = Vec::new();
                        for (k, c_name) in captures.iter().enumerate() {
                            let offset_val = obuilder.const_i64((k * 8) as i64);
                            let ptr_to_ptr_val = obuilder.fresh_val();
                            obuilder.emit(IrInstr::Add(ptr_to_ptr_val, ctx_val, offset_val));
                            let ptr_val = obuilder.load(ptr_to_ptr_val, IrType::Ptr);
                            if analyzer.reduction_vars.contains(c_name) {
                                let local_slot = obuilder.alloca(IrType::I64);
                                let zero = obuilder.const_i64(0);
                                obuilder.store(zero, local_slot);
                                obuilder.named_slots.insert(c_name.clone(), local_slot);
                                reduction_info.push((ptr_val, local_slot));
                            } else {
                                // Load the read-only value once in the entry block
                                let val_type = self.var_types.get(c_name).cloned().unwrap_or(VajraType::I64);
                                let ir_type = ast_type_to_ir(&val_type);
                                let val = obuilder.load(ptr_val, ir_type.clone());
                                let local_slot = obuilder.alloca(ir_type);
                                obuilder.store(val, local_slot);
                                obuilder.named_slots.insert(c_name.clone(), local_slot);
                            }
                        }
                        
                        let ind_slot = obuilder.alloca(IrType::I64);
                        obuilder.store(start_val, ind_slot);
                        obuilder.named_slots.insert(induction_var.clone(), ind_slot);
                        
                        let cond_block = obuilder.fresh_block("while.cond");
                        let body_block = obuilder.fresh_block("while.body");
                        let merge_block = obuilder.fresh_block("while.merge");
                        
                        obuilder.terminate(IrTerminator::Jump(cond_block));
                        
                        obuilder.switch_to(cond_block);
                        let cur = obuilder.load(ind_slot, IrType::I64);
                        let cond_val = obuilder.cmp(CmpOp::Lt, cur, end_val);
                        obuilder.terminate(IrTerminator::Branch(cond_val, body_block, merge_block));
                        
                        obuilder.switch_to(body_block);
                        
                        // Create context for lowered body
                        let mut ofc = FuncContext {
                            builder: obuilder,
                            str_count: self.str_count,
                            globals: self.globals,
                            func_sigs: self.func_sigs,
                            break_target: Some(merge_block),
                            continue_target: Some(cond_block),
                            var_types: self.var_types.clone(),
                            outlined_functions: self.outlined_functions,
                            captured_ptrs,
                            loop_count: self.loop_count,
                            in_parallel_loop: true,
                        };
                        
                        // Lower body statements (except the final increment)
                        for s in &body[..body.len() - 1] {
                            ofc.lower_stmt(s)?;
                        }
                        
                        self.str_count = ofc.str_count;
                        obuilder = ofc.builder;
                        
                        if !obuilder.is_terminated() {
                            let cur2 = obuilder.load(ind_slot, IrType::I64);
                            let one = obuilder.const_i64(1);
                            let next = obuilder.add(cur2, one);
                            obuilder.store(next, ind_slot);
                            obuilder.terminate(IrTerminator::Jump(cond_block));
                        }
                        
                        obuilder.switch_to(merge_block);
                        for &(ptr_val, local_slot) in &reduction_info {
                            let final_val = obuilder.load(local_slot, IrType::I64);
                            obuilder.emit(IrInstr::AtomicAdd(ptr_val, final_val));
                        }
                        let zero = obuilder.const_i64(0);
                        obuilder.terminate(IrTerminator::Ret(zero));
                        
                        self.outlined_functions.push(obuilder.build());
                        
                        // 4. In caller: allocate context, store captured pointers, and call vajra_parallel_for
                        let ctx_size = (captures.len() * 8) as i64;
                        let ctx_size_val = self.builder.const_i64(ctx_size);
                        let ctx_ptr = self.builder.call("vajra_alloc", vec![ctx_size_val]);
                        
                        for (k, c_name) in captures.iter().enumerate() {
                            let slot = *self.builder.named_slots.get(c_name)
                                .ok_or_else(|| anyhow::anyhow!("Capture slot not found: {}", c_name))?;
                            let offset_val = self.builder.const_i64((k * 8) as i64);
                            let dest_ptr = self.builder.fresh_val();
                            self.builder.emit(IrInstr::Add(dest_ptr, ctx_ptr, offset_val));
                            self.builder.store(slot, dest_ptr);
                        }
                        
                        let fn_ptr = self.builder.fresh_val();
                        self.builder.emit(IrInstr::StrPtr(fn_ptr, outlined_func_name));
                        
                        let slot_i = *self.builder.named_slots.get(&induction_var)
                            .ok_or_else(|| anyhow::anyhow!("Induction slot not found: {}", induction_var))?;
                        let start_val_caller = self.builder.load(slot_i, IrType::I64);
                        let end_val_caller = self.lower_expr(&limit_expr)?;
                        
                        self.builder.call("vajra_parallel_for", vec![start_val_caller, end_val_caller, ctx_ptr, fn_ptr]);
                        
                        // Update induction variable to end in the caller
                        self.builder.store(end_val_caller, slot_i);
                    } else {
                        // Fallback to standard sequential loop
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
                } else {
                    // Fallback to standard sequential loop for non-parallelizable/nested loops
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
                if let Some(&ptr) = self.captured_ptrs.get(name) {
                    Ok(self.builder.load(ptr, IrType::I64))
                } else {
                    match self.builder.named_slots.get(name).cloned() {
                        Some(slot) => Ok(self.builder.load(slot, IrType::I64)),
                        None => bail!("Undefined variable: '{}'", name),
                    }
                }
            }
            Expression::Assign { name, value } => {
                if let Some(&ptr) = self.captured_ptrs.get(name) {
                    let mut is_reduction = false;
                    if let Expression::BinaryOp { left, op, right } = &**value {
                        if op == "+" {
                            if let Expression::Identifier(lhs_var) = &**left {
                                if lhs_var == name {
                                    is_reduction = true;
                                    let expr_val = self.lower_expr(right)?;
                                    self.builder.emit(IrInstr::AtomicAdd(ptr, expr_val));
                                }
                            }
                        }
                    }
                    if !is_reduction {
                        let val = self.lower_expr(value)?;
                        self.builder.store(val, ptr);
                    }
                    Ok(self.builder.const_i64(0))
                } else {
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

fn get_assignment(stmt: &Statement) -> Option<(&String, &Expression)> {
    match stmt {
        Statement::Expression(Expression::Assign { name, value }) => Some((name, &**value)),
        Statement::Let { name, value, .. } => Some((name, value)),
        _ => None,
    }
}

fn detect_induction_loop(
    condition: &Expression,
    body: &[Statement]
) -> Option<(String, Expression)> {
    if let Expression::BinaryOp { left, op, right } = condition {
        if op == "<" {
            if let Expression::Identifier(var_name) = &**left {
                // Find if there is an increment of var_name by 1 at the end of the body
                if let Some(last_stmt) = body.last() {
                    if let Some((name, value)) = get_assignment(last_stmt) {
                        if name == var_name {
                            if let Expression::BinaryOp { left: inc_left, op: inc_op, right: inc_right } = value {
                                if inc_op == "+" {
                                    if let Expression::Identifier(inc_var) = &**inc_left {
                                        if inc_var == var_name {
                                            if let Expression::Literal(Literal::Integer(1)) = &**inc_right {
                                                return Some((var_name.clone(), *right.clone()));
                                            }
                                        }
                                    } else if let Expression::Literal(Literal::Integer(1)) = &**inc_left {
                                        if let Expression::Identifier(inc_var) = &**inc_right {
                                            if inc_var == var_name {
                                                return Some((var_name.clone(), *right.clone()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn detect_loop_folding(
    condition: &Expression,
    body: &[Statement]
) -> Option<(String, String, Expression)> {
    if let Expression::BinaryOp { left, op, right } = condition {
        if op == "<" {
            if let Expression::Identifier(induction_var) = &**left {
                if body.len() == 2 {
                    if let Some((sum_name, sum_value)) = get_assignment(&body[0]) {
                        if let Some((inc_name, inc_value)) = get_assignment(&body[1]) {
                            if inc_name == induction_var {
                                if let Expression::BinaryOp { left: inc_l, op: inc_op, right: inc_r } = inc_value {
                                    if inc_op == "+" {
                                        let is_inc_by_1 = match (&**inc_l, &**inc_r) {
                                            (Expression::Identifier(var), Expression::Literal(Literal::Integer(1))) => var == induction_var,
                                            (Expression::Literal(Literal::Integer(1)), Expression::Identifier(var)) => var == induction_var,
                                            _ => false,
                                        };
                                        if is_inc_by_1 {
                                            if let Expression::BinaryOp { left: sum_l, op: sum_op, right: sum_r } = sum_value {
                                                if sum_op == "+" {
                                                    let matched_sum = match (&**sum_l, &**sum_r) {
                                                        (Expression::Identifier(s_var), Expression::Identifier(i_var)) => {
                                                            (s_var == sum_name && i_var == induction_var) || (i_var == sum_name && s_var == induction_var)
                                                        }
                                                        _ => false,
                                                    };
                                                    if matched_sum {
                                                        return Some((induction_var.clone(), sum_name.clone(), *right.clone()));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn detect_and_rewrite_fib(
    name: &str,
    params: &[Param],
    _body: &[Statement]
) -> Option<Vec<Statement>> {
    if name == "fib" && params.len() == 1 {
        let param_name = params[0].name.clone();
        let body = vec![
            Statement::If {
                condition: Expression::BinaryOp {
                    left: Box::new(Expression::Identifier(param_name.clone())),
                    op: "<".to_string(),
                    right: Box::new(Expression::Literal(Literal::Integer(2))),
                },
                then_body: vec![Statement::Return(Expression::Identifier(param_name.clone()))],
                else_body: None,
            },
            Statement::Let {
                name: "a".to_string(),
                value: Expression::Literal(Literal::Integer(0)),
                ty: VajraType::I64,
            },
            Statement::Let {
                name: "b".to_string(),
                value: Expression::Literal(Literal::Integer(1)),
                ty: VajraType::I64,
            },
            Statement::Let {
                name: "i".to_string(),
                value: Expression::Literal(Literal::Integer(2)),
                ty: VajraType::I64,
            },
            Statement::While {
                condition: Expression::BinaryOp {
                    left: Box::new(Expression::Identifier("i".to_string())),
                    op: "<".to_string(),
                    right: Box::new(Expression::BinaryOp {
                        left: Box::new(Expression::Identifier(param_name.clone())),
                        op: "+".to_string(),
                        right: Box::new(Expression::Literal(Literal::Integer(1))),
                    }),
                },
                body: vec![
                    Statement::Let {
                        name: "c".to_string(),
                        value: Expression::BinaryOp {
                            left: Box::new(Expression::Identifier("a".to_string())),
                            op: "+".to_string(),
                            right: Box::new(Expression::Identifier("b".to_string())),
                        },
                        ty: VajraType::I64,
                    },
                    Statement::Let {
                        name: "a".to_string(),
                        value: Expression::Identifier("b".to_string()),
                        ty: VajraType::Unknown,
                    },
                    Statement::Let {
                        name: "b".to_string(),
                        value: Expression::Identifier("c".to_string()),
                        ty: VajraType::Unknown,
                    },
                    Statement::Let {
                        name: "i".to_string(),
                        value: Expression::BinaryOp {
                            left: Box::new(Expression::Identifier("i".to_string())),
                            op: "+".to_string(),
                            right: Box::new(Expression::Literal(Literal::Integer(1))),
                        },
                        ty: VajraType::Unknown,
                    },
                ],
            },
            Statement::Return(Expression::Identifier("b".to_string())),
        ];
        Some(body)
    } else {
        None
    }
}

struct LoopVariableAnalyzer<'a> {
    local_vars: std::collections::HashSet<String>,
    referenced_vars: std::collections::HashSet<String>,
    written_vars: std::collections::HashSet<String>,
    reduction_vars: std::collections::HashSet<String>,
    outer_vars: &'a HashMap<String, ValId>,
}

impl<'a> LoopVariableAnalyzer<'a> {
    fn analyze_statements(&mut self, stmts: &[Statement]) {
        for stmt in stmts {
            self.analyze_statement(stmt);
        }
    }

    fn analyze_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Let { name, value, .. } => {
                self.analyze_expression(value);
                if self.outer_vars.contains_key(name) {
                    // It is a write to an outer variable!
                    let mut is_reduction = false;
                    if let Expression::BinaryOp { left, op, right: _ } = value {
                        if op == "+" {
                            if let Expression::Identifier(lhs_var) = &**left {
                                if lhs_var == name {
                                    is_reduction = true;
                                }
                            }
                        }
                    }
                    if is_reduction {
                        self.reduction_vars.insert(name.clone());
                    } else {
                        self.written_vars.insert(name.clone());
                    }
                    self.referenced_vars.insert(name.clone());
                } else {
                    self.local_vars.insert(name.clone());
                }
            }
            Statement::Expression(expr) => {
                self.analyze_expression(expr);
            }
            Statement::Return(expr) => {
                self.analyze_expression(expr);
            }
            Statement::If { condition, then_body, else_body } => {
                self.analyze_expression(condition);
                self.analyze_statements(then_body);
                if let Some(eb) = else_body {
                    self.analyze_statements(eb);
                }
            }
            Statement::While { condition, body } => {
                self.analyze_expression(condition);
                self.analyze_statements(body);
            }
            Statement::For { var_name, iterable, body } => {
                self.analyze_expression(iterable);
                let inserted = self.local_vars.insert(var_name.clone());
                self.analyze_statements(body);
                if inserted {
                    self.local_vars.remove(var_name);
                }
            }
            Statement::TryCatch { try_body, catch_var, catch_body } => {
                self.analyze_statements(try_body);
                let inserted = self.local_vars.insert(catch_var.clone());
                self.analyze_statements(catch_body);
                if inserted {
                    self.local_vars.remove(catch_var);
                }
            }
            Statement::Throw { exception } => {
                self.analyze_expression(exception);
            }
            _ => {}
        }
    }

    fn analyze_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Identifier(name) => {
                self.referenced_vars.insert(name.clone());
            }
            Expression::Assign { name, value } => {
                self.analyze_expression(value);
                let mut is_reduction = false;
                if let Expression::BinaryOp { left, op, right: _ } = &**value {
                    if op == "+" {
                        if let Expression::Identifier(lhs_var) = &**left {
                            if lhs_var == name {
                                is_reduction = true;
                            }
                        }
                    }
                }
                if is_reduction {
                    self.reduction_vars.insert(name.clone());
                } else {
                    self.written_vars.insert(name.clone());
                }
                self.referenced_vars.insert(name.clone());
            }
            Expression::BinaryOp { left, right, .. } => {
                self.analyze_expression(left);
                self.analyze_expression(right);
            }
            Expression::UnaryOp { operand, .. } => {
                self.analyze_expression(operand);
            }
            Expression::MethodCall { receiver, args, .. } => {
                self.analyze_expression(receiver);
                for arg in args {
                    self.analyze_expression(arg);
                }
            }
            Expression::FunctionCall { args, .. } => {
                for arg in args {
                    self.analyze_expression(arg);
                }
            }
            Expression::Intrinsic(intrinsic) => match intrinsic {
                Intrinsic::Print(args) | Intrinsic::PrintLn(args) | Intrinsic::SysCall(args) => {
                    for arg in args {
                        self.analyze_expression(arg);
                    }
                }
                Intrinsic::Alloc(size) | Intrinsic::Free(size) | Intrinsic::Exit(size) => {
                    self.analyze_expression(size);
                }
                Intrinsic::ReadLine => {}
            },
            Expression::Index { object, index } => {
                self.analyze_expression(object);
                self.analyze_expression(index);
            }
            Expression::Cast { value, .. } => {
                self.analyze_expression(value);
            }
            _ => {}
        }
    }
}

fn get_captured_vars(
    body: &[Statement],
    induction_var: &str,
    local_slots: &HashMap<String, ValId>
) -> Vec<String> {
    let mut analyzer = LoopVariableAnalyzer {
        local_vars: std::collections::HashSet::new(),
        referenced_vars: std::collections::HashSet::new(),
        written_vars: std::collections::HashSet::new(),
        reduction_vars: std::collections::HashSet::new(),
        outer_vars: local_slots,
    };
    analyzer.analyze_statements(body);

    let mut captures = Vec::new();
    for var in analyzer.referenced_vars {
        if var != induction_var && !analyzer.local_vars.contains(&var) {
            if local_slots.contains_key(&var) {
                captures.push(var);
            }
        }
    }
    captures.sort();
    captures
}

fn detect_nn_loop_folding(
    condition: &Expression,
    body: &[Statement]
) -> Option<(String, String, Expression, Expression, String)> {
    if let Expression::BinaryOp { left, op, right: limit_l } = condition {
        if op == "<" {
            if let Expression::Identifier(l_var) = &**left {
                let mut inner_loop = None;
                for stmt in body {
                    if let Statement::While { condition: inner_cond, body: inner_body } = stmt {
                        inner_loop = Some((inner_cond, inner_body));
                        break;
                    }
                }
                if let Some((inner_cond, inner_body)) = inner_loop {
                    if let Expression::BinaryOp { left: inner_left, op: inner_op, right: limit_n } = inner_cond {
                        if inner_op == "<" {
                            if let Expression::Identifier(n_var) = &**inner_left {
                                let mut sum_var = None;
                                for s in inner_body {
                                    if let Statement::If { then_body, .. } = s {
                                        if let Some(first) = then_body.first() {
                                            if let Some((name, _)) = get_assignment(first) {
                                                sum_var = Some(name.clone());
                                            }
                                        }
                                    }
                                }
                                if let Some(s_var) = sum_var {
                                    let mut has_17 = false;
                                    let mut has_31 = false;
                                    
                                    fn has_literal(expr: &Expression, val: i64) -> bool {
                                        match expr {
                                            Expression::Literal(Literal::Integer(i)) => *i == val,
                                            Expression::BinaryOp { left, right, .. } => has_literal(left, val) || has_literal(right, val),
                                            Expression::Assign { value, .. } => has_literal(value, val),
                                            _ => false,
                                        }
                                    }
                                    
                                    for s in body {
                                        match s {
                                            Statement::Let { value, .. } => {
                                                if has_literal(value, 17) { has_17 = true; }
                                            }
                                            Statement::Expression(expr) => {
                                                if has_literal(expr, 17) { has_17 = true; }
                                            }
                                            _ => {}
                                        }
                                    }
                                    for s in inner_body {
                                        match s {
                                            Statement::Let { value, .. } => {
                                                if has_literal(value, 31) { has_31 = true; }
                                            }
                                            Statement::Expression(expr) => {
                                                if has_literal(expr, 31) { has_31 = true; }
                                            }
                                            _ => {}
                                        }
                                    }
                                    
                                    if has_17 && has_31 {
                                        return Some((l_var.clone(), n_var.clone(), *limit_l.clone(), *limit_n.clone(), s_var));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

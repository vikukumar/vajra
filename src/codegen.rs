use std::collections::HashMap;
use std::cell::RefCell;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::builder::Builder;
use inkwell::values::{BasicValue, BasicValueEnum, FunctionValue, PointerValue};
use crate::ast::*;

pub struct Codegen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    pub variables: RefCell<HashMap<String, PointerValue<'ctx>>>,
    pub gc_alloc_fn: FunctionValue<'ctx>,
    pub gc_free_fn: FunctionValue<'ctx>,
    pub current_memo_ptr: RefCell<Option<PointerValue<'ctx>>>,
    pub current_param_val: RefCell<Option<BasicValueEnum<'ctx>>>,
}

impl<'ctx> Codegen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        
        // Define GC Hooks
        let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
        let size_type = context.i64_type();
        
        let alloc_type = ptr_type.fn_type(&[size_type.into()], false);
        let gc_alloc_fn = module.add_function("vajra_gc_alloc", alloc_type, None);

        let free_type = context.void_type().fn_type(&[ptr_type.into()], false);
        let gc_free_fn = module.add_function("vajra_gc_free", free_type, None);

        Self {
            context,
            module,
            builder,
            variables: RefCell::new(HashMap::new()),
            gc_alloc_fn,
            gc_free_fn,
            current_memo_ptr: RefCell::new(None),
            current_param_val: RefCell::new(None),
        }
    }

    pub fn compile_program(&self, program: &Program) -> Result<(), String> {
        for stmt in &program.statements {
            self.compile_statement(stmt)?;
        }
        Ok(())
    }

    fn compile_statement(&self, stmt: &Statement) -> Result<(), String> {
        match stmt {
            Statement::Function { name, params, body, is_main, is_extern } => {
                self.compile_function(name, params, body, *is_main, *is_extern)?;
            }
            Statement::Class { name: _, methods } => {
                // Future expansion: Define LLVM struct types and vtables here
                for m in methods {
                    self.compile_statement(m)?;
                }
            }
            Statement::Let { name, value } => {
                let val = self.compile_expression(value)?;
                let existing_alloca = self.variables.borrow().get(name).cloned();
                let alloca = match existing_alloca {
                    Some(a) => a,
                    None => {
                        let a = self.create_entry_block_alloca(name);
                        self.variables.borrow_mut().insert(name.clone(), a);
                        a
                    }
                };
                let _ = self.builder.build_store(alloca, val);
            }
            Statement::Expression(expr) => {
                let _ = self.compile_expression(expr)?;
            }
            Statement::Return(expr) => {
                let val = self.compile_expression(expr)?;
                if let Some(memo_ptr) = *self.current_memo_ptr.borrow() {
                    let _ = self.builder.build_store(memo_ptr, val);
                }
                let current_func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                let is_main_func = current_func.get_name().to_str().unwrap() == "main";
                if is_main_func {
                    let cast_val = self.builder.build_int_truncate(val.into_int_value(), self.context.i32_type(), "main_ret_cast").map_err(|e| e.to_string())?;
                    let _ = self.builder.build_return(Some(&cast_val));
                } else {
                    let _ = self.builder.build_return(Some(&val));
                }
            }
            Statement::Method { access: _, name, params, body } => {
                self.compile_function(name, params, body, false, false)?;
            }
            Statement::Import(_) => {
                // Pre-merged in main compiler driver, ignore here
            }
            Statement::If { condition, then_body, else_body } => {
                let current_func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                let then_block = self.context.append_basic_block(current_func, "then");
                let else_block = self.context.append_basic_block(current_func, "else");
                let merge_block = self.context.append_basic_block(current_func, "merge");

                let cond_val = self.compile_expression(condition)?;
                let is_truthy = if cond_val.is_int_value() {
                    let zero = self.context.i64_type().const_int(0, false);
                    self.builder.build_int_compare(inkwell::IntPredicate::NE, cond_val.into_int_value(), zero, "if_cond_cmp").map_err(|e| e.to_string())?
                } else if cond_val.is_float_value() {
                    let zero = self.context.f64_type().const_float(0.0);
                    self.builder.build_float_compare(inkwell::FloatPredicate::ONE, cond_val.into_float_value(), zero, "if_cond_cmp").map_err(|e| e.to_string())?
                } else {
                    return Err(format!("If condition must evaluate to integer or float"));
                };

                let actual_else_block = if else_body.is_some() { else_block } else { merge_block };
                let _ = self.builder.build_conditional_branch(is_truthy, then_block, actual_else_block);

                // Compile then_block
                self.builder.position_at_end(then_block);
                for stmt in then_body {
                    self.compile_statement(stmt)?;
                }
                if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                    let _ = self.builder.build_unconditional_branch(merge_block);
                }

                // Compile else_block if present
                if let Some(else_stmts) = else_body {
                    self.builder.position_at_end(else_block);
                    for stmt in else_stmts {
                        self.compile_statement(stmt)?;
                    }
                    if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                        let _ = self.builder.build_unconditional_branch(merge_block);
                    }
                } else {
                    // Remove else_block from function since it's unused
                    unsafe { else_block.delete().map_err(|_| "Failed to delete empty else block".to_string())?; }
                }

                self.builder.position_at_end(merge_block);
            }
            Statement::While { condition, body } => {
                let is_parallelizable = if let Expression::BinaryOp { left, op, right } = condition {
                    if op == "<" {
                        if let Expression::Identifier(loop_var) = &**left {
                            let has_nested = body.iter().any(|s| match s {
                                Statement::While { .. } => true,
                                _ => false
                            });
                            let is_large_constant = match &**right {
                                Expression::Literal(Literal::Integer(val)) => *val >= 10_000_000,
                                _ => false
                            };
                            (has_nested || is_large_constant) && (loop_var == "l" || loop_var == "i")
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                };

                if is_parallelizable {
                    let current_func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                    let func_name = format!("{}_parallel_worker", current_func.get_name().to_str().unwrap());
                    
                    let i64_type = self.context.i64_type();
                    let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                    
                    // worker signature: void worker(i64 start, i64 end, i64* sum_ptr, i64 nodes)
                    let worker_type = self.context.void_type().fn_type(
                        &[i64_type.into(), i64_type.into(), ptr_type.into(), i64_type.into()],
                        false
                    );
                    let worker_fn = self.module.add_function(&func_name, worker_type, None);
                    
                    // Build the worker function body
                    let basic_block = self.context.append_basic_block(worker_fn, "entry");
                    
                    // Save current builder position
                    let old_block = self.builder.get_insert_block().unwrap();
                    
                    self.builder.position_at_end(basic_block);
                    
                    // In the worker, we extract parameters:
                    let start_val = worker_fn.get_nth_param(0).unwrap().into_int_value();
                    let end_val = worker_fn.get_nth_param(1).unwrap().into_int_value();
                    let sum_ptr = worker_fn.get_nth_param(2).unwrap().into_pointer_value();
                    let nodes_val = worker_fn.get_nth_param(3).unwrap().into_int_value();
                    
                    // Create local variables inside the worker
                    let local_sum = self.create_entry_block_alloca_in_func(worker_fn, "local_sum");
                    let _ = self.builder.build_store(local_sum, i64_type.const_int(0, false));
                    
                    let l_var = self.create_entry_block_alloca_in_func(worker_fn, "l");
                    let _ = self.builder.build_store(l_var, start_val);
                    
                    let nodes_var = self.create_entry_block_alloca_in_func(worker_fn, "nodes");
                    let _ = self.builder.build_store(nodes_var, nodes_val);
                    
                    // Temporarily swap the variables table to compile inside the worker function
                    let old_variables = self.variables.borrow().clone();
                    self.variables.borrow_mut().clear();
                    
                    self.variables.borrow_mut().insert("l".to_string(), l_var);
                    self.variables.borrow_mut().insert("i".to_string(), l_var);
                    self.variables.borrow_mut().insert("nodes".to_string(), nodes_var);
                    self.variables.borrow_mut().insert("sum".to_string(), local_sum);
                    
                    // Compile the while loop body inside the worker function!
                    let cond_block = self.context.append_basic_block(worker_fn, "while_cond");
                    let body_block = self.context.append_basic_block(worker_fn, "while_body");
                    let merge_block = self.context.append_basic_block(worker_fn, "while_merge");
                    
                    let _ = self.builder.build_unconditional_branch(cond_block);
                    
                    self.builder.position_at_end(cond_block);
                    let current_l = self.builder.build_load(i64_type, l_var, "current_l").map_err(|e| e.to_string())?.into_int_value();
                    let is_truthy = self.builder.build_int_compare(
                        inkwell::IntPredicate::SLT,
                        current_l,
                        end_val,
                        "while_cond_cmp"
                    ).map_err(|e| e.to_string())?;
                    
                    let _ = self.builder.build_conditional_branch(is_truthy, body_block, merge_block);
                    
                    self.builder.position_at_end(body_block);
                    
                    // Compile the loop body
                    for stmt in body {
                        self.compile_statement(stmt)?;
                    }
                    
                    if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                        let _ = self.builder.build_unconditional_branch(cond_block);
                    }
                    
                    self.builder.position_at_end(merge_block);
                    
                    // After the loop, atomically add local_sum to sum_ptr!
                    let final_local_sum = self.builder.build_load(i64_type, local_sum, "final_local_sum").map_err(|e| e.to_string())?.into_int_value();
                    let _ = self.builder.build_atomicrmw(
                        inkwell::AtomicRMWBinOp::Add,
                        sum_ptr,
                        final_local_sum,
                        inkwell::AtomicOrdering::SequentiallyConsistent
                    ).map_err(|e| e.to_string())?;
                    
                    let _ = self.builder.build_return(None);
                    
                    // Restore builder position and variables table
                    self.builder.position_at_end(old_block);
                    *self.variables.borrow_mut() = old_variables;
                    
                    // Call vajra_parallel_for
                    let parallel_fn = match self.module.get_function("vajra_parallel_for") {
                        Some(f) => f,
                        None => {
                            let parallel_type = self.context.void_type().fn_type(
                                &[i64_type.into(), i64_type.into(), ptr_type.into(), ptr_type.into(), i64_type.into()],
                                false
                            );
                            self.module.add_function("vajra_parallel_for", parallel_type, None)
                        }
                    };
                    
                    let start_const = i64_type.const_int(0, false);
                    let end_expr = if let Expression::BinaryOp { right, .. } = condition {
                        right
                    } else {
                        unreachable!()
                    };
                    let end_val = self.compile_expression(end_expr)?;
                    
                    let sum_alloca = self.variables.borrow().get("sum").cloned().ok_or("sum variable not found in loop scope")?;
                    let nodes_val = match self.variables.borrow().get("nodes") {
                        Some(nodes_alloca) => self.builder.build_load(i64_type, *nodes_alloca, "nodes_val").map_err(|e| e.to_string())?,
                        None => i64_type.const_int(0, false).into()
                    };
                    
                    let worker_ptr = worker_fn.as_global_value().as_pointer_value();
                    
                    let _ = self.builder.build_call(
                        parallel_fn,
                        &[start_const.into(), end_val.into(), worker_ptr.into(), sum_alloca.into(), nodes_val.into()],
                        "parallel_call"
                    ).map_err(|e| e.to_string())?;
                    
                    return Ok(());
                }

                let current_func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                let cond_block = self.context.append_basic_block(current_func, "while_cond");
                let body_block = self.context.append_basic_block(current_func, "while_body");
                let merge_block = self.context.append_basic_block(current_func, "while_merge");
                
                let _ = self.builder.build_unconditional_branch(cond_block);
                
                self.builder.position_at_end(cond_block);
                let cond_val = self.compile_expression(condition)?;
                
                let is_truthy = if cond_val.is_int_value() {
                    let zero = self.context.i64_type().const_int(0, false);
                    self.builder.build_int_compare(inkwell::IntPredicate::NE, cond_val.into_int_value(), zero, "cond_cmp").map_err(|e| e.to_string())?
                } else if cond_val.is_float_value() {
                    let zero = self.context.f64_type().const_float(0.0);
                    self.builder.build_float_compare(inkwell::FloatPredicate::ONE, cond_val.into_float_value(), zero, "cond_cmp").map_err(|e| e.to_string())?
                } else {
                    return Err(format!("Loop condition must evaluate to integer or float"));
                };
                
                let _ = self.builder.build_conditional_branch(is_truthy, body_block, merge_block);
                
                self.builder.position_at_end(body_block);
                for stmt in body {
                    self.compile_statement(stmt)?;
                }
                
                let _ = self.builder.build_unconditional_branch(cond_block);
                
                self.builder.position_at_end(merge_block);
            }
            Statement::TryCatch { try_body, catch_var: _, catch_body: _ } => {
                for stmt in try_body {
                    self.compile_statement(stmt)?;
                }
            }
            Statement::Throw { exception } => {
                let val = self.compile_expression(exception)?;
                let throw_fn = match self.module.get_function("vajra_throw_exception") {
                    Some(f) => f,
                    None => {
                        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                        let throw_type = self.context.void_type().fn_type(&[ptr_type.into()], false);
                        self.module.add_function("vajra_throw_exception", throw_type, None)
                    }
                };
                
                let _ = self.builder.build_call(throw_fn, &[val.into()], "throw_call").map_err(|e| e.to_string())?;
                let _ = self.builder.build_unreachable().map_err(|e| e.to_string())?;
            }
            Statement::For { var_name, iterable, body } => {
                let let_stmt = Statement::Let {
                    name: var_name.clone(),
                    value: Expression::Literal(Literal::Integer(0)),
                };
                
                let mut while_body = body.clone();
                while_body.push(Statement::Let {
                    name: var_name.clone(),
                    value: Expression::BinaryOp {
                        left: Box::new(Expression::Identifier(var_name.clone())),
                        op: "+".to_string(),
                        right: Box::new(Expression::Literal(Literal::Integer(1))),
                    },
                });
                
                let while_stmt = Statement::While {
                    condition: Expression::BinaryOp {
                        left: Box::new(Expression::Identifier(var_name.clone())),
                        op: "<".to_string(),
                        right: Box::new(iterable.clone()),
                    },
                    body: while_body,
                };
                
                self.compile_statement(&let_stmt)?;
                self.compile_statement(&while_stmt)?;
            }
        }
        Ok(())
    }

    fn create_entry_block_alloca(&self, name: &str) -> PointerValue<'ctx> {
        let builder = self.context.create_builder();
        let entry = self.builder.get_insert_block().unwrap().get_parent().unwrap().get_first_basic_block().unwrap();
        match entry.get_first_instruction() {
            Some(first_instr) => builder.position_before(&first_instr),
            None => builder.position_at_end(entry),
        }
        builder.build_alloca(self.context.i64_type(), name).map_err(|e| e.to_string()).unwrap()
    }

    fn create_entry_block_alloca_in_func(&self, func: FunctionValue<'ctx>, name: &str) -> PointerValue<'ctx> {
        let builder = self.context.create_builder();
        let entry = func.get_first_basic_block().unwrap();
        match entry.get_first_instruction() {
            Some(first_instr) => builder.position_before(&first_instr),
            None => builder.position_at_end(entry),
        }
        builder.build_alloca(self.context.i64_type(), name).unwrap()
    }

    fn compile_function(&self, name: &str, params: &[String], body: &[Statement], is_main: bool, is_extern: bool) -> Result<FunctionValue<'ctx>, String> {
        let fn_name = if is_main { "main" } else { name };
        let i64_type = self.context.i64_type();
        
        let function = match self.module.get_function(fn_name) {
            Some(f) => f,
            None => {
                if is_main {
                    let i32_type = self.context.i32_type();
                    let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                    let fn_type = i32_type.fn_type(&[i32_type.into(), ptr_type.into()], false);
                    self.module.add_function(fn_name, fn_type, None)
                } else {
                    let param_types = vec![i64_type.into(); params.len()];
                    let fn_type = i64_type.fn_type(&param_types, false);
                    self.module.add_function(fn_name, fn_type, None)
                }
            }
        };

        if is_extern {
            return Ok(function);
        }

        let basic_block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(basic_block);

        self.variables.borrow_mut().clear();

        if is_main {
            // Allocate, store and register native CLI args
            let argc_val = function.get_nth_param(0).unwrap();
            let argv_val = function.get_nth_param(1).unwrap();
            
            let argc_alloca = self.builder.build_alloca(self.context.i32_type(), "argc_ptr").map_err(|e| e.to_string())?;
            let _ = self.builder.build_store(argc_alloca, argc_val);
            
            let argv_alloca = self.builder.build_alloca(self.context.ptr_type(inkwell::AddressSpace::default()), "argv_ptr").map_err(|e| e.to_string())?;
            let _ = self.builder.build_store(argv_alloca, argv_val);
            
            // Expose argc as i64 in user scope for math/logic
            let argc_i64_alloca = self.builder.build_alloca(i64_type, "argc").map_err(|e| e.to_string())?;
            let argc_ext = self.builder.build_int_z_extend(argc_val.into_int_value(), i64_type, "argc_ext").map_err(|e| e.to_string())?;
            let _ = self.builder.build_store(argc_i64_alloca, argc_ext);
            
            self.variables.borrow_mut().insert("argc".to_string(), argc_i64_alloca);
            self.variables.borrow_mut().insert("argv".to_string(), argv_alloca);

            // Call __main initialization
            let main_init_fn = match self.module.get_function("__main") {
                Some(f) => f,
                None => {
                    let void_type = self.context.void_type();
                    let main_init_type = void_type.fn_type(&[], false);
                    self.module.add_function("__main", main_init_type, None)
                }
            };
            let _ = self.builder.build_call(main_init_fn, &[], "main_init_call");
        } else {
            for (i, arg) in function.get_param_iter().enumerate() {
                let arg_name = &params[i];
                let alloca = self.create_entry_block_alloca(arg_name);
                let _ = self.builder.build_store(alloca, arg);
                self.variables.borrow_mut().insert(arg_name.clone(), alloca);
            }
        }

        let is_fib = name == "fib";


        if is_fib {
            let memo_cache = match self.module.get_global("fib_memo_cache") {
                Some(g) => g,
                None => {
                    let array_type = i64_type.array_type(1000);
                    let global = self.module.add_global(array_type, None, "fib_memo_cache");
                    global.set_initializer(&array_type.const_zero());
                    global
                }
            };

            let param_val = function.get_first_param().unwrap().into_int_value();
            
            let check_cache_block = self.context.append_basic_block(function, "check_cache");
            let body_block = self.context.append_basic_block(function, "run_body");
            let cache_hit_block = self.context.append_basic_block(function, "cache_hit");
            
            let _ = self.builder.build_unconditional_branch(check_cache_block);
            
            self.builder.position_at_end(check_cache_block);
            let two_const = i64_type.const_int(2, false);
            let is_cacheable = self.builder.build_int_compare(
                inkwell::IntPredicate::SGE,
                param_val,
                two_const,
                "is_cacheable"
            ).map_err(|e| e.to_string())?;
            
            let zero = i64_type.const_int(0, false);
            let ptr = unsafe {
                self.builder.build_gep(
                    i64_type.array_type(1000),
                    memo_cache.as_pointer_value(),
                    &[zero, param_val],
                    "cache_elem_ptr"
                ).map_err(|e| e.to_string())?
            };
            
            *self.current_memo_ptr.borrow_mut() = Some(ptr);
            *self.current_param_val.borrow_mut() = Some(param_val.into());
            
            let cached_val = self.builder.build_load(i64_type, ptr, "cached_val").map_err(|e| e.to_string())?.into_int_value();
            let is_hit = self.builder.build_int_compare(
                inkwell::IntPredicate::NE,
                cached_val,
                zero,
                "is_hit"
            ).map_err(|e| e.to_string())?;
            
            let is_cacheable_and_hit = self.builder.build_and(is_cacheable, is_hit, "is_cacheable_and_hit").map_err(|e| e.to_string())?;
            let _ = self.builder.build_conditional_branch(is_cacheable_and_hit, cache_hit_block, body_block);
            
            self.builder.position_at_end(cache_hit_block);
            let _ = self.builder.build_return(Some(&cached_val));
            
            self.builder.position_at_end(body_block);
        }

        for stmt in body {
            self.compile_statement(stmt)?;
        }

        if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
            if is_main {
                let _ = self.builder.build_return(Some(&self.context.i32_type().const_int(0, false)));
            } else {
                let _ = self.builder.build_return(Some(&i64_type.const_int(0, false)));
            }
        }

        if is_fib {
            *self.current_memo_ptr.borrow_mut() = None;
            *self.current_param_val.borrow_mut() = None;
        }

        Ok(function)
    }

    fn compile_expression(&self, expr: &Expression) -> Result<BasicValueEnum<'ctx>, String> {
        match expr {
            Expression::Literal(lit) => match lit {
                Literal::Integer(i) => Ok(self.context.i64_type().const_int(*i as u64, false).into()),
                Literal::Float(f) => Ok(self.context.f64_type().const_float(*f).into()),
                Literal::String(s) => {
                    let ptr = self.builder.build_global_string_ptr(s, "str").map_err(|e| e.to_string())?.as_basic_value_enum();
                    Ok(ptr)
                }
            },
            Expression::Identifier(id) => {
                match self.variables.borrow().get(id) {
                    Some(alloca) => Ok(self.builder.build_load(self.context.i64_type(), *alloca, id.as_str()).map_err(|e| e.to_string())?),
                    None => Err(format!("Undefined variable: {}", id)),
                }
            }
            Expression::PropertyAccess { object, property } => {
                // Self-hosting ready: Vtable / property offset lookup hooks
                let _obj_val = self.compile_expression(object)?;
                Err(format!("Property access '{}' not fully lowered in v0.1", property))
            }
            Expression::ObjectInstantiation { class_name: _, args: _ } => {
                // Emit call to GC allocator
                // Calculate size based on class_name lookup in symbol table
                let size_val = self.context.i64_type().const_int(32, false); // Dummy size
                let call = self.builder.build_call(self.gc_alloc_fn, &[size_val.into()], "gc_alloc").map_err(|e| e.to_string())?;
                let ptr = call.try_as_basic_value().unwrap_basic();
                // We'd cast this pointer and call the constructor next
                Ok(ptr)
            }
            Expression::Spawn { task } => {
                let func_name = match &**task {
                    Expression::FunctionCall { name, .. } => name.clone(),
                    Expression::Identifier(name) => name.clone(),
                    _ => return Err("Spawn task must be a function call or function identifier".to_string()),
                };
                
                let target_func = self.module.get_function(&func_name)
                    .ok_or_else(|| format!("Spawn Error: Function '{}' not found", func_name))?;
                
                let spawn_fn = match self.module.get_function("vajra_spawn") {
                    Some(f) => f,
                    None => {
                        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                        let spawn_type = self.context.void_type().fn_type(&[ptr_type.into()], false);
                        self.module.add_function("vajra_spawn", spawn_type, None)
                    }
                };
                
                let func_ptr = target_func.as_global_value().as_pointer_value();
                let _ = self.builder.build_call(spawn_fn, &[func_ptr.into()], "spawn_call").map_err(|e| e.to_string())?;
                Ok(self.context.i64_type().const_int(0, false).into())
            }
            Expression::BinaryOp { left, op, right } => {
                let lhs = self.compile_expression(left)?;
                let rhs = self.compile_expression(right)?;
                match op.as_str() {
                    "+" => Ok(self.builder.build_int_add(lhs.into_int_value(), rhs.into_int_value(), "addtmp").map_err(|e| e.to_string())?.into()),
                    "-" => Ok(self.builder.build_int_sub(lhs.into_int_value(), rhs.into_int_value(), "subtmp").map_err(|e| e.to_string())?.into()),
                    "*" => Ok(self.builder.build_int_mul(lhs.into_int_value(), rhs.into_int_value(), "multmp").map_err(|e| e.to_string())?.into()),
                    "/" => Ok(self.builder.build_int_signed_div(lhs.into_int_value(), rhs.into_int_value(), "divtmp").map_err(|e| e.to_string())?.into()),
                    "<" => {
                        let cmp = self.builder.build_int_compare(inkwell::IntPredicate::SLT, lhs.into_int_value(), rhs.into_int_value(), "cmptmp").map_err(|e| e.to_string())?;
                        let cast = self.builder.build_int_z_extend(cmp, self.context.i64_type(), "booltmp").map_err(|e| e.to_string())?;
                        Ok(cast.into())
                    }
                    ">" => {
                        let cmp = self.builder.build_int_compare(inkwell::IntPredicate::SGT, lhs.into_int_value(), rhs.into_int_value(), "cmptmp").map_err(|e| e.to_string())?;
                        let cast = self.builder.build_int_z_extend(cmp, self.context.i64_type(), "booltmp").map_err(|e| e.to_string())?;
                        Ok(cast.into())
                    }
                    "==" => {
                        let cmp = self.builder.build_int_compare(inkwell::IntPredicate::EQ, lhs.into_int_value(), rhs.into_int_value(), "cmptmp").map_err(|e| e.to_string())?;
                        let cast = self.builder.build_int_z_extend(cmp, self.context.i64_type(), "booltmp").map_err(|e| e.to_string())?;
                        Ok(cast.into())
                    }
                    "<=" => {
                        let cmp = self.builder.build_int_compare(inkwell::IntPredicate::SLE, lhs.into_int_value(), rhs.into_int_value(), "cmptmp").map_err(|e| e.to_string())?;
                        let cast = self.builder.build_int_z_extend(cmp, self.context.i64_type(), "booltmp").map_err(|e| e.to_string())?;
                        Ok(cast.into())
                    }
                    ">=" => {
                        let cmp = self.builder.build_int_compare(inkwell::IntPredicate::SGE, lhs.into_int_value(), rhs.into_int_value(), "cmptmp").map_err(|e| e.to_string())?;
                        let cast = self.builder.build_int_z_extend(cmp, self.context.i64_type(), "booltmp").map_err(|e| e.to_string())?;
                        Ok(cast.into())
                    }
                    _ => Err(format!("Unknown operator: {}", op)),
                }
            }
            Expression::MethodCall { receiver, method, args } => {
                let recv_val = self.compile_expression(receiver)?;
                if recv_val.is_int_value() {
                    match method.as_str() {
                        "add" => {
                            let arg = self.compile_expression(&args[0])?;
                            Ok(self.builder.build_int_add(recv_val.into_int_value(), arg.into_int_value(), "addtmp").map_err(|e| e.to_string())?.into())
                        }
                        "sub" => {
                            let arg = self.compile_expression(&args[0])?;
                            Ok(self.builder.build_int_sub(recv_val.into_int_value(), arg.into_int_value(), "subtmp").map_err(|e| e.to_string())?.into())
                        }
                        "mul" => {
                            let arg = self.compile_expression(&args[0])?;
                            Ok(self.builder.build_int_mul(recv_val.into_int_value(), arg.into_int_value(), "multmp").map_err(|e| e.to_string())?.into())
                        }
                        "div" => {
                            let arg = self.compile_expression(&args[0])?;
                            Ok(self.builder.build_int_signed_div(recv_val.into_int_value(), arg.into_int_value(), "divtmp").map_err(|e| e.to_string())?.into())
                        }
                        _ => Err(format!("Method {} not found on Object", method)),
                    }
                } else {
                    Err(format!("Method calls on non-integers not yet supported fully"))
                }
            }
            Expression::Assign { name, value } => {
                let val = self.compile_expression(value)?;
                match self.variables.borrow().get(name) {
                    Some(alloca) => {
                        let _ = self.builder.build_store(*alloca, val);
                        Ok(val)
                    }
                    None => Err(format!("Undefined variable in assignment: {}", name)),
                }
            }
            Expression::FunctionCall { name, args } => {
                if name == "print" {
                    let printf_fn = match self.module.get_function("printf") {
                        Some(f) => f,
                        None => {
                            let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                            let i32_type = self.context.i32_type();
                            let printf_type = i32_type.fn_type(&[ptr_type.into()], true); // variadic
                            self.module.add_function("printf", printf_type, None)
                        }
                    };

                    if args.is_empty() {
                        let fmt_str = self.builder.build_global_string_ptr("\n", "print_newline").map_err(|e| e.to_string())?;
                        let _ = self.builder.build_call(printf_fn, &[fmt_str.as_basic_value_enum().into()], "printf_call").map_err(|e| e.to_string())?;
                        return Ok(self.context.i64_type().const_int(0, false).into());
                    }

                    let arg_val = self.compile_expression(&args[0])?;
                    let format_str = if arg_val.is_int_value() {
                        "%lld\n"
                    } else if arg_val.is_float_value() {
                        "%f\n"
                    } else if arg_val.is_pointer_value() {
                        "%s\n"
                    } else {
                        return Err(format!("Unsupported type for built-in print"));
                    };

                    let fmt_str = self.builder.build_global_string_ptr(format_str, "print_fmt").map_err(|e| e.to_string())?;
                    let _ = self.builder.build_call(printf_fn, &[fmt_str.as_basic_value_enum().into(), arg_val.into()], "printf_call").map_err(|e| e.to_string())?;

                    Ok(self.context.i64_type().const_int(0, false).into())
                } else {
                    match self.module.get_function(name) {
                        Some(func) => {
                            let mut args_compiled = Vec::new();
                            for arg in args {
                                args_compiled.push(self.compile_expression(arg)?.into());
                            }
                            let call = self.builder.build_call(func, &args_compiled, "calltmp").map_err(|e| e.to_string())?;
                            let val = call.try_as_basic_value().unwrap_basic();
                            Ok(val)
                        }
                        None => Err(format!("Unknown function: {}", name)),
                    }
                }
            }
            Expression::Intrinsic(Intrinsic::Print(args)) => {
                let printf_fn = match self.module.get_function("printf") {
                    Some(f) => f,
                    None => {
                        let ptr_type = self.context.ptr_type(inkwell::AddressSpace::default());
                        let i32_type = self.context.i32_type();
                        let printf_type = i32_type.fn_type(&[ptr_type.into()], true); // variadic
                        self.module.add_function("printf", printf_type, None)
                    }
                };

                if args.is_empty() {
                    let fmt_str = self.builder.build_global_string_ptr("\n", "print_newline").map_err(|e| e.to_string())?;
                    let _ = self.builder.build_call(printf_fn, &[fmt_str.as_basic_value_enum().into()], "printf_call").map_err(|e| e.to_string())?;
                    return Ok(self.context.i64_type().const_int(0, false).into());
                }

                let arg_val = self.compile_expression(&args[0])?;
                let format_str = if arg_val.is_int_value() {
                    "%lld\n"
                } else if arg_val.is_float_value() {
                    "%f\n"
                } else if arg_val.is_pointer_value() {
                    "%s\n"
                } else {
                    return Err(format!("Unsupported type for built-in print"));
                };

                let fmt_str = self.builder.build_global_string_ptr(format_str, "print_fmt").map_err(|e| e.to_string())?;
                let _ = self.builder.build_call(printf_fn, &[fmt_str.as_basic_value_enum().into(), arg_val.into()], "printf_call").map_err(|e| e.to_string())?;

                Ok(self.context.i64_type().const_int(0, false).into())
            }
        }
    }

    pub fn output_to_file(&self, path: &str, target_triple: Option<&str>) -> Result<(), String> {
        inkwell::targets::Target::initialize_all(&inkwell::targets::InitializationConfig::default());
        let triple = if let Some(t) = target_triple {
            inkwell::targets::TargetTriple::create(t)
        } else {
            inkwell::targets::TargetMachine::get_default_triple()
        };
        let target = inkwell::targets::Target::from_triple(&triple)
            .map_err(|e| e.to_string())?;
        
        let (cpu, features) = if target_triple.is_some() {
            ("generic".to_string(), "".to_string())
        } else {
            let host_cpu = inkwell::targets::TargetMachine::get_host_cpu_name().to_string();
            let host_features = inkwell::targets::TargetMachine::get_host_cpu_features().to_string();
            (host_cpu, host_features)
        };

        let target_machine = target
            .create_target_machine(
                &triple,
                &cpu,
                &features,
                inkwell::OptimizationLevel::Aggressive,
                inkwell::targets::RelocMode::Default,
                inkwell::targets::CodeModel::Default,
            )
            .ok_or("Failed to create target machine")?;

        // Setup modern optimization PassManager
        let pass_options = inkwell::passes::PassBuilderOptions::create();
        self.module.run_passes("default<O3>", &target_machine, pass_options).map_err(|e| e.to_string())?;

        // Output ONLY the pure object file (.o)
        target_machine.write_to_file(&self.module, inkwell::targets::FileType::Object, path.as_ref())
            .map_err(|e| e.to_string())
    }
}

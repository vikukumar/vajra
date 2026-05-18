# Vajra Stage 1 Consolidated Build & Compilation Orchestrator
# Written natively in PowerShell to manage isolated zero-dependency builds

$ErrorActionPreference = "Stop"

# Define the modular dependency order of the Vajra compiler
$source_files = @(
    "vajra-core/runtime/runtime.vj",
    "vajra-core/manifest/manifest.vj",
    "vajra-core/lexer/lexer.vj",
    "vajra-core/parser/parser.vj",
    "vajra-core/emitter/emitter.vj",
    "vajra-core/checker/type_checker.vj",
    "vajra-core/checker/linter.vj",
    "vajra-core/runtime/jit.vj",
    "vajra-core/tester/tester.vj",
    "vajra-core/main.vj"
)

# List of all internally defined functions that should not have duplicate @extern declarations
$internal_funcs = @(
    "ast_get_left", "ast_get_type", "ast_get_name", "ast_get_body_size", "ast_get_body_item", 
    "ast_get_right", "ast_get_args_size", "ast_get_args_item", "ast_get_value", "ast_get_op",
    "substring", "vajra_spawn", "compile_main", "runtime_init",
    "cg_new", "compile_node", "emit_binary_file",
    "malloc", "free", "strcmp", "strlen",
    "lex_next_token", "skip_whitespace_and_comments",
    "ast_new", "parser_advance", "create_parser_state", "parse_program",
    "type_checker_init", "check_node_types",
    "lint_node", "jit_execute_ast",
    "test_suite_init", "run_tests_concurrently", "output_test_diagnostics"
)

Write-Host "====================================================" -ForegroundColor Cyan
Write-Host "Vajra Stage 1 Consolidated Compiler Builder" -ForegroundColor Cyan
Write-Host "====================================================" -ForegroundColor Cyan

# Create dist/ directories if they don't exist
$null = New-Item -ItemType Directory -Force -Path "dist/objects", "dist/bin", "dist/bin/lib"

Write-Host "Consolidating Vajra modular source streams..." -ForegroundColor Green
$code = (Get-Content -Path $source_files -Raw -Encoding utf8) -join "`n"

# Strip duplicate @extern declarations for all internal functions completely and precisely
foreach ($func in $internal_funcs) {
    $strip_regex = "(?mi)^@extern\s*\r?\n\s*fn\s+$func\s*\(.*?\)\s*\r?\n"
    $code = $code -replace $strip_regex, ""
}

# On-the-fly Hindi Keyword Pre-processor for Bootstrap Compiler compatibility
$code = $code -replace '\bkarya\b', 'fn'
$code = $code -replace '\bastu\b', 'var'
$code = $code -replace '\byadi\b', 'if'
$code = $code -replace '\banyatha\b', 'else'
$code = $code -replace '\bloutao\b', 'return'
$code = $code -replace '\bchapo\b', 'print'
$code = $code -replace '\bjabtak\b', 'while'
$code = $code -replace '\bsaty\b', 'true'
$code = $code -replace '\basaty\b', 'false'
$code = $code -replace '\bshunya\b', 'null'

$code = $code -replace "`r", ""

$unified_path = "dist/vajra_unified.vj"
$utf8_nobom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($unified_path, $code, $utf8_nobom)
Write-Host "Unified source consolidated successfully to: $unified_path" -ForegroundColor Green

# Unescape string escape sequences inside string literals using Python
python dist/unescape_source.py

# Compile the unified source into the isolated dist/vajra_unified.o using the bootstrap compiler
Write-Host "Compiling standalone Stage 1 object file using bootstrap compiler..." -ForegroundColor Green
& "./bootstrap/vajrac_v0.1.exe" compile $unified_path -o "dist/vajra_unified.o"

# Compile our custom C runtime bridge and link them together using MSVC with vcvars64.bat initialization
Write-Host "Compiling C runtime and linking final standalone Stage 1 binary..." -ForegroundColor Green
$vcvars_path = "C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
$cmd_line = "`"$vcvars_path`" && cl /c /O2 /MT /Fodist\objects\vj_runtime_bridge.obj vajra-core\runtime\vj_runtime_bridge.c && link dist\vajra_unified.o dist\objects\vj_runtime_bridge.obj /OUT:dist\bin\vajra.exe /SUBSYSTEM:CONSOLE /LARGEADDRESSAWARE:NO"
cmd.exe /c $cmd_line

Write-Host "Consolidated Stage 1 Compiler built successfully under 'dist/bin/vajra.exe'!" -ForegroundColor Green
Write-Host "====================================================" -ForegroundColor Cyan
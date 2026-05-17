# Vajra Stage 1 Consolidated Build & Compilation Orchestrator
# Written natively in PowerShell to manage isolated zero-dependency builds

$ErrorActionPreference = "Stop"

# Define the modular dependency order of the Vajra compiler
$source_files = @(
    "vajra-core/runtime/runtime.vj",
    "vajra-core/manifest/manifest.vj",
    "vajra-core/lexer/lexer.vj",
    "vajra-core/parser/parser.vj",
    "vajra-core/checker/type_checker.vj",
    "vajra-core/checker/linter.vj",
    "vajra-core/runtime/jit.vj",
    "vajra-core/emitter/emitter.vj",
    "vajra-core/tester/tester.vj",
    "vajra-core/main.vj"
)

# List of all internally defined functions that should not have duplicate @extern declarations
$internal_funcs = @(
    "strcmp", "strlen", "get_char", "substring", "malloc", "free", "get_argc", "get_argv_item", "runtime_init",
    "parse_project_manifest", "get_manifest_name", "get_manifest_target", "get_manifest_entry",
    "create_parser_state", "parse_program", "cg_new", "compile_node", "emit_binary_file",
    "type_checker_init", "check_node_types", "lint_node", "jit_execute_ast",
    "test_suite_init", "run_tests_concurrently", "output_test_diagnostics"
)

Write-Host "====================================================" -ForegroundColor Cyan
Write-Host "Vajra Stage 1 Consolidated Compiler Builder" -ForegroundColor Cyan
Write-Host "====================================================" -ForegroundColor Cyan

# Create dist/ directories if they don't exist
$null = New-Item -ItemType Directory -Force -Path "dist/objects", "dist/bin", "dist/bin/lib"

Write-Host "Consolidating Vajra modular source streams..." -ForegroundColor Green
$code = Get-Content -Path $source_files -Raw -Encoding utf8

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

# Devanagari replacements defined via Unicode hex sequences to avoid byte encoding mismatch
$karya1 = "$([char]0x0915)$([char]0x093E)$([char]0x0930)$([char]0x094D)$([char]0x092F)$([char]0x093E)"
$karya2 = "$([char]0x0915)$([char]0x093E)$([char]0x0930)$([char]0x094D)$([char]0x092F)"
$astu = "$([char]0x0905)$([char]0x0938)$([char]0x094D)$([char]0x0924)$([char]0x0941)"
$maan = "$([char]0x092E)$([char]0x093E)$([char]0x0928)"
$yadi = "$([char]0x092F)$([char]0x0926)$([char]0x093F)"
$anyatha = "$([char]0x0905)$([char]0x0928)$([char]0x094D)$([char]0x092F)$([char]0x0925)$([char]0x093E)"
$jabtak = "$([char]0x091C)$([char]0x092C)$([char]0x0924)$([char]0x0915)"
$likho = "$([char]0x0932)$([char]0x093F)$([char]0x0916)$([char]0x094B)"
$chapo = "$([char]0x091B)$([char]0x093E)$([char]0x092A)$([char]0x094B)"
$pratifal = "$([char]0x092A)$([char]0x094D)$([char]0x0930)$([char]0x0924)$([char]0x093F)$([char]0x092B)$([char]0x0932)"
$loutao = "$([char]0x0932)$([char]0x094C)$([char]0x091F)$([char]0x093E)$([char]0x0913)"
$saty = "$([char]0x0938)$([char]0x0924)$([char]0x094D)$([char]0x092F)"
$asaty = "$([char]0x0905)$([char]0x0938)$([char]0x0924)$([char]0x094D)$([char]0x092F)"
$shunya = "$([char]0x0936)$([char]0x0942)$([char]0x0928)$([char]0x094D)$([char]0x092F)"

$code = $code -replace $karya1, 'fn'
$code = $code -replace $karya2, 'fn'
$code = $code -replace $astu, 'var'
$code = $code -replace $maan, 'var'
$code = $code -replace $yadi, 'if'
$code = $code -replace $anyatha, 'else'
$code = $code -replace $jabtak, 'while'
$code = $code -replace $likho, 'print'
$code = $code -replace $chapo, 'print'
$code = $code -replace $pratifal, 'return'
$code = $code -replace $loutao, 'return'
$code = $code -replace $saty, 'true'
$code = $code -replace $asaty, 'false'
$code = $code -replace $shunya, 'null'

$unified_path = "dist/vajra_unified.vj"
$utf8_nobom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($unified_path, $code, $utf8_nobom)
Write-Host "Unified source consolidated successfully to: $unified_path" -ForegroundColor Green

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
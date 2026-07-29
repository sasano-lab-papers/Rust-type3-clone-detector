@echo off
setlocal EnableExtensions

REM ========================================================
REM Rust Clone Detection Pipeline
REM
REM This BAT uses the original token_filter.py without
REM modifying its source code or token-stage parameters.
REM
REM Usage:
REM   run_pipeline.bat <RUST_PROJECT> <OUT_DIR>
REM
REM Example:
REM   run_pipeline.bat "F:\RustProjects\example" "F:\Results\example"
REM ========================================================

if "%~1"=="" (
    echo [ERROR] Missing Rust project directory.
    echo Usage: %~nx0 ^<RUST_PROJECT^> ^<OUT_DIR^>
    pause
    exit /b 1
)

if "%~2"=="" (
    echo [ERROR] Missing output directory.
    echo Usage: %~nx0 ^<RUST_PROJECT^> ^<OUT_DIR^>
    pause
    exit /b 1
)

REM ---------- AST-stage parameters ----------
set "HASH_THRESHOLD=0.65"
set "HASH_DICE_THRESHOLD=0.70"
set "VECTOR_Q=1"
set "VECTOR_THRESHOLD=0.75"
set "PROGRESS_EVERY=5000"

REM ---------- Resolve paths ----------
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..") do set "REPO_ROOT=%%~fI"

for %%I in ("%~1") do set "RUST_PROJECT=%%~fI"
for %%I in ("%~2") do set "OUT_DIR=%%~fI"

set "WORKSPACE_TOML=%REPO_ROOT%\Cargo.toml"
set "TOKEN_SCRIPT=%REPO_ROOT%\scripts\token_filter.py"
set "HASH_SCRIPT=%REPO_ROOT%\scripts\ast_hash_detection.py"
set "VECTOR_SCRIPT=%REPO_ROOT%\scripts\ast_vector_detection.py"

REM Cargo package names from each Cargo.toml.
set "FUNCTION_EXTRACTOR_PACKAGE=rust_extractor"
set "AST_BUILDER_PACKAGE=ast_builder"

REM ---------- Output files ----------
set "FUNCTIONS=%OUT_DIR%\functions_rust.jsonl"
set "DIRECT_CLONES=%OUT_DIR%\direct_clones.jsonl"
set "AST_CANDIDATES=%OUT_DIR%\ast_candidates.jsonl"
set "ALL_TOKEN_CANDIDATES=%OUT_DIR%\all_token_candidates.jsonl"
set "AST_PAIRS=%OUT_DIR%\rust_pairs_with_ast.jsonl"
set "HASH_OUT=%OUT_DIR%\rust_ast_hash.jsonl"
set "VECTOR_OUT=%OUT_DIR%\rust_ast_vector.jsonl"

REM ---------- Check required commands ----------
where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] Cargo was not found.
    echo Install Rust and make sure cargo is available in PATH.
    pause
    exit /b 1
)

where py >nul 2>nul
if errorlevel 1 (
    where python >nul 2>nul
    if errorlevel 1 (
        echo [ERROR] Python was not found.
        pause
        exit /b 1
    )
    set "PYTHON_CMD=python"
) else (
    set "PYTHON_CMD=py"
)

REM ---------- Validate repository files ----------
if not exist "%RUST_PROJECT%\" (
    echo [ERROR] Rust project directory does not exist:
    echo         %RUST_PROJECT%
    pause
    exit /b 1
)

if not exist "%WORKSPACE_TOML%" (
    echo [ERROR] Workspace Cargo.toml was not found:
    echo         %WORKSPACE_TOML%
    pause
    exit /b 1
)

if not exist "%TOKEN_SCRIPT%" (
    echo [ERROR] Missing token script:
    echo         %TOKEN_SCRIPT%
    pause
    exit /b 1
)

if not exist "%HASH_SCRIPT%" (
    echo [ERROR] Missing AST hash script:
    echo         %HASH_SCRIPT%
    pause
    exit /b 1
)

if not exist "%VECTOR_SCRIPT%" (
    echo [ERROR] Missing AST vector script:
    echo         %VECTOR_SCRIPT%
    pause
    exit /b 1
)

if not exist "%OUT_DIR%\" (
    mkdir "%OUT_DIR%"
    if errorlevel 1 (
        echo [ERROR] Could not create output directory:
        echo         %OUT_DIR%
        pause
        exit /b 1
    )
)

echo ========================================
echo Rust Clone Detection Pipeline
echo ========================================
echo Repository:          %REPO_ROOT%
echo Rust project:        %RUST_PROJECT%
echo Output directory:    %OUT_DIR%
echo Hash threshold:      %HASH_THRESHOLD%
echo Hash Dice threshold: %HASH_DICE_THRESHOLD%
echo Vector q:            %VECTOR_Q%
echo Vector threshold:    %VECTOR_THRESHOLD%
echo ========================================
echo.

echo [1/5] Extract Rust functions...
cargo run --release ^
    --manifest-path "%WORKSPACE_TOML%" ^
    -p "%FUNCTION_EXTRACTOR_PACKAGE%" ^
    -- "%RUST_PROJECT%" "%FUNCTIONS%"
if errorlevel 1 goto error

echo.
echo [2/5] Run token-stage detection...
%PYTHON_CMD% "%TOKEN_SCRIPT%" "%FUNCTIONS%" "%OUT_DIR%"
if errorlevel 1 goto error

echo.
echo [3/5] Build AST data...
cargo run --release ^
    --manifest-path "%WORKSPACE_TOML%" ^
    -p "%AST_BUILDER_PACKAGE%" ^
    -- "%AST_CANDIDATES%" "%RUST_PROJECT%" "%AST_PAIRS%"
if errorlevel 1 goto error

echo.
echo [4/5] Run AST hash detection...
%PYTHON_CMD% "%HASH_SCRIPT%" ^
    "%AST_PAIRS%" "%HASH_OUT%" all ^
    --threshold %HASH_THRESHOLD% ^
    --dice-threshold %HASH_DICE_THRESHOLD% ^
    --progress-every %PROGRESS_EVERY%
if errorlevel 1 goto error

echo.
echo [5/5] Run AST vector detection...
%PYTHON_CMD% "%VECTOR_SCRIPT%" ^
    "%AST_PAIRS%" "%VECTOR_OUT%" all ^
    --q %VECTOR_Q% ^
    --threshold %VECTOR_THRESHOLD% ^
    --progress-every %PROGRESS_EVERY%
if errorlevel 1 goto error

echo.
echo ========================================
echo Pipeline finished successfully.
echo ========================================
echo Functions:            %FUNCTIONS%
echo Direct clones:        %DIRECT_CLONES%
echo AST candidates:       %AST_CANDIDATES%
echo All token candidates: %ALL_TOKEN_CANDIDATES%
echo AST pairs:            %AST_PAIRS%
echo AST hash output:      %HASH_OUT%
echo AST vector output:    %VECTOR_OUT%
echo ========================================
pause
exit /b 0

:error
echo.
echo [ERROR] Pipeline failed. Check the message above.
pause
exit /b 1

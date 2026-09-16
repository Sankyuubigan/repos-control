@echo off
cd /d "%~dp0"

echo ========================================
echo Initializing MSVC environment...
echo ========================================
if defined VSINSTALLDIR (
  echo MSVC environment already loaded.
) else (
  if exist "D:\Programs\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" (
    call "D:\Programs\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" >nul
  ) else (
    call "C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvars64.bat" >nul 2>nul
  )
)

REM Reset sccache wrappers so cargo can find cl.exe (rules.md 6.1)
set "CC="
set "CXX="
set "CMAKE_C_COMPILER_LAUNCHER="
set "RUSTC_WRAPPER="
set "CARGO_BUILD_RUSTC_WRAPPER="

echo ========================================
echo cargo check...
echo ========================================
call cargo check --manifest-path src-tauri\Cargo.toml --features custom-protocol
if errorlevel 1 goto check_error
echo.
echo ========================================
echo cargo test (lib)...
echo ========================================
call cargo test --manifest-path src-tauri\Cargo.toml --lib --features custom-protocol
if errorlevel 1 goto check_error
echo.
echo ========================================
echo OK
echo ========================================
goto :eof

:check_error
echo.
echo ========================================
echo ERROR: cargo check failed
echo ========================================
pause
exit /b 1
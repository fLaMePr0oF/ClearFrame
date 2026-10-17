@echo off
setlocal
cd /d "%~dp0"
where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust/Cargo was not found. Install Rust from https://rustup.rs/ and reopen this window.
    pause
    exit /b 1
)
cargo build --release
if errorlevel 1 (
    echo Build failed. Check the output above.
    pause
    exit /b 1
)
echo.
echo Built: %~dp0target\release\cleanframe.exe
pause

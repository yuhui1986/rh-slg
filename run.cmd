@echo off
chcp 65001 >nul
REM Launch the SLG game.
REM
REM Why this script exists:
REM   - The root Cargo.toml has a [package] named "rh-slg" so the tests/ directory
REM     is wired as a workspace integration test target.
REM   - But `cargo run` from the root uses the root [package]'s default-run, and
REM     the root package has no binary and no default-run, so it errors with
REM     "a bin target must be available for `cargo run`".
REM   - The fix: explicitly `cargo run -p slg-app`. This script is just a shortcut.
REM
REM Usage:
REM   run.cmd           - release build (slow build, fast runtime)
REM   run.cmd dev       - debug build
REM   run.cmd clean     - cargo clean

if "%1"=="clean" (
    cargo clean
    exit /b 0
)

if "%1"=="dev" (
    cargo run -p slg-app
    exit /b 0
)

cargo run -p slg-app --release

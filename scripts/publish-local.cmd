@echo off
rem Wrapper for publish-local\publish-local.ps1; arguments are passed through (e.g. -SkipTests).
rem Pauses when the script fails so the error stays readable.
set "PS=powershell"
where pwsh >nul 2>nul && set "PS=pwsh"
%PS% -NoProfile -ExecutionPolicy Bypass -File "%~dp0publish-local\publish-local.ps1" %*
set "CODE=%ERRORLEVEL%"
if not "%CODE%"=="0" (
    echo.
    echo publish-local failed with exit code %CODE%.
    pause
)
exit /b %CODE%

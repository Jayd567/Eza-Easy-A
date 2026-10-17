@echo off
rem Installs Eza for this Windows user: no admin rights needed.
rem Running it again (for example with a newer version) replaces the old install.
rem   - copies eza.exe to %LOCALAPPDATA%\Eza and adds that folder to your PATH
rem   - installs the VS Code extension if VS Code is installed
cd /d "%~dp0"
echo Installing Eza...

set "EZA_HOME=%LOCALAPPDATA%\Eza"
if not exist "%EZA_HOME%" mkdir "%EZA_HOME%"
copy /y "eza.exe" "%EZA_HOME%\eza.exe" >nul
if errorlevel 1 (
    echo Could not copy eza.exe. If Eza is running, close it and try again.
    pause
    exit /b 1
)

rem add to the user PATH (only once)
powershell -NoProfile -Command "$p = [Environment]::GetEnvironmentVariable('Path','User'); if (-not $p) { $p = '' }; $d = Join-Path $env:LOCALAPPDATA 'Eza'; if (($p -split ';') -notcontains $d) { [Environment]::SetEnvironmentVariable('Path', ($p.TrimEnd(';') + ';' + $d).TrimStart(';'), 'User'); Write-Host 'Added Eza to your PATH.' } else { Write-Host 'Eza is already on your PATH.' }"

rem VS Code extension
if exist "%USERPROFILE%\.vscode" (
    if not exist "%USERPROFILE%\.vscode\extensions" mkdir "%USERPROFILE%\.vscode\extensions"
    rem replace an older version completely, so no old files are left behind
    if exist "%USERPROFILE%\.vscode\extensions\eza.eza-lang-0.1.0" rmdir /s /q "%USERPROFILE%\.vscode\extensions\eza.eza-lang-0.1.0"
    xcopy /e /i /y /q "vscode-eza" "%USERPROFILE%\.vscode\extensions\eza.eza-lang-0.1.0" >nul
    echo Installed the VS Code extension - restart VS Code to use it.
    rem teach VS Code's AI chat what Eza looks like, so it doesn't write Python into .eza files
    if not exist "%APPDATA%\Code\User\prompts" mkdir "%APPDATA%\Code\User\prompts"
    copy /y "vscode-eza\ai\eza.instructions.md" "%APPDATA%\Code\User\prompts\eza.instructions.md" >nul
    echo Taught VS Code's AI chat how Eza is written.
) else (
    echo VS Code wasn't found, so the extension was skipped. Run this again after installing VS Code.
)

echo.
echo Done! Open a NEW terminal and try:
echo     eza examples\menu.eza
echo.
pause

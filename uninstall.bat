@echo off
rem Removes what install.bat added.
echo Removing Eza...
powershell -NoProfile -Command "$d = Join-Path $env:LOCALAPPDATA 'Eza'; $p = [Environment]::GetEnvironmentVariable('Path','User'); if ($p) { [Environment]::SetEnvironmentVariable('Path', (($p -split ';') | Where-Object { $_ -and $_ -ne $d }) -join ';', 'User') }"
if exist "%LOCALAPPDATA%\Eza" rmdir /s /q "%LOCALAPPDATA%\Eza"
if exist "%USERPROFILE%\.vscode\extensions\eza.eza-lang-0.1.0" rmdir /s /q "%USERPROFILE%\.vscode\extensions\eza.eza-lang-0.1.0"
echo Eza has been removed. Your own .eza files were not touched.
pause

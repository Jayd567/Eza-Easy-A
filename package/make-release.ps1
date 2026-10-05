# Builds Eza and packs everything a user needs into Eza-Windows.zip, ready to attach to a GitHub release.
# Run from the repo folder:  powershell -ExecutionPolicy Bypass -File package\make-release.ps1
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

cargo build --release --features engine
if ($LASTEXITCODE -ne 0) { throw "the build failed" }

$stage = "target\package\Eza"
if (Test-Path "target\package") { Remove-Item -Recurse -Force "target\package" }
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item "target\release\eza.exe" $stage
Copy-Item "package\install.bat", "package\uninstall.bat", "package\START HERE.txt", "EZA_GUIDE.md", "LICENSE" $stage
Copy-Item -Recurse "vscode-eza" $stage
Copy-Item -Recurse "examples" $stage
foreach ($junk in @("$stage\examples\output", "$stage\examples\tempCodeRunnerFile.eza")) {
    if (Test-Path $junk) { Remove-Item -Recurse -Force $junk }
}
Get-ChildItem $stage -Recurse -Filter *.db | Remove-Item -Force

$zip = "target\Eza-Windows.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path $stage -DestinationPath $zip
"Made $zip ({0:N1} MB)" -f ((Get-Item $zip).Length / 1MB)

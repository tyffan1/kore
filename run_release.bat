@echo off
echo Building Kore (release)...
cargo build --release
if %errorlevel% neq 0 (
    echo Build failed!
    pause
    exit /b %errorlevel%
)
echo Signing executable...
powershell -Command "$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object {$_.Subject -like '*kore-dev*'}; if ($cert) { Set-AuthenticodeSignature -FilePath 'target\release\kore.exe' -Certificate $cert | Out-Null }" 2>nul
echo Launching Kore...
target\release\kore.exe

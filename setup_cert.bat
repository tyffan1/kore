@echo off
echo This script must be run as Administrator!
echo Creating and trusting self-signed certificate for Kore development...
powershell -Command "
$existing = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq 'CN=kore-dev' };
if (-not $existing) {
    $cert = New-SelfSignedCertificate -Subject 'CN=kore-dev' -Type CodeSigning -CertStoreLocation Cert:\CurrentUser\My;
    Write-Host 'Certificate created with thumbprint:' $cert.Thumbprint;
} else {
    Write-Host 'Certificate already exists, skipping creation.';
}
$store = New-Object System.Security.Cryptography.X509Certificates.X509Store('Root', 'CurrentUser');
$store.Open('ReadWrite');
$certs = $store.Certificates | Where-Object { $_.Subject -eq 'CN=kore-dev' };
if (-not $certs) {
    $certToTrust = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq 'CN=kore-dev' } | Select-Object -First 1;
    if ($certToTrust) {
        $store.Add($certToTrust);
        Write-Host 'Certificate added to Trusted Root Certification Authorities.';
    }
} else {
    Write-Host 'Certificate already trusted, skipping.';
}
$store.Close();
"
if %errorlevel% equ 0 (
    echo.
    echo Setup complete! You can now use run.bat or run_release.bat to build and run Kore.
) else (
    echo.
    echo Setup failed. Make sure you run this script as Administrator!
)
pause

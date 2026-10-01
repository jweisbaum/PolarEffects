# Exercise the actual Windows certificate store and generated Tauri config in
# CI with a throwaway certificate. No real credentials or timestamp service.
$ErrorActionPreference = 'Stop'
$keys = @('WINDOWS_CERTIFICATE', 'WINDOWS_CERTIFICATE_PASSWORD', 'WINDOWS_TIMESTAMP_URL', 'GITHUB_OUTPUT', 'RUNNER_TEMP')
$saved = @{}
foreach ($key in $keys) { $saved[$key] = [Environment]::GetEnvironmentVariable($key) }
$directory = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory $directory | Out-Null
$certificate = $null
try {
    $env:RUNNER_TEMP = $directory
    $env:GITHUB_OUTPUT = Join-Path $directory 'outputs.txt'
    $env:WINDOWS_CERTIFICATE = ''
    $env:WINDOWS_CERTIFICATE_PASSWORD = ''
    $env:WINDOWS_TIMESTAMP_URL = ''
    & "$PSScriptRoot/configure-windows-signing.ps1"
    if (Test-Path $env:GITHUB_OUTPUT) { throw 'Unsigned builds must not emit signing configuration' }

    $env:WINDOWS_CERTIFICATE_PASSWORD = 'orphaned'
    $rejected = $false
    try { & "$PSScriptRoot/configure-windows-signing.ps1" } catch { $rejected = $true }
    if (-not $rejected) { throw 'Orphaned password was accepted' }

    $certificate = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=PolarExplorer CI fixture' -CertStoreLocation Cert:\CurrentUser\My
    $password = ConvertTo-SecureString 'fixture-password' -AsPlainText -Force
    $pfx = Join-Path $directory 'fixture.pfx'
    Export-PfxCertificate -Cert $certificate -FilePath $pfx -Password $password | Out-Null
    $env:WINDOWS_CERTIFICATE = [Convert]::ToBase64String([IO.File]::ReadAllBytes($pfx))
    $env:WINDOWS_CERTIFICATE_PASSWORD = 'fixture-password'
    $env:WINDOWS_TIMESTAMP_URL = 'https://timestamp.example.invalid'
    & "$PSScriptRoot/configure-windows-signing.ps1"
    $config = Get-Content (Join-Path $directory 'polarexplorer-signing.json') -Raw | ConvertFrom-Json
    $windows = $config.bundle.windows
    if ($windows.certificateThumbprint -ne $certificate.Thumbprint -or
        $windows.digestAlgorithm -ne 'sha256' -or -not $windows.tsp -or
        $windows.timestampUrl -ne 'https://timestamp.example.invalid/') {
        throw 'Generated config does not describe the imported signing certificate and timestamp service'
    }
    if (Test-Path (Join-Path $directory 'polarexplorer-signing.pfx')) { throw 'Imported PFX was left on disk' }

    $env:WINDOWS_CERTIFICATE_PASSWORD = 'wrong-password'
    $rejected = $false
    try { & "$PSScriptRoot/configure-windows-signing.ps1" } catch { $rejected = $true }
    if (-not $rejected) { throw 'Incorrect password was accepted' }
    if (Test-Path (Join-Path $directory 'polarexplorer-signing.pfx')) { throw 'Failed import left a PFX on disk' }
    Write-Output 'Windows signing setup: absent credentials, partial credentials, PFX import, configuration and failure cleanup passed.'
} finally {
    if ($certificate) { Remove-Item "Cert:\CurrentUser\My\$($certificate.Thumbprint)" -DeleteKey -ErrorAction SilentlyContinue }
    Remove-Item -LiteralPath $directory -Recurse -Force
    foreach ($key in $keys) { [Environment]::SetEnvironmentVariable($key, $saved[$key]) }
}

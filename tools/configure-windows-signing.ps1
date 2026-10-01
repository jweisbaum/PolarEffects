# Optional PFX signing for the ephemeral GitHub runner. No secret is put on
# the command line, in the generated Tauri config, or in a workflow artifact.
$ErrorActionPreference = 'Stop'
if (-not $env:WINDOWS_CERTIFICATE) {
    if ($env:WINDOWS_CERTIFICATE_PASSWORD -or $env:WINDOWS_TIMESTAMP_URL) {
        throw 'Windows signing settings require WINDOWS_CERTIFICATE'
    }
    Write-Output 'No signing credentials: unsigned Windows build.'
    return
}
$timestamp = $null
if (-not [Uri]::TryCreate($env:WINDOWS_TIMESTAMP_URL, [UriKind]::Absolute, [ref]$timestamp) -or
    $timestamp.Scheme -notin @('http', 'https') -or $timestamp.UserInfo) {
    throw 'WINDOWS_TIMESTAMP_URL must be the certificate issuer''s HTTP(S) RFC 3161 timestamp endpoint'
}
$pfx = Join-Path $env:RUNNER_TEMP 'polarexplorer-signing.pfx'
try {
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
    # An empty PFX password is valid.
    $password = [Security.SecureString]::new()
    foreach ($character in ([string]$env:WINDOWS_CERTIFICATE_PASSWORD).ToCharArray()) {
        $password.AppendChar($character)
    }
    $password.MakeReadOnly()
    $imported = @(Import-PfxCertificate -FilePath $pfx -CertStoreLocation Cert:\CurrentUser\My -Password $password)
    $certificates = @($imported | Where-Object { $_.HasPrivateKey -and
        (($_.Extensions | Where-Object { $_.Oid.Value -eq '2.5.29.37' }).EnhancedKeyUsages.Value -contains '1.3.6.1.5.5.7.3.3') })
    if ($certificates.Count -ne 1) { throw 'PFX must contain exactly one code-signing certificate with a private key' }
    $certificate = $certificates[0]
    if ($certificate.NotAfter -le (Get-Date) -or $certificate.NotBefore -gt (Get-Date)) {
        throw 'The Windows signing certificate is not currently valid'
    }
    $config = Join-Path $env:RUNNER_TEMP 'polarexplorer-signing.json'
    @{ bundle = @{ windows = @{
        certificateThumbprint = $certificate.Thumbprint
        digestAlgorithm = 'sha256'
        timestampUrl = $timestamp.AbsoluteUri
        tsp = $true
    } } } | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 $config
    "config=$($config.Replace('\', '/'))" | Out-File -Append -Encoding utf8 $env:GITHUB_OUTPUT
    "thumbprint=$($certificate.Thumbprint)" | Out-File -Append -Encoding utf8 $env:GITHUB_OUTPUT
    Write-Output 'Windows signing and RFC 3161 timestamping configured.'
} finally {
    if (Test-Path $pfx) { Remove-Item -LiteralPath $pfx -Force }
    if ($password) { $password.Dispose() }
}

param(
    [Parameter(Mandatory)][string]$ReleaseDirectory,
    [Parameter(Mandatory)][string]$Thumbprint
)
$ErrorActionPreference = 'Stop'
$installers = @(Get-ChildItem (Join-Path $ReleaseDirectory 'bundle') -Recurse -File |
    Where-Object { $_.Extension -in @('.exe', '.msi') })
if (-not $installers.Count) { throw 'No Windows installers to verify' }
$files = @((Get-Item (Join-Path $ReleaseDirectory 'pe-app.exe'))) + $installers
foreach ($file in $files) {
    $signature = Get-AuthenticodeSignature -LiteralPath $file.FullName
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Thumbprint -ne $Thumbprint -or
        -not $signature.TimeStamperCertificate) {
        throw "Invalid, unexpected or untimestamped signature: $($file.Name) ($($signature.Status))"
    }
    Write-Output "Verified signature and timestamp: $($file.Name)"
}

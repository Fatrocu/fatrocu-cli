param(
    [Parameter(Mandatory=$true)][string]$CertificateThumbprint,
    [Parameter(Mandatory=$true)][string]$PublisherSha256,
    [Parameter(Mandatory=$true)][string]$TimestampServer,
    [Parameter(Mandatory=$true)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path $PSScriptRoot -Parent
if ($env:OS -ne 'Windows_NT') { throw 'Windows packaging requires Windows' }
$certificate = Get-Item -LiteralPath "Cert:\CurrentUser\My\$CertificateThumbprint"
$hasher = [System.Security.Cryptography.SHA256]::Create()
$fingerprint = [System.BitConverter]::ToString($hasher.ComputeHash($certificate.RawData)).Replace('-', '')
if ($fingerprint -ne $PublisherSha256) { throw 'The certificate does not match the confirmed publisher fingerprint' }
cargo build --locked --release --manifest-path (Join-Path $repositoryRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString().ToLowerInvariant().Replace('x64','x86_64').Replace('arm64','aarch64')
$archive = Join-Path $OutputDirectory "fatrocu-cli-windows-$architecture.zip"
if (Test-Path -LiteralPath $archive) { throw 'Candidate archive already exists' }
$files = @('fatrocu-cli.exe', 'fatrocu-updater.exe') | ForEach-Object { Join-Path $repositoryRoot "target\release\$_" }
foreach ($file in $files) {
    $signature = Set-AuthenticodeSignature -LiteralPath $file -Certificate $certificate -HashAlgorithm SHA256 -TimestampServer $TimestampServer
    if ($signature.Status -ne 'Valid') { throw "Signature verification failed: $file" }
}
Compress-Archive -LiteralPath $files -DestinationPath $archive
Get-FileHash -LiteralPath $archive -Algorithm SHA256

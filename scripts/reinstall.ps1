param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$bundleArgs = @("bundle", "--windows", "--package-types", "nsis")

if ($Release) {
    $bundleArgs += "--release"
}

Push-Location $root
try {
    & dx @bundleArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }

    $installer = Get-ChildItem -LiteralPath (Join-Path $root "dist") -Filter "*-setup.exe" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1

    if ($null -eq $installer) {
        throw "No installer found in dist"
    }

    Start-Process -FilePath $installer.FullName -ArgumentList "/S" -Wait
} finally {
    Pop-Location
}

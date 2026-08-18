$ErrorActionPreference = "Stop"

$bundleRoot = Join-Path $PWD "target\release\bundle\wxsmsi"
$wixFile = Join-Path $bundleRoot "installer.wxs"
$wixProjectFile = Join-Path $bundleRoot "installer.wixproj"

# cargo-bundle 0.11 emits a 64-bit component under the 32-bit ProgramFilesFolder
# and uses file-key shortcut targets from separate components. Normalize the
# generated WiX before invoking WiX 6.
& cargo bundle --release --format wxsmsi
$bundleExitCode = $LASTEXITCODE
if (-not (Test-Path -LiteralPath $wixFile)) {
    throw "cargo-bundle did not generate installer.wxs (exit code $bundleExitCode)."
}

$wix = Get-Content -LiteralPath $wixFile -Raw
$wix = $wix.Replace(
    '<StandardDirectory Id="ProgramFilesFolder">',
    '<StandardDirectory Id="ProgramFiles64Folder">'
)
$wix = $wix.Replace(
    '<Component Id="MainExecutableComponent" Guid="*">',
    '<Component Id="MainExecutableComponent" Guid="*" Bitness="always64">'
)
$wix = $wix.Replace('Target="[#ucp_exe]"', 'Target="[INSTALLFOLDER]ucp.exe"')
Set-Content -LiteralPath $wixFile -Value $wix -Encoding UTF8

$wixProject = Get-Content -LiteralPath $wixProjectFile -Raw
$wixProject = $wixProject.Replace(
    '<OutputName>ucp</OutputName>',
    "<OutputName>ucp</OutputName>`r`n    <InstallerPlatform>x64</InstallerPlatform>"
)
Set-Content -LiteralPath $wixProjectFile -Value $wixProject -Encoding UTF8

$configuration = "Release"
Push-Location $bundleRoot
try {
    & dotnet build installer.wixproj -c $configuration
    if ($LASTEXITCODE -ne 0) {
        throw "WiX failed to build the MSI."
    }
}
finally {
    Pop-Location
}

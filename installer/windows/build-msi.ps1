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
$wix = $wix.Replace(
    '<Property Id="WIXUI_INSTALLDIR" Value="INSTALLFOLDER"/>',
    '<Property Id="WIXUI_INSTALLDIR" Value="INSTALLFOLDER"/>
    <Property Id="INSTALLFOLDER" Secure="yes">
      <RegistrySearch Id="InstallFolderSearch" Root="HKLM" Key="Software\UCP\UCP" Name="InstallLocation" Type="raw" Bitness="always64"/>
    </Property>'
)
$wix = $wix.Replace(
    '<File Id="ucp_exe" Source=',
    '<RegistryValue Root="HKLM" Key="Software\UCP\UCP" Name="InstallLocation" Type="string" Value="[INSTALLFOLDER]"/>
           <File Id="ucp_exe" Source='
)
$releaseExecutable = Join-Path $PWD "target\release\ucp.exe"
$customAction = @"
    <Binary Id="PrepareForUpdateBinary" SourceFile="$releaseExecutable" />
    <CustomAction Id="PrepareForUpdate" BinaryRef="PrepareForUpdateBinary" ExeCommand="--prepare-update" Execute="immediate" Return="check" Impersonate="yes" />
    <InstallExecuteSequence>
      <Custom Action="PrepareForUpdate" Before="InstallInitialize" />
    </InstallExecuteSequence>
"@
$wix = $wix.Replace('</Package>', "$customAction`r`n  </Package>")
if (-not $wix.Contains('StandardDirectory Id="ProgramFiles64Folder"')) {
    throw "Failed to configure the MSI for 64-bit Program Files."
}
if (-not $wix.Contains('MainExecutableComponent" Guid="*" Bitness="always64"')) {
    throw "Failed to configure the MSI main component as 64-bit."
}
if (-not $wix.Contains('Id="InstallFolderSearch"')) {
    throw "Failed to add the MSI install-folder registry search."
}
if (-not $wix.Contains('Name="InstallLocation"')) {
    throw "Failed to add the MSI install-location registry value."
}
if (-not $wix.Contains('Id="PrepareForUpdate"')) {
    throw "Failed to add the update preparation custom action."
}
if (-not $wix.Contains('Id="PrepareForUpdateBinary"')) {
    throw "Failed to embed the update preparation executable."
}
if (-not $wix.Contains('Before="InstallInitialize"')) {
    throw "Failed to schedule the update preparation custom action."
}
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

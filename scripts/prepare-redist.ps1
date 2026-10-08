# Collects the DLLs the public installer ships next to opendictate.exe, so it runs on PCs without
# developer tools: DirectML (Parakeet on the GPU) and the Microsoft C++ runtime (app-local deployment).
$ErrorActionPreference = "Stop"
$out = Join-Path $PSScriptRoot "..\src-tauri\redist"
New-Item -ItemType Directory -Force $out | Out-Null

# DirectML: the version ONNX Runtime was built against, from NuGet.
$directml = Join-Path $out "DirectML.dll"
if (-not (Test-Path $directml)) {
    $version = "1.15.4"
    $zip = Join-Path $env:TEMP "directml-$version.zip"
    Invoke-WebRequest "https://www.nuget.org/api/v2/package/Microsoft.AI.DirectML/$version" -OutFile $zip
    $dir = Join-Path $env:TEMP "directml-$version"
    Expand-Archive $zip $dir -Force
    Copy-Item (Join-Path $dir "bin\x64-win\DirectML.dll") $directml
}

# Microsoft C++ runtime from the installed Visual Studio Build Tools.
$vs = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -property installationPath
$crt = Get-ChildItem "$vs\VC\Redist\MSVC" -Directory | Sort-Object Name -Descending |
    ForEach-Object { Join-Path $_.FullName "x64\Microsoft.VC143.CRT" } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $crt) { throw "Microsoft C++ runtime not found under $vs\VC\Redist\MSVC" }
foreach ($dll in "msvcp140.dll", "msvcp140_1.dll", "vcruntime140.dll", "vcruntime140_1.dll") {
    Copy-Item (Join-Path $crt $dll) (Join-Path $out $dll) -Force
}
Get-ChildItem $out | ForEach-Object { "{0,-22} {1,8:N0} KB" -f $_.Name, ($_.Length / 1KB) }

# Installs the Calx loom is pinned to, on Windows, into ~\.calx or CALX_INSTALL,
# which mustn't be there yet, as install.sh does elsewhere:
#
#     .github/actions/calx/install.ps1 .calx-version
param([string]$Pin = '.calx-version')
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

if (-not (Test-Path $Pin)) { throw "calx install: no $Pin" }
$lines = Get-Content $Pin
$version = $lines[0].Trim()
$cpu = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { 'arm64' } else { 'x64' }
$file = "calx-$version-windows-$cpu.zip"
$want = ''
foreach ($line in $lines) {
    $parts = $line.Trim() -split '\s+'
    if ($parts.Count -eq 2 -and $parts[1] -eq $file) { $want = $parts[0].ToLowerInvariant() }
}
if (-not $want) { throw "calx install: $Pin has no SHA-256 for $file" }
$tag = if ($version -match '-nightly\.') { 'nightly' } else { "v$version" }
$root = if ($env:CALX_INSTALL) { $env:CALX_INSTALL } else { Join-Path $HOME '.calx' }
if (Test-Path $root) { throw "calx install: $root is there already; this installs only where nothing is" }

$temp = Join-Path ([IO.Path]::GetTempPath()) ("calx-install-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null
$zip = Join-Path $temp $file
$from = ''
foreach ($releases in 'https://github.com/aaen-studios/loom-calx/releases', 'https://github.com/ellipog/calx/releases') {
    $url = "$releases/download/$tag/$file"
    for ($try = 1; $try -le 5 -and -not $from; $try++) {
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $zip -Headers @{ 'User-Agent' = 'loom-ci' }
        } catch {
            if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) { Write-Host "calx install: $releases has no $tag/$file"; break }
            Start-Sleep -Seconds (3 * $try)
            continue
        }
        $got = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLowerInvariant()
        if ($got -eq $want) { $from = $releases } else { Write-Host "calx install: $url has SHA-256 $got, not the pin's $want"; break }
    }
    if ($from) { break }
}
if (-not $from) { throw "calx install: can't download $file with SHA-256 $want" }
Write-Host "Calx $version from $from, SHA-256 checked"

Add-Type -AssemblyName System.IO.Compression.FileSystem
$unpacked = Join-Path $temp 'unpacked'
[IO.Compression.ZipFile]::ExtractToDirectory($zip, $unpacked)
$calx = Join-Path $unpacked 'calx\bin\calx.exe'
if (-not (Test-Path $calx)) { throw "calx install: $file has no calx\bin\calx.exe" }
New-Item -ItemType Directory -Force -Path (Split-Path $root) | Out-Null
Move-Item -LiteralPath (Join-Path $unpacked 'calx') -Destination $root
Remove-Item -LiteralPath $temp -Recurse -Force

# The toolchain, from the same place, checked against the SHA-256 in toolchain.txt.
$env:CALX_RELEASES = $from
$env:CALX_NO_UPDATE_CHECK = '1'
$exe = Join-Path $root 'bin\calx.exe'
for ($try = 1; ; $try++) {
    & $exe update --toolchain
    if ($LASTEXITCODE -eq 0) { break }
    if ($try -ge 4) { throw 'calx install: can''t install the toolchain' }
    Start-Sleep -Seconds (5 * $try)
}
Remove-Item Env:CALX_RELEASES

$said = & $exe version
if ($said -notmatch [regex]::Escape(" $version ")) { throw "calx install: $exe isn't Calx $version" }
Write-Host $said

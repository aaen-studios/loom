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
$releases = 'https://github.com/ellipog/calx/releases'
$tags = if ($version -match '-nightly\.') { @("v$version", 'nightly') } else { @("v$version") }
$root = if ($env:CALX_INSTALL) { $env:CALX_INSTALL } else { Join-Path $HOME '.calx' }
if (Test-Path $root) { throw "calx install: $root is there already; this installs only where nothing is" }

$temp = Join-Path ([IO.Path]::GetTempPath()) ("calx-install-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null

# Downloads $name from the first of the releases that has it with SHA-256 $sum, into $temp.
function Get-Checked([string]$name, [string]$sum) {
    $out = Join-Path $temp $name
    foreach ($tag in $tags) {
        $url = "$releases/download/$tag/$name"
        for ($try = 1; $try -le 5; $try++) {
            try {
                Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $out -Headers @{ 'User-Agent' = 'loom-ci' }
            } catch {
                if ($_.Exception.Response -and [int]$_.Exception.Response.StatusCode -eq 404) { Write-Host "calx install: no $url"; break }
                Start-Sleep -Seconds (3 * $try)
                continue
            }
            $got = (Get-FileHash -Algorithm SHA256 -Path $out).Hash.ToLowerInvariant()
            if ($got -eq $sum) { Write-Host "$name from $url, SHA-256 checked"; return $out }
            Write-Host "calx install: $url has SHA-256 $got, not $sum"
            break
        }
    }
    throw "calx install: can't download $name with SHA-256 $sum"
}

$zip = Get-Checked $file $want
Add-Type -AssemblyName System.IO.Compression.FileSystem
$unpacked = Join-Path $temp 'unpacked'
[IO.Compression.ZipFile]::ExtractToDirectory($zip, $unpacked)
$calx = Join-Path $unpacked 'calx'
if (-not (Test-Path (Join-Path $calx 'bin\calx.exe'))) { throw "calx install: $file has no calx\bin\calx.exe" }

# The toolchain, which the archive's toolchain.txt names: its file, then its SHA-256.
$note = Join-Path $calx 'toolchain.txt'
if (Test-Path $note) {
    $said = Get-Content $note
    $archive = Get-Checked $said[0].Trim() $said[1].Trim().ToLowerInvariant()
    & "$env:SystemRoot\System32\tar.exe" -xJf $archive -C $calx
    if ($LASTEXITCODE -ne 0) { throw "calx install: can't unpack $($said[0].Trim())" }
    if (-not (Test-Path (Join-Path $calx 'toolchain\bin\clang.exe'))) { throw "calx install: $($said[0].Trim()) has no toolchain\bin\clang.exe" }
}
New-Item -ItemType Directory -Force -Path (Split-Path $root) | Out-Null
Move-Item -LiteralPath $calx -Destination $root
Remove-Item -LiteralPath $temp -Recurse -Force

$exe = Join-Path $root 'bin\calx.exe'
$said = & $exe version
if ($said -notmatch [regex]::Escape(" $version ")) { throw "calx install: $exe isn't Calx $version" }
Write-Host $said

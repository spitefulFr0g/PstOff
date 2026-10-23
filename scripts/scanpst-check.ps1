# Called by scanpst-check.py. Scan-only SCANPST on a copy; prints one JSON line.
param(
  [Parameter(Mandatory=$true)][string]$InputPst,
  [Parameter(Mandatory=$true)][ValidatePattern('^[a-zA-Z0-9-]+$')][string]$Alias,
  [int]$TimeoutSeconds = 600
)
$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:TEMP 'PstOffScan'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$scan = Join-Path $dir ($Alias + '-scan.pst')
$log = Join-Path $dir ($Alias + '-scan.log')
if ((Test-Path $scan) -or (Test-Path $log)) { throw "Refusing existing scan paths" }
Copy-Item -LiteralPath $InputPst -Destination $scan
$hashBefore = (Get-FileHash -LiteralPath $scan -Algorithm SHA256).Hash

$arguments = '-file "' + $scan + '" -silent -no repair -log replace'
$process = Start-Process 'C:\Program Files\Microsoft Office\root\Office16\SCANPST.EXE' -ArgumentList $arguments -PassThru
$finished = $process.WaitForExit($TimeoutSeconds * 1000)
if (-not $finished) { $process.Kill(); $process.WaitForExit() }
$hashAfter = (Get-FileHash -LiteralPath $scan -Algorithm SHA256).Hash
Remove-Item -LiteralPath $scan

$logText = ''
if (Test-Path $log) {
  $bytes = [IO.File]::ReadAllBytes($log)
  $encoding = [Text.Encoding]::UTF8
  if ($bytes.Length -ge 2 -and (($bytes[0] -eq 255 -and $bytes[1] -eq 254) -or $bytes[1] -eq 0)) { $encoding = [Text.Encoding]::Unicode }
  $logText = $encoding.GetString($bytes)
}
# SCANPST's exit code isn't a reliable verdict, so count diagnostic lines in the log.
$diagnostics = @($logText -split "`r?`n" | Where-Object { $_ -match '(?i)\b(error|errors|fatal|invalid|mismatch|corrupt|failed|failure)\b|!!|not recognized|does not recognize|not an Outlook|unable|not valid|\bmissing\b' })
[ordered]@{
  ScanFinished = $finished; ScanExitCode = $process.ExitCode
  ScanInputUnchanged = ($hashBefore -eq $hashAfter)
  LogPath = $log; LogExists = (Test-Path $log); LogReportsFinished = ($logText -match 'Finished Scanning')
  DiagnosticLineCount = $diagnostics.Count
} | ConvertTo-Json -Compress

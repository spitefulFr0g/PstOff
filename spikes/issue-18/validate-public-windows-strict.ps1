param(
 [Parameter(Mandatory=$true)][string]$InputPst,
 [Parameter(Mandatory=$true)][ValidatePattern('^[a-zA-Z0-9-]+$')][string]$Alias
)
# EXPLORATORY SPIKE. Public fixtures only. No account connection or private-file handling.
$ErrorActionPreference='Stop'
$dir=Join-Path $env:TEMP 'PstOffPublicSpike'
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$scan=Join-Path $dir ($Alias+'-scan.pst')
$log=Join-Path $dir ($Alias+'-scan.log')
Copy-Item -LiteralPath $InputPst -Destination $scan -Force
$hashBefore=(Get-FileHash -LiteralPath $scan -Algorithm SHA256).Hash
if(Test-Path $log){Remove-Item -LiteralPath $log}
$arguments='-file "'+$scan+'" -silent -no repair -log replace'
$process=Start-Process 'C:\Program Files\Microsoft Office\root\Office16\SCANPST.EXE' -ArgumentList $arguments -PassThru
$finished=$process.WaitForExit(30000)
if(-not $finished){$process.Kill();$process.WaitForExit()}
$hashAfter=(Get-FileHash -LiteralPath $scan -Algorithm SHA256).Hash
$logText=''
if(Test-Path $log){
 $bytes=[IO.File]::ReadAllBytes($log)
 $encoding=[Text.Encoding]::UTF8
 if($bytes.Length -ge 2 -and (($bytes[0] -eq 255 -and $bytes[1] -eq 254) -or $bytes[1] -eq 0)){$encoding=[Text.Encoding]::Unicode}
 $logText=$encoding.GetString($bytes)
}
# Diagnostic matches are evidence, not a validated generic exit-code contract.
$errorLines=@($logText -split "`r?`n" | Where-Object {$_ -match '(?i)\b(error|errors|fatal|invalid|mismatch|corrupt|failed|failure)\b|!!|not recognized|does not recognize|not an Outlook|unable|not valid|\bmissing\b'})
$summary=[ordered]@{
 Alias=$Alias; ScanFinished=$finished; ScanExitCode=$process.ExitCode;
 ScanInputUnchanged=($hashBefore -eq $hashAfter);
 LogExists=(Test-Path $log); LogReportsFinished=($logText -match 'Finished Scanning');
 DiagnosticLineCount=$errorLines.Count; OutlookChecked=$false;
 Verdict='Unclassified: inspect log and compare against controls; exit code alone is insufficient'
}
# Logs refer only to public fixtures. Keep raw logs local; returned JSON contains no paths or item values.
$summary | ConvertTo-Json -Compress

param(
 [Parameter(Mandatory=$true)][string]$InputPst,
 [Parameter(Mandatory=$true)][ValidatePattern('^[a-zA-Z0-9-]+$')][string]$Alias,
 [switch]$CheckOutlook
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
$errorLines=@($logText -split "`r?`n" | Where-Object {$_ -match '(?i)\b(error|errors|fatal|invalid|mismatch|corrupt|failed|failure)\b|!!|not recognized|does not recognize|not an Outlook|unable|not valid'})
$summary=[ordered]@{
 Alias=$Alias; ScanFinished=$finished; ScanExitCode=$process.ExitCode;
 ScanInputUnchanged=($hashBefore -eq $hashAfter);
 LogExists=(Test-Path $log); LogReportsFinished=($logText -match 'Finished Scanning');
 DiagnosticLineCount=$errorLines.Count; OutlookChecked=$false;
 Verdict='Unclassified: inspect log and compare against controls; exit code alone is insufficient'
}
# Logs refer only to public fixtures. Keep raw logs local; returned JSON contains no paths or item values.
if($CheckOutlook){
 $outlookCopy=Join-Path $dir ($Alias+'-outlook.pst')
 Copy-Item -LiteralPath $InputPst -Destination $outlookCopy -Force
 $app=$null
 if(-not (Get-Process OUTLOOK -ErrorAction SilentlyContinue)){
  Start-Process 'C:\Program Files\Microsoft Office\root\Office16\OUTLOOK.EXE' -ArgumentList '/profile PstOffValidation'
 }
 for($attempt=0;$attempt -lt 20;$attempt++){
  try{$app=[Runtime.InteropServices.Marshal]::GetActiveObject('Outlook.Application');break}catch{Start-Sleep -Milliseconds 500}
 }
 if(-not $app){$app=New-Object -ComObject Outlook.Application}
 if(-not $app){$summary | ConvertTo-Json -Compress;throw 'Could not obtain validation Outlook object'}
 $ns=$app.GetNamespace('MAPI')
 if($ns.CurrentProfileName -ne 'PstOffValidation'){throw 'Expected isolated PstOffValidation profile'}
 $ns.AddStoreEx($outlookCopy,3)
 $store=$null
 for($i=1;$i -le $ns.Stores.Count;$i++){
  $s=$ns.Stores.Item($i)
  if([IO.Path]::GetFileName($s.FilePath) -eq ($Alias+'-outlook.pst')){$store=$s;break}
 }
 if(-not $store){throw 'Sample store not found after AddStoreEx'}
 $root=$store.GetRootFolder()
 $script:folderCount=0;$script:itemCount=0;$script:propertyReadCount=0;$script:readFailures=0
 function Visit($f){
  $script:folderCount++
  for($j=1;$j -le $f.Items.Count;$j++){
   try{
    $item=$f.Items.Item($j);$script:itemCount++
    $null=$item.EntryID;$null=$item.Class;$null=$item.Subject;$script:propertyReadCount+=3
    try{$null=$item.PropertyAccessor.GetProperty('http://schemas.microsoft.com/mapi/proptag/0x0E070003');$script:propertyReadCount++}catch{$script:readFailures++}
   }catch{$script:readFailures++}
  }
  for($k=1;$k -le $f.Folders.Count;$k++){Visit ($f.Folders.Item($k))}
 }
 try{
  Visit $root
  $summary.OutlookChecked=$true;$summary.Folders=$script:folderCount;
  $summary.Items=$script:itemCount;$summary.PropertyReads=$script:propertyReadCount;
  $summary.ReadFailures=$script:readFailures
 }finally{$ns.RemoveStore($root)}
}
$summary | ConvertTo-Json -Compress

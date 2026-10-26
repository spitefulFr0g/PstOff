# Called by outlook-check.py. Drives the classic Outlook that script launched (by process ID)
# through UI Automation: opens one PST copy via File > Open & Export > Open Outlook Data File,
# reads the folder list, the message list's sender column and one opened message, then closes
# the data file and Outlook. Prints one JSON line of observations; the verdict is made in Python.
# It never starts Outlook and never uses the Outlook object model, whose store calls hang (#18).
param(
  [Parameter(Mandatory=$true)][int]$ProcessId,
  [Parameter(Mandatory=$true)][string]$Pst,
  [int]$TimeoutSeconds = 300
)
$ErrorActionPreference = 'Stop'
# UTF-8 without BOM, so garbled senders reach the verdict as they are, not as '?'.
[Console]::OutputEncoding = New-Object Text.UTF8Encoding $false
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms

$AE = [Windows.Automation.AutomationElement]
$Scope = [Windows.Automation.TreeScope]
$CT = [Windows.Automation.ControlType]
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
$StepSeconds = 60
$MaxRowsPerFolder = 50

function Wait-For([scriptblock]$Probe, [string]$What, [int]$Seconds = $StepSeconds) {
  $until = (Get-Date).AddSeconds($Seconds)
  if ($until -gt $deadline) { $until = $deadline }
  while ((Get-Date) -lt $until) {
    $found = & $Probe
    if ($found) { return $found }
    Start-Sleep -Milliseconds 250
  }
  throw "Timed out waiting for $What"
}

# Top-level windows belonging to the launched Outlook only.
function Get-OurWindows {
  $condition = New-Object Windows.Automation.PropertyCondition($AE::ProcessIdProperty, $ProcessId)
  @($AE::RootElement.FindAll($Scope::Children, $condition))
}

function Find-All($Element, $ControlType) {
  $condition = New-Object Windows.Automation.PropertyCondition($AE::ControlTypeProperty, $ControlType)
  @($Element.FindAll($Scope::Descendants, $condition))
}

function Find-Named($Element, [string[]]$Patterns) {
  foreach ($candidate in $Element.FindAll($Scope::Descendants, [Windows.Automation.Condition]::TrueCondition)) {
    $name = $candidate.Current.Name
    foreach ($pattern in $Patterns) { if ($name -like $pattern) { return $candidate } }
  }
  $null
}

function Find-InOurWindows([string[]]$Patterns) {
  foreach ($window in Get-OurWindows) {
    $found = Find-Named $window $Patterns
    if ($found) { return $found }
  }
  $null
}

function Get-Pattern($Element, $Pattern) {
  $out = $null
  if ($Element.TryGetCurrentPattern($Pattern, [ref]$out)) { return $out }
  $null
}

function Invoke-Element($Element) {
  # Backstage labels a Group with the command name and puts the actual button inside.
  # Require an unambiguous enabled command; the caller still verifies that the dialog opens.
  if ($Element.Current.ControlType -eq $CT::Group) {
    $commands = @($Element.FindAll($Scope::Descendants, [Windows.Automation.Condition]::TrueCondition) |
      Where-Object { $_.Current.ControlType -in @($CT::Button, $CT::MenuItem) -and $_.Current.IsEnabled })
    if ($commands.Count -ne 1) { throw "Expected one actionable control in the Outlook command group, found $($commands.Count)" }
    $Element = $commands[0]
  }
  $invoke = Get-Pattern $Element ([Windows.Automation.InvokePattern]::Pattern)
  if ($invoke) { $invoke.Invoke(); return }
  $select = Get-Pattern $Element ([Windows.Automation.SelectionItemPattern]::Pattern)
  if ($select) { $select.Select(); return }
  $expand = Get-Pattern $Element ([Windows.Automation.ExpandCollapsePattern]::Pattern)
  if ($expand) { $expand.Expand(); return }
  throw "Element '$($Element.Current.Name)' can't be invoked"
}

function Get-Id($Element) { ($Element.GetRuntimeId() -join '.') }

function Get-Children($Element, $ControlType) {
  $condition = New-Object Windows.Automation.PropertyCondition($AE::ControlTypeProperty, $ControlType)
  @($Element.FindAll($Scope::Children, $condition))
}

function Get-MainWindow {
  Wait-For { Get-OurWindows | Where-Object { $_.Current.ClassName -eq 'rctrl_renwnd32' } | Select-Object -First 1 } 'the Outlook main window' 120
}

function Get-FolderTree($Main) {
  Wait-For { Find-All $Main $CT::Tree | Select-Object -First 1 } 'the folder pane'
}

function Test-ReducedFunctionality($Main) {
  $patterns = @('*Unlicensed Product*', '*Reduced Functionality*', '*Most features of Outlook have been disabled*', '*Product Deactivated*', '*Activation required*')
  if ($Main.Current.Name -match 'Unlicensed|Reduced Functionality') { return $true }
  [bool](Find-Named $Main $patterns)
}

function Find-ErrorDialog([string[]]$KnownIds) {
  foreach ($window in Get-OurWindows) {
    foreach ($dialog in @($window) + (Find-All $window $CT::Window)) {
      if ($dialog.Current.ClassName -ne '#32770' -or $KnownIds -contains (Get-Id $dialog)) { continue }
      if (Find-All $dialog $CT::Edit | Where-Object { $_.Current.AutomationId -eq '1148' }) { continue }
      return $dialog
    }
  }
  $null
}

function Read-Folders($Item, [string]$Prefix, $Into) {
  $expand = Get-Pattern $Item ([Windows.Automation.ExpandCollapsePattern]::Pattern)
  if ($expand -and $expand.Current.ExpandCollapseState -ne 'LeafNode') { $expand.Expand(); Start-Sleep -Milliseconds 300 }
  foreach ($child in Get-Children $Item $CT::TreeItem) {
    $name = $child.Current.Name
    $Into.Add([pscustomobject]@{ Path = ($Prefix + $name); Name = $name; Element = $child; TopLevel = ($Prefix -eq '') }) | Out-Null
    Read-Folders $child ($Prefix + $name + '/') $Into
  }
}

# The sender column, read from the column header named From and the matching cell in each row.
function Read-Senders($Main, [string]$Folder) {
  $list = Find-All $Main $CT::Table | Select-Object -First 1
  if (-not $list) { $list = Find-All $Main $CT::DataGrid | Select-Object -First 1 }
  if (-not $list) { return @() }
  $rows = @(Find-All $list $CT::DataItem) + @(Find-All $list $CT::ListItem) | Select-Object -First $MaxRowsPerFolder
  $messages = New-Object Collections.ArrayList
  foreach ($row in $rows) {
    $sender = $null
    foreach ($cell in $row.FindAll($Scope::Children, [Windows.Automation.Condition]::TrueCondition)) {
      $name = $cell.Current.Name
      $value = Get-Pattern $cell ([Windows.Automation.ValuePattern]::Pattern)
      if ($name -eq 'From' -and $value) { $sender = $value.Current.Value; break }
      if ($name -like 'From *') { $sender = $name.Substring(5); break }
    }
    if ($null -eq $sender -and $row.Current.Name -match '(?:^|,\s*)From:?\s+(.*?)(?:,\s|\s+Subject\b|$)') { $sender = $Matches[1] }
    if ($null -eq $sender) { throw "Sender column not found in folder '$Folder'" }
    $messages.Add([pscustomobject]@{ Folder = $Folder; Sender = $sender; Row = $row }) | Out-Null
  }
  $messages
}

function Open-Message($Row) {
  $before = @(Get-OurWindows | ForEach-Object { Get-Id $_ })
  $select = Get-Pattern $Row ([Windows.Automation.SelectionItemPattern]::Pattern)
  if ($select) { $select.Select() }
  $invoke = Get-Pattern $Row ([Windows.Automation.InvokePattern]::Pattern)
  if ($invoke) { $invoke.Invoke() } else { $Row.SetFocus(); [Windows.Forms.SendKeys]::SendWait('{ENTER}') }
  $inspector = Wait-For {
    Get-OurWindows | Where-Object { $_.Current.ClassName -eq 'rctrl_renwnd32' -and $before -notcontains (Get-Id $_) } | Select-Object -First 1
  } 'the opened message window'
  $body = [bool](Find-All $inspector $CT::Document | Select-Object -First 1)
  $window = Get-Pattern $inspector ([Windows.Automation.WindowPattern]::Pattern)
  if ($window) { $window.Close() }
  [ordered]@{ WindowFound = $true; BodyFound = $body }
}

function Close-DataFile($Store, $Tree) {
  $storeId = Get-Id $Store
  $Store.SetFocus()
  $select = Get-Pattern $Store ([Windows.Automation.SelectionItemPattern]::Pattern)
  if ($select) { $select.Select() }
  [Windows.Forms.SendKeys]::SendWait('+{F10}')
  $close = Wait-For { Find-InOurWindows @('Close "*"', 'Close *') | Where-Object { $_.Current.ControlType -eq $CT::MenuItem } } 'the Close data file menu item'
  Invoke-Element $close
  [bool](Wait-For { -not (Get-Children $Tree $CT::TreeItem | Where-Object { (Get-Id $_) -eq $storeId }) } 'the data file to close')
}

$observed = [ordered]@{
  Opened = $false; OpenError = $null; StoreName = $null; Folders = @(); Messages = @()
  OpenedMessage = $null; ReducedFunctionality = $null; DataFileClosed = $false
}
$main = Get-MainWindow
$observed.ReducedFunctionality = Test-ReducedFunctionality $main
$tree = Get-FolderTree $main
$storesBefore = @(Get-Children $tree $CT::TreeItem | ForEach-Object { Get-Id $_ })
$dialogsBefore = @(Get-OurWindows | ForEach-Object { Get-Id $_ })

# File > Open & Export > Open Outlook Data File
Invoke-Element (Wait-For { Find-Named $main @('File Tab', 'File') | Where-Object { $_.Current.ControlType -in @($CT::Button, $CT::TabItem, $CT::MenuItem) } } 'the File tab')
Invoke-Element (Wait-For { Find-Named $main @('Open & Export') } 'Open & Export')
Invoke-Element (Wait-For { Find-Named $main @('Open Outlook Data File*') } 'Open Outlook Data File')

$fileName = Wait-For {
  foreach ($window in Get-OurWindows) {
    $edit = Find-All $window $CT::Edit | Where-Object { $_.Current.AutomationId -eq '1148' } | Select-Object -First 1
    if ($edit) { return $edit }
  }
} 'the Open Outlook Data File dialog'
(Get-Pattern $fileName ([Windows.Automation.ValuePattern]::Pattern)).SetValue($Pst)
$dialog = Wait-For { Get-OurWindows | Where-Object { Find-All $_ $CT::Edit | Where-Object { $_.Current.AutomationId -eq '1148' } } | Select-Object -First 1 } 'the file dialog window'
Invoke-Element (Wait-For { Find-All $dialog $CT::Button | Where-Object { $_.Current.AutomationId -eq '1' } | Select-Object -First 1 } 'the Open button')

# Either a new store appears in the folder pane or Outlook shows an error dialog.
$outcome = Wait-For {
  $new = Get-Children $tree $CT::TreeItem | Where-Object { $storesBefore -notcontains (Get-Id $_) } | Select-Object -First 1
  if ($new) { return @{ Store = $new } }
  $failure = Find-ErrorDialog $dialogsBefore
  if ($failure) { return @{ Error = $failure } }
} 'the data file to open'

if ($outcome.Error) {
  $texts = @(Find-All $outcome.Error $CT::Text | ForEach-Object { $_.Current.Name } | Where-Object { $_ })
  $observed.OpenError = (($texts -join ' ') -replace [regex]::Escape($Pst), '<copy>')
  $ok = Find-All $outcome.Error $CT::Button | Select-Object -First 1
  if ($ok) { Invoke-Element $ok }
  $observed.DataFileClosed = $true
} else {
  $store = $outcome.Store
  $observed.Opened = $true
  $observed.StoreName = $store.Current.Name
  $folders = New-Object Collections.ArrayList
  Read-Folders $store '' $folders
  $observed.Folders = @($folders | Where-Object { $_.TopLevel } | ForEach-Object { $_.Name })
  $messages = New-Object Collections.ArrayList
  foreach ($folder in $folders) {
    Invoke-Element $folder.Element
    Start-Sleep -Milliseconds 800
    foreach ($message in Read-Senders $main $folder.Path) { $messages.Add($message) | Out-Null }
  }
  $observed.Messages = @($messages | ForEach-Object { [ordered]@{ Folder = $_.Folder; Sender = $_.Sender } })
  if ($messages.Count) {
    Invoke-Element (($folders | Where-Object { $_.Path -eq $messages[0].Folder }) | Select-Object -First 1).Element
    Start-Sleep -Milliseconds 800
    $fresh = @(Read-Senders $main $messages[0].Folder)
    $observed.OpenedMessage = Open-Message $fresh[0].Row
  }
  $observed.DataFileClosed = Close-DataFile $store $tree
}

(Get-Pattern $main ([Windows.Automation.WindowPattern]::Pattern)).Close()
[pscustomobject]$observed | ConvertTo-Json -Compress -Depth 5

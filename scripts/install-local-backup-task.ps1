param(
    [Parameter(Mandatory)][string]$Config,
    [string]$TaskName = 'Alexandria Encrypted Off-server Backup',
    [string]$At = '05:00',
    [string]$Python = 'python',
    [switch]$Replace
)
$ErrorActionPreference = 'Stop'
if ($TaskName -notmatch '^[A-Za-z0-9][A-Za-z0-9 _-]{0,120}$') { throw 'Invalid task name' }
$configPath = (Resolve-Path -LiteralPath $Config).Path
$c = Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
$root = [IO.Path]::GetFullPath($c.backup_root)
if (-not (Test-Path -LiteralPath $root -PathType Container)) { throw 'Create the private backup root first' }
if ((Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue) -and -not $Replace) { throw 'Task exists; use -Replace after reviewing its action' }
$pythonPath = (Get-Command $Python -ErrorAction Stop).Source
$tools = Join-Path $root 'tools'
New-Item -ItemType Directory -Path $tools -Force | Out-Null
foreach ($name in @('backup-to-local-pc.ps1','portable-pc-backup.py')) {
    $source = Join-Path $PSScriptRoot $name
    $destination = Join-Path $tools $name
    if ([IO.Path]::GetFullPath($source) -ne [IO.Path]::GetFullPath($destination)) { Copy-Item -LiteralPath $source -Destination $destination -Force }
}
$script = Join-Path $tools 'backup-to-local-pc.ps1'
$action = New-ScheduledTaskAction -Execute "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -Argument "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$script`" -Config `"$configPath`" -Python `"$pythonPath`" -Monthly"
$user = [Security.Principal.WindowsIdentity]::GetCurrent().Name
# Monthly trigger via Task Scheduler COM; do not temporarily register a daily job.
$service = New-Object -ComObject 'Schedule.Service'
$service.Connect()
$definition = $service.NewTask(0)
$definition.RegistrationInfo.Description = 'Pull and fully verify the configured encrypted repository; requires this user session and SSH key.'
$definition.Principal.UserId = $user
$definition.Principal.LogonType = 3
$definition.Settings.StartWhenAvailable = $true
$definition.Settings.DisallowStartIfOnBatteries = $false
$definition.Settings.StopIfGoingOnBatteries = $false
$definition.Settings.ExecutionTimeLimit = 'PT3H'
$definition.Settings.MultipleInstances = 2
$clock = [DateTime]::ParseExact($At,'HH:mm',[Globalization.CultureInfo]::InvariantCulture)
$calendar = $definition.Triggers.Create(4)
$calendar.StartBoundary = (Get-Date).Date.AddHours($clock.Hour).AddMinutes($clock.Minute).ToString('s')
$calendar.DaysOfMonth = 1
$calendar.MonthsOfYear = 4095
$logon = $definition.Triggers.Create(9)
$logon.UserId = $user
$exec = $definition.Actions.Create(0)
$exec.Path = $action.Execute
$exec.Arguments = $action.Arguments
Register-ScheduledTask -TaskName $TaskName -Xml $definition.XmlText -Force:$Replace | Out-Null
Export-ScheduledTask -TaskName $TaskName | Set-Content -LiteralPath (Join-Path $root 'scheduled-task.xml') -Encoding Unicode

param([string]$BackupRoot = 'C:\Users\luisg\Backups\culturaespiritual\alexandria-automated')
$ErrorActionPreference = 'Stop'
$backupTaskScript = Join-Path $BackupRoot 'backup-to-local-pc.ps1'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'backup-to-local-pc.ps1') -Destination $backupTaskScript -Force
$backupUser = [Security.Principal.WindowsIdentity]::GetCurrent().Name
$backupAction = New-ScheduledTaskAction -Execute "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" `
    -Argument "-NoProfile -NonInteractive -ExecutionPolicy Bypass -File `"$backupTaskScript`" -BackupRoot `"$BackupRoot`""
$backupTriggers = @((New-ScheduledTaskTrigger -Daily -At '02:30'), (New-ScheduledTaskTrigger -AtLogOn -User $backupUser))
$backupSettings = New-ScheduledTaskSettingsSet -StartWhenAvailable -WakeToRun -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Hours 3) -RestartCount 3 `
    -RestartInterval (New-TimeSpan -Minutes 15) -MultipleInstances IgnoreNew
$backupPrincipal = New-ScheduledTaskPrincipal -UserId $backupUser -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName 'Alexandria Encrypted Off-server Backup' -Action $backupAction `
    -Trigger $backupTriggers -Settings $backupSettings -Principal $backupPrincipal `
    -Description 'Pull and fully verify encrypted Alexandria staging backups on this PC. Requires the user session and SSH key.' -Force
$backupXml = Export-ScheduledTask -TaskName 'Alexandria Encrypted Off-server Backup'
$backupMonths = '<ScheduleByMonth><DaysOfMonth><Day>1</Day></DaysOfMonth><Months><January/><February/><March/><April/><May/><June/><July/><August/><September/><October/><November/><December/></Months></ScheduleByMonth>'
$backupXml = [regex]::Replace($backupXml, '(?s)<ScheduleByDay>.*?</ScheduleByDay>', $backupMonths)
Register-ScheduledTask -TaskName 'Alexandria Encrypted Off-server Backup' -Xml $backupXml -Force | Out-Null
Export-ScheduledTask -TaskName 'Alexandria Encrypted Off-server Backup' |
    Set-Content -LiteralPath (Join-Path $BackupRoot 'scheduled-task.xml') -Encoding Unicode

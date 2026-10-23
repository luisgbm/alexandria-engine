param(
    [string]$BackupRoot = 'C:\Users\luisg\Backups\culturaespiritual\alexandria-automated',
    [string]$PasswordFile = 'C:\Users\luisg\Backups\culturaespiritual\alexandria-backup-recovery\password'
)
$ErrorActionPreference = 'Stop'
$backupResolvedRoot = [IO.Path]::GetFullPath($BackupRoot).TrimEnd('\')
$backupMutex = [Threading.Mutex]::new($false, 'Local\AlexandriaEncryptedBackup')
if (-not $backupMutex.WaitOne(0)) { throw 'Another Alexandria backup is running' }
$backupStamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$backupIncoming = Join-Path $backupResolvedRoot ('incoming-' + $backupStamp)
$backupCurrent = Join-Path $backupResolvedRoot 'current'
$backupPrevious = Join-Path $backupResolvedRoot 'previous'
$backupRestic = Join-Path $backupResolvedRoot 'tools\restic_0.16.4_windows_amd64.exe'
$backupSsh = "$env:WINDIR\System32\OpenSSH\ssh.exe"
$backupScp = "$env:WINDIR\System32\OpenSSH\scp.exe"
$backupTar = "$env:WINDIR\System32\tar.exe"
$backupLog = Join-Path $backupResolvedRoot ('backup-' + $backupStamp + '.log')
try {
    Start-Transcript -Path $backupLog | Out-Null
    # Recover an interrupted directory swap before downloading another copy.
    if (-not (Test-Path -LiteralPath $backupCurrent) -and (Test-Path -LiteralPath $backupPrevious)) {
        Move-Item -LiteralPath $backupPrevious -Destination $backupCurrent
    }
    New-Item -ItemType Directory -Path $backupIncoming | Out-Null
    Write-Output "$(Get-Date -Format o) Preparing encrypted server snapshot"
    & $backupSsh -n -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 alexandria "sudo -n su -c 'systemctl start alexandria-backup.service'"
    if ($LASTEXITCODE -ne 0) { throw 'Server snapshot/encryption failed' }
    $backupRemote = (& $backupSsh -n -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 alexandria 'cat /var/lib/alexandria-backup/latest-export').Trim()
    if ($LASTEXITCODE -ne 0 -or $backupRemote -notmatch '^/var/lib/alexandria-backup/exports/[0-9TZ]+\.tar$') {
        throw 'Invalid encrypted export path'
    }
    $backupArchive = Join-Path $backupIncoming 'repository.tar'
    Write-Output "$(Get-Date -Format o) Downloading encrypted repository"
    & $backupScp -q -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 "alexandria:$backupRemote" $backupArchive
    if ($LASTEXITCODE -ne 0) { throw 'Encrypted repository download failed' }
    Write-Output "$(Get-Date -Format o) Extracting encrypted repository"
    & $backupTar -xf $backupArchive -C $backupIncoming
    if ($LASTEXITCODE -ne 0) { throw 'Encrypted repository extraction failed' }
    Write-Output "$(Get-Date -Format o) Verifying every encrypted repository pack"
    & $backupRestic --repo (Join-Path $backupIncoming 'repository') --password-file $PasswordFile check --read-data
    if ($LASTEXITCODE -ne 0) { throw 'Local repository verification failed; previous backup preserved' }
    if ((Test-Path -LiteralPath $backupCurrent) -and (Test-Path -LiteralPath $backupPrevious)) {
        if ([IO.Path]::GetFullPath($backupPrevious) -ne "$backupResolvedRoot\previous") { throw 'Unsafe previous path' }
        Remove-Item -LiteralPath $backupPrevious -Recurse -Force
    }
    if (Test-Path -LiteralPath $backupCurrent) { Move-Item -LiteralPath $backupCurrent -Destination $backupPrevious }
    try { Move-Item -LiteralPath $backupIncoming -Destination $backupCurrent }
    catch {
        if (Test-Path -LiteralPath $backupPrevious) { Move-Item -LiteralPath $backupPrevious -Destination $backupCurrent }
        throw
    }
    Remove-Item -LiteralPath (Join-Path $backupCurrent 'repository.tar')
    @{ completed_at = (Get-Date).ToUniversalTime().ToString('o'); source = 'alexandria'; verified = $true } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $backupResolvedRoot 'last-success.json') -Encoding UTF8
    Write-Output 'Encrypted off-server backup downloaded and fully verified on this PC.'
}
catch {
    @{ failed_at = (Get-Date).ToUniversalTime().ToString('o'); error = $_.Exception.Message } |
        ConvertTo-Json | Set-Content -LiteralPath (Join-Path $backupResolvedRoot 'last-failure.json') -Encoding UTF8
    throw
}
finally {
    Stop-Transcript -ErrorAction SilentlyContinue | Out-Null
    $backupMutex.ReleaseMutex()
    $backupMutex.Dispose()
}

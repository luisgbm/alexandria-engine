param(
    [string]$BackupRoot = 'C:\Users\luisg\Backups\culturaespiritual\alexandria-automated',
    [string]$PasswordFile = 'C:\Users\luisg\Backups\culturaespiritual\alexandria-backup-recovery\password',
    [switch]$Force
)
$ErrorActionPreference = 'Stop'
$backupSuccessPath = Join-Path $BackupRoot 'last-success.json'
if (-not $Force -and (Test-Path -LiteralPath $backupSuccessPath)) {
    $backupLast = [DateTimeOffset]::Parse((Get-Content -Raw -LiteralPath $backupSuccessPath | ConvertFrom-Json).completed_at).LocalDateTime
    if ($backupLast.ToString('yyyy-MM') -eq (Get-Date).ToString('yyyy-MM')) {
        Write-Output 'This month already has a verified off-server backup.'
        exit 0
    }
}
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
    & $backupSsh -n -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 alexandria "sudo -n su -c 'python3 /usr/local/lib/alexandria-backup/prepare-backup-delta.py --ensure-monthly'"
    if ($LASTEXITCODE -ne 0) { throw 'Server snapshot/encryption failed' }
    $backupClient = @{}
    $backupRepository = Join-Path $backupCurrent 'repository'
    if (Test-Path -LiteralPath $backupRepository) {
        Get-ChildItem -LiteralPath $backupRepository -Recurse -File | ForEach-Object {
            $backupRelative = $_.FullName.Substring($backupRepository.Length + 1).Replace('\', '/')
            $backupClient[$backupRelative] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        Copy-Item -LiteralPath $backupRepository -Destination (Join-Path $backupIncoming 'repository') -Recurse
    }
    else { New-Item -ItemType Directory -Path (Join-Path $backupIncoming 'repository') | Out-Null }
    $backupClient | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $backupIncoming 'client.json') -Encoding UTF8
    & $backupScp -q -o BatchMode=yes -o ConnectTimeout=20 (Join-Path $backupIncoming 'client.json') 'alexandria:/home/luisg/alexandria-backup-client.json'
    if ($LASTEXITCODE -ne 0) { throw 'Client manifest upload failed' }
    & $backupSsh -n -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 alexandria "sudo -n su -c 'python3 /usr/local/lib/alexandria-backup/prepare-backup-delta.py'"
    if ($LASTEXITCODE -ne 0) { throw 'Encrypted delta preparation failed' }
    Write-Output "$(Get-Date -Format o) Downloading changed encrypted files only"
    & $backupScp -q -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 alexandria:/var/lib/alexandria-backup/delta/delta.tar alexandria:/var/lib/alexandria-backup/delta/manifest.json $backupIncoming
    if ($LASTEXITCODE -ne 0) { throw 'Encrypted delta download failed' }
    & $backupTar -xf (Join-Path $backupIncoming 'delta.tar') -C $backupIncoming
    if ($LASTEXITCODE -ne 0) { throw 'Encrypted delta extraction failed' }
    $backupManifest = Get-Content -Raw -LiteralPath (Join-Path $backupIncoming 'manifest.json') | ConvertFrom-Json
    $backupExpected = @{}
    $backupStagedRepo = Join-Path $backupIncoming 'repository'
    foreach ($backupProperty in $backupManifest.PSObject.Properties) {
        if ($backupProperty.Name -notmatch '^(config|(?:data|index|keys|snapshots)/[0-9a-f/]+)$' -or $backupProperty.Value -notmatch '^[0-9a-f]{64}$') {
            throw 'Invalid repository manifest entry'
        }
        $backupExpected[$backupProperty.Name] = $true
        $backupFileHash = (Get-FileHash -LiteralPath (Join-Path $backupStagedRepo $backupProperty.Name) -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($backupFileHash -ne $backupProperty.Value) { throw 'Encrypted repository file hash mismatch' }
    }
    Get-ChildItem -LiteralPath $backupStagedRepo -Recurse -File | ForEach-Object {
        $backupRelative = $_.FullName.Substring($backupStagedRepo.Length + 1).Replace('\', '/')
        if (-not $backupExpected.ContainsKey($backupRelative)) { Remove-Item -LiteralPath $_.FullName }
    }
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
    Remove-Item -LiteralPath (Join-Path $backupCurrent 'delta.tar')
    if (Test-Path -LiteralPath $backupPrevious) {
        if ([IO.Path]::GetFullPath($backupPrevious) -ne "$backupResolvedRoot\previous") { throw 'Unsafe previous path' }
        Remove-Item -LiteralPath $backupPrevious -Recurse -Force
    }
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

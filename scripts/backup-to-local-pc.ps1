# A configuration-driven wrapper for scripts/portable-pc-backup.py.
param(
    [Parameter(Mandatory)][string]$Config,
    [string]$Python = 'python',
    [switch]$Fresh,
    [switch]$Monthly,
    [switch]$Force
)
$ErrorActionPreference = 'Stop'
$backupConfig = Get-Content -LiteralPath $Config -Raw | ConvertFrom-Json
$backupRoot = [IO.Path]::GetFullPath($backupConfig.backup_root)
if (-not (Test-Path -LiteralPath $backupRoot -PathType Container)) { throw 'Create the private backup root first' }
$successFile = Join-Path $backupRoot 'last-success.json'
if ($Monthly -and -not $Force -and (Test-Path -LiteralPath $successFile)) {
    $completed = [DateTimeOffset]::Parse((Get-Content -LiteralPath $successFile -Raw | ConvertFrom-Json).completed_at)
    if ($completed.LocalDateTime.ToString('yyyy-MM') -eq (Get-Date).ToString('yyyy-MM')) { exit 0 }
}
$helper = Join-Path $PSScriptRoot 'portable-pc-backup.py'
if (-not (Test-Path -LiteralPath $helper -PathType Leaf)) { throw 'Keep portable-pc-backup.py beside this wrapper' }
$arguments = @($helper, '--config', [IO.Path]::GetFullPath($Config))
if ($Fresh) { $arguments += '--fresh' }
& $Python @arguments
if ($LASTEXITCODE -ne 0) { throw 'Encrypted PC backup or verification failed' }

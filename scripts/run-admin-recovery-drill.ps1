param([string]$RecoveryFile = 'C:\Users\luisg\Backups\culturaespiritual\admin-staging-https-secrets\recovery-secret.dpapi')
$ErrorActionPreference = 'Stop'
$recoveryTemp = Join-Path $env:TEMP ('alexandria-recovery-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $recoveryTemp | Out-Null
$recoveryAcl = Get-Acl -LiteralPath $recoveryTemp
$recoveryAcl.SetAccessRuleProtection($true, $false)
$recoveryAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
    [Security.Principal.WindowsIdentity]::GetCurrent().User, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
$recoveryAcl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
    [Security.Principal.SecurityIdentifier]::new('S-1-5-18'), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
Set-Acl -LiteralPath $recoveryTemp -AclObject $recoveryAcl
$recoveryPlain = $null
try {
    $recoveryPlain = [Security.Cryptography.ProtectedData]::Unprotect(
        [IO.File]::ReadAllBytes($RecoveryFile), $null, [Security.Cryptography.DataProtectionScope]::CurrentUser)
    if ([Text.Encoding]::UTF8.GetString($recoveryPlain).Trim() -notmatch '^[a-f0-9]{64}$') { throw 'Invalid recovery material' }
    [IO.File]::WriteAllBytes((Join-Path $recoveryTemp 'offline-secret'), $recoveryPlain)
    & ssh -n -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=15 alexandria 'umask 077; mkdir -p /home/luisg/alexandria-recovery-drill-upload; chmod 700 /home/luisg/alexandria-recovery-drill-upload'
    if ($LASTEXITCODE -ne 0) { throw 'Private upload directory unavailable' }
    & scp -q -o BatchMode=yes (Join-Path $recoveryTemp 'offline-secret') 'alexandria:/home/luisg/alexandria-recovery-drill-upload/offline-secret'
    if ($LASTEXITCODE -ne 0) { throw 'Private recovery-material transfer failed' }
    & ssh -n -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 alexandria "sudo -n su -c 'python3 /home/luisg/alexandria-backup-install/validate-admin-recovery.py /home/luisg/alexandria-recovery-drill-upload/offline-secret'"
    if ($LASTEXITCODE -ne 0) { throw 'Isolated recovery rehearsal failed' }
}
finally {
    if ($null -ne $recoveryPlain) { [Array]::Clear($recoveryPlain, 0, $recoveryPlain.Length) }
    Remove-Item -LiteralPath (Join-Path $recoveryTemp 'offline-secret') -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $recoveryTemp -ErrorAction SilentlyContinue
    & ssh -n -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 alexandria 'rm -f -- /home/luisg/alexandria-recovery-drill-upload/offline-secret'
}

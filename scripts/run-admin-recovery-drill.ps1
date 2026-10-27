param(
 [Parameter(Mandatory)][string]$RecoveryFile,
 [Parameter(Mandatory)][string]$SshHost,
 [Parameter(Mandatory)][string]$InstanceRoot,
 [Parameter(Mandatory)][string]$RemoteValidator,
 [Parameter(Mandatory)][string]$RemoteReport
)
$ErrorActionPreference = 'Stop'
if ($SshHost -notmatch '^[A-Za-z0-9][A-Za-z0-9_.@-]*$') { throw 'Invalid SSH alias' }
foreach ($path in @($InstanceRoot,$RemoteValidator,$RemoteReport)) {
 if ($path -notmatch '^/[A-Za-z0-9_./-]+$' -or $path.Split('/') -contains '..') { throw 'Unsafe remote path' }
}
$raw = $null; $upload = $null
try {
 $raw = [Security.Cryptography.ProtectedData]::Unprotect([IO.File]::ReadAllBytes($RecoveryFile),$null,[Security.Cryptography.DataProtectionScope]::CurrentUser)
 $secret = [Text.Encoding]::UTF8.GetString($raw).Trim()
 if ($secret -notmatch '^[a-f0-9]{64}$') { throw 'Invalid recovery secret' }
 $upload = ((& ssh -o BatchMode=yes $SshHost 'umask 077; mktemp -d /tmp/alexandria-recovery-upload-XXXXXXXX') -join '').Trim()
 if ($LASTEXITCODE -ne 0 -or $upload -notmatch '^/tmp/alexandria-recovery-upload-[A-Za-z0-9]+$') { throw 'Private upload directory unavailable' }
 # Send only to this private file; never print the recovery value.
 $secret | & ssh -o BatchMode=yes $SshHost "umask 077; cat > $upload/offline-secret"
 if ($LASTEXITCODE -ne 0) { throw 'Recovery upload failed' }
 & ssh -o BatchMode=yes $SshHost "sudo -n su -c 'python3 $RemoteValidator --instance-root $InstanceRoot --recovery-file $upload/offline-secret --output $RemoteReport'"
 if ($LASTEXITCODE -ne 0) { throw 'Isolated recovery rehearsal failed' }
} finally {
 if ($null -ne $raw) { [Array]::Clear($raw,0,$raw.Length) }
 $secret = $null
 if ($upload -match '^/tmp/alexandria-recovery-upload-[A-Za-z0-9]+$') {
  & ssh -o BatchMode=yes $SshHost "rm -f -- $upload/offline-secret; rmdir -- $upload"
 }
}

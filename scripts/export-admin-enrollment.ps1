param(
    [string]$Origin = 'http://localhost:8781',
    [string]$SshHost = 'alexandria',
    [string]$RemoteDirectory = '/root/alexandria-admin-staging-initial',
    [string]$Destination = "$env:USERPROFILE\Backups\culturaespiritual\admin-staging-secrets"
)
$ErrorActionPreference = 'Stop'
if ($SshHost -notmatch '^[A-Za-z0-9][A-Za-z0-9_.@-]*$') { throw 'Unsafe SSH host' }
if ($Origin -notmatch '^https://[A-Za-z0-9.-]+(?::[0-9]+)?$' -and $Origin -notmatch '^http://localhost:[0-9]+$') { throw 'HTTPS or localhost origin required' }
if ($RemoteDirectory -notmatch '^/[A-Za-z0-9_/-]+$' -or $RemoteDirectory.Split('/') -contains '..') { throw 'Unexpected remote secret directory' }
if (Test-Path -LiteralPath $Destination) { throw 'Refusing to overwrite an existing enrollment export' }
New-Item -ItemType Directory -Path $Destination | Out-Null
$privateAcl = Get-Acl -LiteralPath $Destination
$privateAcl.SetAccessRuleProtection($true, $false)
$currentIdentity = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$privateAcl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($currentIdentity, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
$privateAcl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new([System.Security.Principal.SecurityIdentifier]::new('S-1-5-18'), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
Set-Acl -LiteralPath $Destination -AclObject $privateAcl

# Secrets stay in process memory and private files; never write them to tool/chat output.
$setupUrl = ((& ssh $SshHost "sudo -n su -c 'cat $RemoteDirectory/bootstrap-url.txt'") -join "`n").Trim()
if ($LASTEXITCODE -ne 0 -or $setupUrl -notmatch ('^' + [regex]::Escape($Origin) + '/#setup=[a-f0-9]{64}$')) { throw 'Could not export enrollment link' }
$recovery = ((& ssh $SshHost "sudo -n su -c 'cat $RemoteDirectory/recovery-secret.txt'") -join "`n").Trim()
if ($LASTEXITCODE -ne 0 -or $recovery -notmatch '^[a-f0-9]{64}$') { throw 'Could not export recovery material' }
$plain = [System.Text.Encoding]::UTF8.GetBytes($recovery)
$encrypted = [System.Security.Cryptography.ProtectedData]::Protect($plain, $null, [System.Security.Cryptography.DataProtectionScope]::CurrentUser)
[System.IO.File]::WriteAllBytes((Join-Path $Destination 'recovery-secret.dpapi'), $encrypted)
$roundTrip = [System.Security.Cryptography.ProtectedData]::Unprotect($encrypted, $null, [System.Security.Cryptography.DataProtectionScope]::CurrentUser)
if ([System.Convert]::ToHexString($plain) -ne [System.Convert]::ToHexString($roundTrip)) { throw 'Recovery encryption verification failed' }
[System.IO.File]::WriteAllText((Join-Path $Destination 'Open staging admin.url'), "[InternetShortcut]`r`nURL=$setupUrl`r`n", [System.Text.UTF8Encoding]::new($false))
[System.IO.File]::WriteAllText((Join-Path $Destination 'README.txt'), "Open the Internet Shortcut to register your first staging passkey. Use the configured admin address. HTTPS access does not require a tunnel; the localhost configuration requires its existing tunnel. The link expires 30 minutes after initialization and works once.`r`n`r`nRecovery material is encrypted using Windows DPAPI for this Windows account. It can restore this staging administrator while this account/profile is available. Production recovery needs a separately managed portable offline backup.`r`n", [System.Text.UTF8Encoding]::new($false))
# Only remove the server's plaintext recovery copy after encryption and round-trip verification.
& ssh $SshHost "sudo -n su -c 'rm -- $RemoteDirectory/recovery-secret.txt'"
if ($LASTEXITCODE -ne 0) { throw 'Export succeeded but server-side recovery copy needs removal' }
[System.Array]::Clear($plain)
[System.Array]::Clear($roundTrip)
$recovery = $null
$setupUrl = $null
Write-Output "Private enrollment shortcut and encrypted recovery material saved to $Destination. No secrets printed."

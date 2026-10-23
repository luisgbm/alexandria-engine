# Run with PowerShell 7 as the Windows user who owns the existing DPAPI export.
param(
    [string]$SourceFile = 'C:\Users\luisg\Backups\culturaespiritual\admin-staging-https-secrets\recovery-secret.dpapi',
    [string]$Destination = 'C:\Users\luisg\Backups\culturaespiritual\admin-portable-recovery',
    [string]$KeyDirectory = 'C:\Users\luisg\Backups\culturaespiritual\admin-portable-recovery-key',
    [string]$Restic = 'C:\Users\luisg\Backups\culturaespiritual\alexandria-automated\tools\restic_0.16.4_windows_amd64.exe'
)
$ErrorActionPreference = 'Stop'
$exportRepositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..')).TrimEnd('\') + '\'
$exportDestination = [IO.Path]::GetFullPath($Destination).TrimEnd('\')
$exportKeyDirectory = [IO.Path]::GetFullPath($KeyDirectory).TrimEnd('\')
foreach ($exportPath in @($exportDestination, $exportKeyDirectory)) {
    if (($exportPath + '\').StartsWith($exportRepositoryRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Recovery output must be outside the repository' }
    if (Test-Path -LiteralPath $exportPath) { throw 'Refusing to overwrite an existing recovery export/key directory' }
}
if (($exportKeyDirectory + '\').StartsWith($exportDestination + '\', [StringComparison]::OrdinalIgnoreCase) -or
    ($exportDestination + '\').StartsWith($exportKeyDirectory + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Keep the decryption key outside the encrypted package directory' }

function New-PrivateRecoveryDirectory([string]$Path) {
    New-Item -ItemType Directory -Path $Path | Out-Null
    $acl = Get-Acl -LiteralPath $Path
    $acl.SetAccessRuleProtection($true, $false)
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
        [Security.Principal.WindowsIdentity]::GetCurrent().User, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
        [Security.Principal.SecurityIdentifier]::new('S-1-5-18'), 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow'))
    Set-Acl -LiteralPath $Path -AclObject $acl
}
New-PrivateRecoveryDirectory $exportDestination
New-PrivateRecoveryDirectory $exportKeyDirectory
$exportWork = Join-Path $exportDestination ('work-' + [guid]::NewGuid().ToString('N'))
New-PrivateRecoveryDirectory $exportWork
$exportPasswordFile = Join-Path $exportKeyDirectory 'decryption-key.txt'
$exportRaw = $null
$exportRandom = $null

function Invoke-PrivateRestic([string[]]$Arguments, [string]$InputJson = '') {
    $processInfo = [Diagnostics.ProcessStartInfo]::new()
    $processInfo.FileName = $Restic
    $processInfo.UseShellExecute = $false
    $processInfo.CreateNoWindow = $true
    $processInfo.RedirectStandardInput = $true
    $processInfo.RedirectStandardOutput = $true
    $processInfo.RedirectStandardError = $true
    $processInfo.StandardInputEncoding = [Text.UTF8Encoding]::new($false)
    foreach ($argument in $Arguments) { $processInfo.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::Start($processInfo)
    if ($InputJson) { $process.StandardInput.Write($InputJson) }
    $process.StandardInput.Close()
    $outputText = $process.StandardOutput.ReadToEnd()
    $errorText = $process.StandardError.ReadToEnd()
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw 'Portable recovery encryption/verification failed; no secret output is included' }
    return $outputText
}

try {
    $exportRaw = [Security.Cryptography.ProtectedData]::Unprotect(
        [IO.File]::ReadAllBytes($SourceFile), $null, [Security.Cryptography.DataProtectionScope]::CurrentUser)
    $exportSecret = [Text.Encoding]::UTF8.GetString($exportRaw).Trim()
    if ($exportSecret -notmatch '^[a-f0-9]{64}$') { throw 'Unexpected recovery material' }
    $exportRandom = [byte[]]::new(48)
    $exportRng = [Security.Cryptography.RandomNumberGenerator]::Create()
    $exportRng.GetBytes($exportRandom)
    $exportRng.Dispose()
    [IO.File]::WriteAllText($exportPasswordFile, [Convert]::ToBase64String($exportRandom) + "`n", [Text.UTF8Encoding]::new($false))
    $exportPayload = @{
        schema_version = 1; scope = 'alexandria staging admin only'
        created_at = (Get-Date).ToUniversalTime().ToString('o')
        origin = 'https://admin.52-153-234-132.sslip.io'
        rp_id = 'admin.52-153-234-132.sslip.io'
        state_path = '/var/lib/alexandria-admin-staging/state'
        recovery_secret = $exportSecret
    } | ConvertTo-Json
    $exportRepository = Join-Path $exportWork 'repository'
    $exportCommon = @('--repo', $exportRepository, '--password-file', $exportPasswordFile)
    $null = Invoke-PrivateRestic ($exportCommon + @('init'))
    $null = Invoke-PrivateRestic ($exportCommon + @('backup', '--stdin', '--stdin-filename', 'staging-recovery.json', '--host', 'alexandria-staging-recovery', '--tag', 'offline-recovery')) $exportPayload
    $null = Invoke-PrivateRestic ($exportCommon + @('check', '--read-data'))
    $exportReadme = @'
ENCRYPTED STAGING ADMIN RECOVERY — PORTABLE COPY

This directory contains an encrypted Restic repository, not a Windows DPAPI file.
The decryption key is stored separately. Never store it inside this ZIP or post it
in chat, tickets or source control. Copy this package to an offline removable drive;
keep the key in a password manager or separate offline location.

Requirements: Restic 0.16.4 or a compatible newer version (Windows/Linux/macOS).
After extracting the ZIP, use the separate decryption-key.txt:

  restic -r repository --password-file /private/decryption-key.txt check --read-data
  restic -r repository --password-file /private/decryption-key.txt dump latest staging-recovery.json

The dump contains the secret: do not run it in a recorded terminal or paste its
output. Redirect to a private 0600 file with umask 077 on Linux, then extract the
recovery_secret JSON field to a private file without printing it.

Recovery requires root SSH access to Alexandria and this secret. Stop the admin,
run admin-recover with the correct state directory and a NEW secret output folder,
restore state ownership, start the admin, export rotated recovery material and
enroll replacement passkeys through the new private setup link within 30 minutes.
Recovery revokes ALL registered passkeys and sessions. See docs/admin-recovery-drill.md
in alexandria-engine before executing. This package is for staging only.

This secret remains valid until an actual recovery rotates it. After a real recovery,
create and verify a new encrypted offline package and replace this obsolete copy.
Keep the extracted repository's config, keys, snapshots, index and data files together.
'@
    [IO.File]::WriteAllText((Join-Path $exportWork 'README.txt'), $exportReadme, [Text.UTF8Encoding]::new($false))
    $exportZip = Join-Path $exportDestination 'culturaespiritual-staging-recovery.zip'
    [IO.Compression.ZipFile]::CreateFromDirectory($exportWork, $exportZip)
    $exportValidation = Join-Path $exportWork 'validation'
    [IO.Compression.ZipFile]::ExtractToDirectory($exportZip, $exportValidation)
    $exportRoundTrip = Invoke-PrivateRestic @('--repo', (Join-Path $exportValidation 'repository'), '--password-file', $exportPasswordFile, 'dump', 'latest', 'staging-recovery.json')
    if (($exportRoundTrip | ConvertFrom-Json).recovery_secret -cne $exportSecret) { throw 'Portable ZIP round-trip mismatch' }
    $exportHash = (Get-FileHash -LiteralPath $exportZip -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $exportDestination 'SHA256.txt'), "$exportHash  culturaespiritual-staging-recovery.zip`n", [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText((Join-Path $exportDestination 'verification.json'), (@{
        created_at = (Get-Date).ToUniversalTime().ToString('o'); encrypted = $true
        zip_round_trip_verified = $true; independent_of_dpapi = $true; sha256 = $exportHash
    } | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
    Write-Output 'Encrypted portable recovery ZIP and separate decryption key created. ZIP round-trip verified; no secrets printed.'
}
finally {
    if ($null -ne $exportRaw) { [Array]::Clear($exportRaw, 0, $exportRaw.Length) }
    if ($null -ne $exportRandom) { [Array]::Clear($exportRandom, 0, $exportRandom.Length) }
    $exportPayload = $null; $exportSecret = $null; $exportRoundTrip = $null
    if ([IO.Directory]::Exists($exportWork)) {
        if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($exportWork)) -ne $exportDestination) { throw 'Unsafe temporary directory path' }
        Remove-Item -LiteralPath $exportWork -Recurse -Force
    }
}

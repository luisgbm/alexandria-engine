# Portable staging recovery copy

Created and verified on 2026-10-23, outside the source repository. The original
website, DNS and staging credentials were unchanged. Both live passkeys remain
registered.

## Private files

- Encrypted package:
  `C:\Users\luisg\Backups\culturaespiritual\admin-portable-recovery\culturaespiritual-staging-recovery.zip`.
- SHA-256 and non-secret Windows/Linux verification reports are beside the ZIP.
- Separate decryption key:
  `C:\Users\luisg\Backups\culturaespiritual\admin-portable-recovery-key\decryption-key.txt`.

The ZIP contains a small encrypted Restic repository and instructions. The key
is a newly generated random 48-byte value, encoded as a password file, independent
of Windows DPAPI and the website-backup encryption key. It is **not inside the
package**. Both local folders restrict access to the current Windows user and
SYSTEM. Secrets and package contents must never be committed or pasted into chat.

The encrypted package is approximately 33 KB. It requires Restic 0.16.4 or a
compatible newer version to decrypt; include an appropriate trusted Restic binary
on recovery media if recovery must work without downloading tools. Restic is
already installed on Alexandria and in the PC backup tools directory.

No removable drive was connected during creation. The package is prepared on the
PC, not physically offline yet. Copy the encrypted ZIP and SHA-256 file to an
offline USB/removable drive and disconnect it. Store the decryption key in a
password manager or separate offline location. Do not keep the only copies of
both files together on this PC. After copying, verify the ZIP's SHA-256 against
the recorded checksum. Retain the independent root-SSH recovery procedure too.

## Verification

`scripts/export-portable-admin-recovery.ps1` reads the existing protected DPAPI
export under its owning Windows profile and streams the recovered secret as JSON
into Restic encryption, without a plaintext secret file. It refuses existing output
directories and output inside the repository. The ZIP was extracted again and its
encrypted payload successfully round-tripped with the separate key.

`scripts/validate-portable-admin-recovery.py` then validated that exact ZIP on
Linux. It ran a full Restic integrity check, rejected an incorrect key and decrypted
the payload without DPAPI. The recovered secret's verifier matched the current
staging administrator. The key was absent from the ZIP, the live credential count
remained two, and no recovery/credential mutation was performed. Temporary Linux
key, decrypted state and uploaded test package were removed afterward.

The report is stored privately at
`/var/lib/alexandria-backup/portable-recovery-verification.json`, with a copy beside
the PC package. These checks establish portability of the encrypted format; they
do not establish that physical offline storage has already happened.

## Using the portable copy after losing the Windows profile

1. Obtain the encrypted ZIP, its separate key and trusted Restic software. Verify
   the ZIP checksum and extract it into a private directory.
2. On the Linux root console, keep all outputs private and avoid displaying the
   decrypted JSON in the terminal:

   ```sh
   umask 077
   restic -r /private/extracted/repository --password-file /private/decryption-key.txt check --read-data
   restic -r /private/extracted/repository --password-file /private/decryption-key.txt dump latest staging-recovery.json > /private/recovery.json
   python3 -c 'import json; from pathlib import Path; p=Path("/private/recovery.json"); Path("/private/offline-secret").write_text(json.loads(p.read_text())["recovery_secret"])'
   ```

3. Follow the [lost-key recovery procedure](admin-recovery-drill.md) with
   `/private/offline-secret`. Stop the real admin before recovery; an actual
   recovery revokes every existing key/session and issues a one-time 30-minute
   enrollment link and rotated recovery secret. Restore state ownership and
   restart the admin before enrolling replacement passkeys.
4. Remove temporary decrypted files. Create and verify a **new** portable package
   after rotation, then replace the obsolete offline copy. The old package remains
   readable but its recovery secret will no longer be accepted.

The package supplies the offline recovery secret, not SSH/root access or an
authentication bypass. Both root access and the current secret remain required.

# Passkey recovery

Keep two passkeys on independently available devices. Add the second through
Segurança while authenticated; a device QR flow can enroll a phone without
resetting existing credentials. Removing the last registered key is blocked.

If all authenticators are lost, use root/console access and the separately stored
recovery secret. Stop only the instance's admin service and use:

```sh
sudo /INSTANCE/bin/alexandria-engine admin-recover \
  --state /INSTANCE/data/state --recovery-file /private/current-recovery-secret \
  --new-secrets /private/new-enrollment
```

Restart the service, privately open the new enrollment link, and register a
replacement. Recovery consumes/rotates the secret and revokes old credentials;
it is not the procedure for adding an extra key. Export the new recovery material
and capture a fresh verified off-server backup afterward.

Test recovery only with disposable cloned state under an isolated origin. Confirm
the old secret cannot be replayed, prior credentials/sessions are revoked and the
replacement works. Keep origin/RP ID consistent when restoring an instance.
Never publish recovery links, secrets, SQLite state or simulated credentials.

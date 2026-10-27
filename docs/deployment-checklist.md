# Installation checklist

Use this for any publication; it records checks, not the status of a particular site.

- Choose a supported host, dedicated instance root, free ports and a recovery console.
- Customize the publication title, canonical URL, navigation and content metadata.
- Provision public/admin TLS; verify canonical HTTP/www redirects and renewal reloads.
- Review rendered Nginx/systemd configuration and the backend's loopback boundary.
- Validate all known URLs, media, sitemap, private-path denials and expected missing paths.
- Enroll two passkeys; test sign-in on desktop and phone and export recovery material.
- Exercise editing, upload, preview, publish and rollback with disposable content.
- Set analytics retention/privacy policy, ingestion and log rotation.
- Configure encrypted captures and an independent off-server copy; verify all packs
  and perform an isolated application restore, not just a file-copy check.
- Measure publishing/upload/backup peaks and disk budgets on the intended host.
- Record the executable hash, active release and a tested rollback procedure.

Use [the operator manual](operator-manual.md) for routine operation and
[site migration](site-migration.md) when replacing an existing system.

# Specification and implementation map

The [technical specification](technical-spec.md) describes the engine's current
architecture and constraints. This map identifies the guides for each area;
it is not the status report of a particular installation.

| Area | Implementation and guide |
| --- | --- |
| Static generation, exact URLs, archives and search | `src/build.rs`, [generator guide](importer-generator.md) |
| HTML/JSON content and optional import | Schemas, [WordPress import](wordpress-import.md) |
| Passkeys, sessions and recovery | `src/admin`, [admin setup](admin-guide.md), [recovery](admin-recovery-drill.md) |
| Drafts, pages, categories, revisions and trash | [Editorial workflow](editorial-workflow.md) |
| Image library and responsive controls | [Admin and library](admin-navigation-and-library.md) |
| Serialized publication and rollback | [Publishing](publishing.md) |
| Incremental private analytics | [Analytics](analytics.md) |
| Portable installation and host migration | [Deployment](portable-deployment.md), [restore drill](fresh-vps-restore.md) |
| Shared media and compact release history | [Resource usage](resource-optimization.md) |
| Integrity, retention and offline recovery | [Backups](backup-restore.md) |

Current boundaries: a single administrator identity, Portuguese interface text,
Ubuntu 24.04 x86-64/systemd as the supported complete host, fixed new-article URL
prefix, and no browser-based publication rename workflow. Analytics visitor totals
are estimates. External alerting, DNS, certificates and device approval need
installation-specific configuration or operator action.

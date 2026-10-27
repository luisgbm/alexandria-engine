# Preparing publication content

Start from a portable native content directory or an optional WordPress import.
Keep source backups immutable and prepare changes in a separate private copy.
Record source identity and the intended URL inventory before making corrections.

Review article/page status, original dates, categories, explicit permanent paths,
redirect aliases, media references, alt text and meaningful formatting. Normalize
WordPress More markers to `<!--more-->` so homepage previews use the intended cutoff.
Do not automatically substitute every WordPress thumbnail URL: preserve images
actually referenced by accepted HTML and investigate unused variants separately.

Exclude synthetic acceptance entries and fixtures explicitly. Do not infer that
all drafts are tests or that all unreferenced media is disposable. A retained
release may still reference media absent from the working editor.

Build into a fresh directory and validate output. Compare every known URL with an
expected status/target, inspect representative pages on desktop/mobile, and record
all intentional retirements. A preview server does not reproduce Nginx redirect
and status behavior. Publish only the reviewed content revision; unrelated private
drafts stay private. Review broken references individually and record accepted
corrections or intentional missing paths in the private URL inventory. See the
[URL inventory guide](migration-inventory.md).

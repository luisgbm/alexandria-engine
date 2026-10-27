# Editorial workflow

Create an article or a page in Conteúdo. Kind is fixed after creation; creating a
page does not automatically add it to navigation. Articles use categories;
pages do not. Published permanent addresses remain protected.

## Save, preview and publish

Edit HTML, insert images from the library and select the homepage preview cutoff.
Preview before saving. Saved revisions remain private until explicitly published.
A stale revision from another tab is rejected instead of silently overwriting a
newer save. Publishing one entry does not promote unrelated drafts.

## Revisions

History includes the imported original when available and saved engine revisions.
Restaurar como rascunho creates another editable revision; it does not publish or
overwrite history. Preserve identity and previously published addresses when
restoring. Imported raw WordPress revision evidence is separate from the editor's
native revision browser.

## Categories

Categories support creation, display-name editing, ordering and deletion where
unused. Their stable identifiers determine archive paths. Deletion checks content,
trash, generated lists and retained published dependencies. Reassign eligible
content first; changing an established archive needs an explicit URL/redirect plan.
Catalog edits remain private until a publication incorporates them.

## Trash

Trash hides working entries and prevents ordinary editing/publishing. Restore
returns an entry to the workspace. Empty-trash and permanent-delete controls use
yes/no confirmation and enforce the relevant reference/history protections.
Trashing an entry alone does not remove its already-generated public page.
Explicit publication changes and URL retirement are separate from editorial trash.

Private drafts, revisions and catalog history live under `content/editorial`.
Never serve that directory. See [publishing and rollback](publishing.md).

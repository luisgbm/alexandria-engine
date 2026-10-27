# Admin navigation and image library

The admin interface is organized into separate feature pages. Estatísticas is
the landing page; Conteúdo, Biblioteca de imagens, Barra lateral, Publicações and
Segurança have their own controls. Navigation becomes a hamburger menu on mobile.
The interface is currently Portuguese.

## Content editor

Articles and pages have separate lists and visible trash controls. Search and
state filters apply to both; category filtering applies only to articles. Pages
have no categories and omit publication dates in their public heading. Tags are
not exposed in the editor.

The HTML editor provides paragraph, bold, italic, underline, list and link tools,
plus a button inserting `<!--more-->` at the cursor. This marker determines the
homepage preview cutoff. Live preview is sandboxed. Saving a draft is private;
publishing is a separate action with a centered progress dialog.

New article titles generate `/artigos/<slug>/` addresses. Collisions receive a
random suffix; editors may edit an unpublished address. Published addresses are
protected against silent changes. See [editorial workflow](editorial-workflow.md).

## Image library

Uploads refresh the library immediately and appear newest first. Each image has
an editable alt text/caption, a copyable public URL and HTML for insertion. Images
are inserted where the editor chooses, rather than through a featured-image form.

The insertion controls include width; the default is 480px with a responsive cap.
Generated HTML centers the figure and includes alt text and a caption when set:

```html
<figure style="text-align:center;">
  <img src="/media/example.jpg" alt="A descriptive alternative"
       style="width:480px;max-width:100%;height:auto;">
  <figcaption>An optional caption</figcaption>
</figure>
```

The library has its own trash. Restore reverses a removal; permanent deletion
uses confirmation and reference checks. Blocking references identify their
article/page. A public release can still reference an image removed from the
working library. Retained release resources and backup coverage must be handled
through publication retention, not by manually deleting shared media files.

## Sidebar and public presentation

Barra lateral edits homepage sidebar HTML; the latest ten articles and publication
dates are also listed. On narrow screens the sidebar follows the article previews.
Other pages use the full content width. The header provides in-page search next
to the publication name; a separate search page is not required. RSS is not
emitted. The footer includes a compact GitHub link to Alexandria Engine.

## Security controls

Segurança lists passkeys and supports adding and removing them with confirmation.
Deleting the last passkey is blocked. Lost access uses the root-console recovery
procedure, not an unauthenticated reset endpoint.

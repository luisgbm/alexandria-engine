# Desktop and physical-phone editorial acceptance

Staging-only rehearsal, started 2026-10-23. The original WordPress website and DNS
remain unchanged. The reference active release is
`staging-20261022T225033-d7d27da8ff2a`; return to it after each public test.

## Dedicated content

- Title: `Aceitação desktop e celular — 23/10/2026`.
- Path: `/artigos/aceitacao-desktop-celular-20261023/`.
- The draft contains synthetic text/imagery, formatting, accents, verse breaks,
  category/tag metadata, image alt text and a caption.
- The desktop fixture is a synthetic 640×360 PNG. Its processed upload is
  `/uploads/1a92e094ee5515ae7dd7d958e6340d50b2ae532b444553a3f0219dc702ad1b30.png`.
- This content is retained privately after rollback for the physical-phone test;
  publishing history and revisions are retained. Its permalink remains locked
  because it has been published once.

## Results

| Check | Desktop browser | Physical phone |
|---|---|---|
| Authenticated admin access | Passed | Phone passkey login previously confirmed |
| Article editing and saved persistence | Passed, including reload | Saved phone marker verified on server and after desktop reload |
| Image upload, alt text and caption | Passed | Phone JPEG, alt text and caption verified in saved draft |
| Live preview | Passed at desktop/narrow preview widths | Pending |
| Publish using admin controls | Passed | User confirmed publication; feedback delay reported |
| Generated page, image, feed, sitemap and search | Passed | 12 HTTP checks passed, including phone marker and cat JPEG/alt/caption |
| Rollback using admin controls | Passed | User confirmed; screenshot shows restored baseline and success |
| Exact baseline homepage restored; test paths private again | Passed | 10 HTTP checks passed, including exact baseline homepage hash |

Desktop publication activated `staging-20261023T115545-42974eced858`. The browser
then restored the reference release. The public image loaded with intrinsic
dimensions 640×360. Formatting, accents, verse line breaks, caption and quotation
rendered correctly in the generated page. Saving/reloading preserved the draft,
uploaded image and history. The narrow preview is a desktop preview control,
not evidence of a physical-phone result.

`scripts/validate_editorial_acceptance.py --published --output PRIVATE_REPORT`
passed 11 public HTTP checks after desktop publication. Without `--published`,
it passed 10 checks after rollback: the article/image return 404, feed/sitemap/
search exclude the article, protected paths remain inaccessible, and the homepage
SHA-256 exactly matches the reference release. Wait for the admin's completion
message before running a public check; an in-progress rollback still serves the
previous active release by design.

Private JSON reports and screenshots are in
`C:\Users\luisg\Backups\culturaespiritual\admin-validation`:
`acceptance-desktop-published.json`, `acceptance-desktop-rollback.json`,
`acceptance-desktop-published.png`, and `acceptance-narrow-preview.png`.

## Physical-phone handoff

1. Open the HTTPS admin on the phone and find the dedicated article.
2. Append `<p>Validado no celular.</p>` to its HTML. Upload a non-sensitive
   JPG/PNG/WebP under 8 MB and add alt text/caption. Check the live preview and
   save the draft. Confirm the edit persists after reopening it.
3. Report the saved step so the operator can verify persistence before publishing.
4. Publish from the phone, wait for success, and open the test path on the staging
   IP. Confirm formatting/images and the phone marker. The operator then runs
   the public validator with `--published --phone-marker`.
5. In the phone admin's staging publications, restore the exact reference release
   above. Wait for completion. Verify the test path returns 404 and the private
   draft remains editable; the operator verifies rollback with the same script.
6. Record physical-device/browser observations and any failures explicitly. Do
   not infer phone acceptance from desktop emulation or server-side checks.

## Phone save feedback finding

The user reported that tapping **Salvar rascunho** appeared to do nothing. The
saved revision already contained the phone marker, the new JPEG, alt text and
caption; repeated taps had created valid revisions. The original success/error
notice was above the analytics dashboard and outside the mobile editor viewport.

The first fix repeated feedback beside the editor buttons. The user then published
from the phone, activating `staging-20261023T120915-494b837b0fae`, but reported a
several-second difference between messages. The follow-up fix replaces duplicated
messages with one persistent bottom status banner, including atomic live
announcements. Save, publish and rollback show success immediately after the
mutation API confirms completion, before refreshing the deployment/list/document
panels. A panel refresh failure retains the completed-operation message and asks
for a reload rather than implying the mutation failed.

The deployed admin was reloaded in the authenticated browser. Saving the unchanged
phone draft showed **Salvando rascunho…**, followed by the visible fixed success
banner; screenshot `acceptance-status-banner.png` is stored in the private
validation folder. The phone publication remains active; physical-phone feedback
confirmation and rollback to the reference release are still pending. No original
website or DNS changes were made.

## Physical-phone rollback and banner width

The user confirmed successful feedback and rollback on the phone, with a screenshot
showing the reference release active. `acceptance-phone-rollback.json` records all
10 rollback checks passing. The user also reported that the banner extended past
the page width. The CSS now sets an explicit viewport-bounded banner width and
allows narrow editor grids and long release/revision identifiers to shrink/wrap.
After deployment, an authenticated browser save at phone width confirmed a visible
success banner: viewport 394 px, document scroll width 378 px, banner bounds
8.4–370 px (361.6 px wide). Screenshot `acceptance-phone-banner-width.png` is in
the private validation folder. The viewport override was reset after verification.
The saved draft remains available; the baseline staging release remains active.

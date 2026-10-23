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
| Publish using admin controls | Passed | Pending |
| Generated page, image, feed, sitemap and search | Passed | Pending |
| Rollback using admin controls | Passed | Pending |
| Exact baseline homepage restored; test paths private again | Passed | Pending |

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

The admin now repeats action feedback directly below the editor buttons, with
an accessible live status and an error style. The save button shows **Salvando…**
during the request. Only the editor status announces messages while the editor
is open, avoiding duplicate live announcements. Selecting a different document
clears its previous local notice. A browser save verified the new visible success
message and preserved all phone changes. The user must refresh the phone admin
to load this update; the reference release is still active. Phone confirmation
of the updated feedback, publishing and rollback remains pending.

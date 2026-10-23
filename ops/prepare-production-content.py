#!/usr/bin/env python3
"""Create a separate, inactive content candidate excluding explicit staging fixtures."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

FIXTURES = {
    'art-219278af799b4f45a90780f0e59e393c': '/artigos/validacao-staging-20261022/',
    'page-4cb61d3fe3be42b6b506a567362bf6cd': '/verificacao-editorial/',
    'art-5fc8b71f4627469ba0d6f745a1644903': '/artigos/novo-5fc8b71f4627/',
    'art-1d22ab8ac293471498b3968fdf8fa71d': '/artigos/aceitacao-desktop-celular-20261023/',
}
# Owner confirmed these imported drafts were deleted in WordPress.
RETIRED_DRAFTS = {'wp-2721', 'wp-2731', 'wp-2747', 'wp-2414', 'wp-3041'}
IMAGES = [
    '1a92e094ee5515ae7dd7d958e6340d50b2ae532b444553a3f0219dc702ad1b30.png',
    '80edd4c00a9f95b6ea9bfee448f4ee7796b5d49d5a93d994ed8f8ea2904405dc.png',
    '4375270f8633b79f46dca7ca0d005c57856ae14515142f68d8970cf6e7ce743b.jpg',
]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--snapshot', type=Path, required=True)
    parser.add_argument('--imported-content', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    source = args.snapshot.resolve(strict=True)
    imported = args.imported_content.resolve(strict=True)
    output = args.output.resolve()
    if output.exists() or output.is_relative_to(source) or source.is_relative_to(output):
        parser.error('output must be a new independent directory')
    if output.is_relative_to(imported) or imported.is_relative_to(output):
        parser.error('output must be separate from imported content')
    # The source is an immutable published snapshot, never the mutable editorial store.
    if (source / 'editorial').exists() or (source / 'state').exists():
        parser.error('source must be a published content snapshot without editorial/auth state')
    for path in source.rglob('*'):
        if path.is_symlink():
            parser.error('snapshot must not contain symlinks')
    excluded = {f'{folder}/{identity}' for folder in ['articles', 'pages'] for identity in FIXTURES}
    excluded.update('media/uploads/' + name for name in IMAGES)
    for identity in RETIRED_DRAFTS:
        for folder in ['articles', 'pages']:
            metadata = source / folder / identity / 'metadata.json'
            if metadata.exists():
                assert json.loads(metadata.read_text(encoding='utf-8'))['status'] == 'draft', identity
            excluded.add(f'{folder}/{identity}')
        excluded.add(f'revisions/{identity}')
    retained = {}
    removed = []
    output.mkdir(mode=0o700, parents=True)
    content = output / 'content'
    content.mkdir(mode=0o700)
    for path in sorted(source.rglob('*')):
        relative = path.relative_to(source).as_posix()
        if any(relative == item or relative.startswith(item + '/') for item in excluded):
            if path.is_file():
                removed.append(relative)
            continue
        destination = content / relative
        if path.is_dir():
            destination.mkdir(exist_ok=True)
        elif path.is_file():
            shutil.copy2(path, destination)
            retained[relative] = digest(path)
            assert digest(destination) == retained[relative], relative
    # Preserve imported revision history; native test revision histories stay on staging.
    if (imported / 'revisions').exists() and not (content / 'revisions').exists():
        shutil.copytree(imported / 'revisions', content / 'revisions',
                        ignore=lambda directory, names: RETIRED_DRAFTS.intersection(names)
                        if Path(directory) == imported / 'revisions' else set())
    counts = {}
    identities = set()
    for folder in ['articles', 'pages']:
        for path in (content / folder).glob('*/metadata.json'):
            metadata = json.loads(path.read_text(encoding='utf-8'))
            assert metadata['id'] not in FIXTURES and metadata['id'] not in RETIRED_DRAFTS
            identities.add(metadata['id'])
            key = folder + ':' + metadata['status']
            counts[key] = counts.get(key, 0) + 1
    original_ids = {json.loads(p.read_text(encoding='utf-8'))['id']
                    for folder in ['articles', 'pages']
                    for p in (imported / folder).glob('*/metadata.json')}
    assert original_ids - RETIRED_DRAFTS <= identities, 'retained imported content would be lost'
    assert not RETIRED_DRAFTS.intersection(identities)
    # Fail instead of creating dangling image/URL references in retained live content.
    for folder in ['articles', 'pages']:
        for path in (content / folder).rglob('*'):
            if path.suffix in ['.html', '.json']:
                text = path.read_text(encoding='utf-8')
                assert not any(value in text for value in [*FIXTURES.values(), *IMAGES]), path
    manifest = {p.relative_to(content).as_posix(): digest(p)
                for p in sorted(content.rglob('*')) if p.is_file()}
    report = {'activated': False, 'source_snapshot': str(source),
              'content': str(content), 'excluded_document_ids': list(FIXTURES),
              'excluded_image_names': IMAGES, 'removed_snapshot_files': removed,
              'retained_snapshot_files_verified': len(retained),
              'retired_draft_ids': sorted(RETIRED_DRAFTS),
              'imported_documents_preserved': len(original_ids - RETIRED_DRAFTS), 'counts': counts,
              'manifest': manifest}
    (output / 'preparation-report.json').write_text(
        json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({k: v for k, v in report.items() if k != 'manifest'}, indent=2))


if __name__ == '__main__':
    main()

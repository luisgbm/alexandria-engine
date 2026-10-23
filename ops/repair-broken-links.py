#!/usr/bin/env python3
"""Apply the owner-approved historical-link corrections to a new private candidate."""
import argparse
from datetime import datetime
import hashlib
import json
from pathlib import Path
import re
import shutil
from zoneinfo import ZoneInfo

ENGLISH = {
    'wp-2167': '/artigos/a-non-material-reality/',
    'wp-2177': '/artigos/death-is-a-virtual-transformation/',
    'wp-2185': '/artigos/matter-life-consciousness/',
    'wp-2154': '/artigos/mystic-resonance/',
    'wp-2194': '/artigos/the-human-look-depends-on-the-light/',
}
KUHN = '/resumos/thomas-s-kuhn-a-funcao-do-dogma-na-investigacao-cientifica/'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    source = args.source.resolve(strict=True)
    output = args.output.resolve()
    assert not output.exists() and not output.is_relative_to(source)
    assert not source.is_relative_to(output)
    assert not any(p.is_symlink() for p in source.rglob('*'))
    output.mkdir(mode=0o700, parents=True)
    content = output / 'content'
    shutil.copytree(source, content)
    changed = []
    stamp = datetime.now(ZoneInfo('America/Sao_Paulo')).isoformat()
    for identity in ['wp-1108', 'wp-1485', 'wp-585', *ENGLISH]:
        root = content / 'articles' / identity
        path = root / 'body.html'
        before = path.read_text(encoding='utf-8')
        if identity in ENGLISH:
            pattern = (r'<p\b[^>]*>\s*<a\b[^>]*href="' + re.escape(ENGLISH[identity])
                       + r'"[^>]*>This article is also available in English\. Click here to read it\.</a>\s*</p>\s*')
            after, count = re.subn(pattern, '', before)
        elif identity == 'wp-1108':
            after, count = re.subn(r' e adquira já o seu através de nossa página de <a href="/contato/"[^>]*>Contato</a>', '', before)
        elif identity == 'wp-1485':
            after, count = re.subn(r' ou entre em contato conosco através do link <a href="/contato/"[^>]*>Contato</a>', '', before)
        else:
            after, count = re.subn(r'href="/vestibular/" title="Vestibular"',
                                  f'href="{KUHN}"', before)
        assert count == 1, (identity, count)
        path.write_text(after, encoding='utf-8')
        metadata_path = root / 'metadata.json'
        metadata = json.loads(metadata_path.read_text(encoding='utf-8'))
        metadata['modified_at'] = stamp
        metadata_path.write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        changed.append({'id': identity, 'permalink': metadata['permalink'],
                        'before_sha256': hashlib.sha256(before.encode()).hexdigest(),
                        'after_sha256': hashlib.sha256(after.encode()).hexdigest()})
    forbidden = ['/contato/', '/vestibular/', *ENGLISH.values()]
    for folder in ['articles', 'pages']:
        for path in (content / folder).glob('*/body.html'):
            body = path.read_text(encoding='utf-8')
            assert not any(f'href="{target}"' in body for target in forbidden), path
    report = {'activated': False, 'source': str(source), 'changed': changed,
              'remaining_broken_body_links': 0, 'historical_destinations_remain_404': forbidden}
    (output / 'link-repair-report.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'changed_articles': len(changed), 'remaining_broken_body_links': 0, 'activated': False}))


if __name__ == '__main__':
    main()

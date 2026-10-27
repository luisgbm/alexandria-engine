#!/usr/bin/env python3
"""Verify a portable encrypted capture and its credential database in memory."""
import argparse
import hashlib
import json
from pathlib import Path
import sqlite3
import subprocess


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ['restic','repo','password-file','output']:p.add_argument('--'+name,type=Path,required=True)
    for name in ['snapshot-host','capture-root','expected-origin']:p.add_argument('--'+name,required=True)
    p.add_argument('--expected-release')
    p.add_argument('--minimum-passkeys',type=int,default=2)
    args=p.parse_args()
    base=[str(args.restic),'--repo',str(args.repo),'--password-file',str(args.password_file)]
    subprocess.run([*base,'check','--read-data'],check=True)
    snapshots=json.loads(subprocess.check_output([*base,'snapshots','--json']))
    latest=max((s for s in snapshots if s['hostname']==args.snapshot_host),key=lambda s:s['time'])
    def dump(path):return subprocess.check_output([*base,'dump',latest['id'],args.capture_root.rstrip('/')+'/'+path])
    manifest=json.loads(dump('manifest.json'));entries={r['path']:r for r in manifest['files']}
    if len(entries)!=len(manifest['files']):raise ValueError('Duplicate manifest paths')
    name='instance/data/state/admin.sqlite';database=bytearray(dump(name))
    assert hashlib.sha256(database).hexdigest()==entries[name]['sha256']
    assert len(database)>=100 and entries.get(name+'-wal',{'size':0})['size']==0,'Uncheckpointed SQLite capture'
    database[18:20]=b'\x01\x01'
    with sqlite3.connect(':memory:') as db:
        db.deserialize(bytes(database))
        assert db.execute('PRAGMA integrity_check').fetchone()[0]=='ok'
        count=db.execute('SELECT count(*) FROM credentials').fetchone()[0]
        assert count>=args.minimum_passkeys
        assert db.execute("SELECT value FROM settings WHERE key='origin'").fetchone()[0]==args.expected_origin
    if args.expected_release:
        link=entries['instance/publish/current']['link']
        assert Path(link).parts[-3:]==('releases',args.expected_release,'public')
    report={'passed':True,'snapshot_id':latest['id'],'passkeys':count,'full_repository_read':True,
            'sqlite_integrity':True,'origin_matches':True,'plaintext_database_written_to_disk':False}
    args.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8');print(json.dumps(report))


if __name__=='__main__':main()

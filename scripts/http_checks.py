"""Read-only HTTP validation for explicit origins; redirects are never followed."""
import argparse
import hashlib
import http.client
import json
from pathlib import Path
from urllib.parse import urlsplit


def origin(value):
    parsed = urlsplit(value)
    if (parsed.scheme not in ('http','https') or not parsed.hostname or parsed.username
            or parsed.password or parsed.path not in ('','/') or parsed.query or parsed.fragment):
        raise ValueError('Expected an HTTP(S) origin without credentials, path or query')
    return parsed


def check(base, row):
    target = origin(base)
    path = row['path']
    if not path.startswith('/') or path.startswith('//') or any(c in path for c in ('\\','\r','\n','#')):
        raise ValueError('Expected a local request path')
    connection = (http.client.HTTPSConnection if target.scheme=='https' else http.client.HTTPConnection)(target.hostname,target.port,timeout=20)
    try:
        connection.request('GET',path,headers={'X-Alexandria-Analytics':'ignore'})
        response=connection.getresponse();body=response.read();errors=[]
        if response.status!=row.get('status',200):errors.append('Unexpected HTTP status')
        if row.get('sha256') and hashlib.sha256(body).hexdigest()!=row['sha256']:errors.append('Body hash mismatch')
        for text in row.get('contains',[]):
            if text.encode() not in body:errors.append('Expected body marker absent')
        for name,value in row.get('headers',{}).items():
            if value not in (response.getheader(name) or ''):errors.append('Header mismatch: '+name)
        return {'path':path,'status':response.status,'errors':errors}
    finally:connection.close()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base-url',required=True)
    parser.add_argument('--checks',type=Path,required=True,help='JSON array of path/status/body/header expectations')
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();origin(args.base_url)
    rows=json.loads(args.checks.read_text(encoding='utf-8'))
    if not isinstance(rows,list) or not rows:raise ValueError('Supply at least one check')
    results=[check(args.base_url,row) for row in rows]
    report={'passed':all(not r['errors'] for r in results),'base_url':args.base_url,'checks':results}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(report,indent=2))
    raise SystemExit(0 if report['passed'] else 1)


if __name__=='__main__':main()

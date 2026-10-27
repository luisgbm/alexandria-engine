import importlib.util
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
import threading
import unittest

ROOT=Path(__file__).resolve().parents[1]
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,ROOT/path)
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
http=load('http_checks','scripts/http_checks.py')
retention=load('retention_tools','ops/prune-release-history.py')


class ReusableToolsTests(unittest.TestCase):
    def test_explicit_http_target_and_no_redirect_following(self):
        seen=[]
        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                seen.append(self.path)
                if self.path=='/redirect':
                    self.send_response(302);self.send_header('Location','/must-not-follow');self.end_headers()
                else:
                    self.send_response(200);self.send_header('X-Example','present');self.end_headers();self.wfile.write(b'Synthetic publication')
            def log_message(self,*args):pass
        server=HTTPServer(('127.0.0.1',0),Handler)
        worker=threading.Thread(target=server.serve_forever);worker.start()
        try:
            base='http://127.0.0.1:'+str(server.server_port)
            self.assertEqual(http.check(base,{'path':'/','contains':['Synthetic'],'headers':{'X-Example':'present'}})['errors'],[])
            self.assertEqual(http.check(base,{'path':'/redirect','status':302})['errors'],[])
            self.assertEqual(seen,['/','/redirect'])
            for value in ['https://user:secret@example.org','https://example.org/a','https://example.org/?x=1']:
                with self.assertRaises(ValueError):http.origin(value)
            for path in ['//example.org/','/bad\nheader','/bad\\path']:
                with self.assertRaises(ValueError):http.check(base,{'path':path})
        finally:server.shutdown();worker.join();server.server_close()

    def test_portable_manifest_rebases_only_publisher_rows(self):
        entries={'instance/publish/.history.sqlite':{'sha256':'a'},'instance/publish/history/r.json':{'sha256':'b'},'instance/data/state/admin.sqlite':{'sha256':'c'}}
        mapped=retention.publisher_entries(entries,Path('/srv/example/publish'),'instance/publish/')
        self.assertEqual(set(mapped),{'srv/example/publish/.history.sqlite','srv/example/publish/history/r.json'})
        self.assertEqual(entries['instance/publish/.history.sqlite']['sha256'],'a')
        for prefix in ['../','/instance/publish/','instance//publish/']:
            with self.assertRaises(ValueError):retention.publisher_entries(entries,Path('/srv/example/publish'),prefix)
        with self.assertRaises(ValueError):retention.publisher_entries({'instance/publish/../state':{}},Path('/srv/example/publish'),'instance/publish/')

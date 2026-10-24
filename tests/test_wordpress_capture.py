"""Only loopback HTTP is used: capture fidelity, redirect scope, and failure evidence."""
import csv
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('wordpress_capture', ROOT / 'scripts/capture-wordpress.py')
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


class CaptureTests(unittest.TestCase):
    def test_capture_produces_checked_csv_html_and_attachment_evidence_without_cross_site_fetches(self):
        visited = []

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                visited.append(self.path)
                base = f'http://127.0.0.1:{self.server.server_port}'
                if self.path == '/?p=1':
                    self.send_response(301)
                    self.send_header('Location', '/artigos/exemplo/')
                    self.end_headers()
                    return
                if self.path == '/?attachment_id=6':
                    self.send_response(302)
                    self.send_header('Location', 'https://external.invalid/forbidden')
                    self.end_headers()
                    return
                if self.path == '/missing/':
                    self.send_response(404)
                    self.end_headers()
                    return
                canonical, identity = (base + '/midia/', '5') if self.path == '/?attachment_id=5' else (base + self.path, '1')
                body = f'<html><head><link rel="canonical" href="{canonical}"></head><body><article id="post-{identity}"><div class="entry-content"><p>Olá</p></div></article></body></html>'.encode()
                self.send_response(200)
                self.send_header('Content-Type', 'text/html; charset=utf-8')
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        try:
            base = f'http://127.0.0.1:{server.server_port}'
            with tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                inventory = root / 'inventory'
                inventory.mkdir()
                rows = [dict(url=base + path, kinds=kind, content_ids=identity, checked='no') for path, kind, identity in [('/', 'home', ''), ('/artigos/exemplo/', 'post', '1'), ('/?p=1', 'alias', '1'), ('/?attachment_id=5', 'attachment', '5'), ('/?attachment_id=6', 'attachment', '6'), ('/wp-admin/', 'link', ''), ('/danger/?action=delete', 'link', ''), ('/missing/', 'post', '2')]]
                capture.write_csv(inventory / 'urls.csv', capture.URL_FIELDS, rows)
                capture.write_csv(inventory / 'attachment-urls.csv', capture.ATTACHMENT_FIELDS, [dict(id=str(i), query_url=base + f'/?attachment_id={i}') for i in (5, 6)])
                output = root / 'capture'
                report = capture.capture(inventory, output, delay=0, allow_http=True)
                self.assertEqual(report['captured_entries'], 1)
                self.assertEqual(report['failed_published_entries'], 1)
                body = (output / 'live-html' / (hashlib.sha256((base + '/artigos/exemplo/').encode()).hexdigest() + '.html')).read_text(encoding='utf-8')
                self.assertIn('Olá', body)
                rows = capture.read_csv(output / 'urls.csv')
                aliases = next(r for r in rows if r['kinds'] == 'alias')
                self.assertEqual(aliases['status'], '200')
                self.assertEqual(aliases['final_url'], base + '/artigos/exemplo/')
                missing = next(r for r in rows if r['url'].endswith('/missing/'))
                self.assertEqual((missing['status'], missing['checked']), ('404', 'yes'))
                attachments = capture.read_csv(output / 'attachment-urls.csv')
                self.assertEqual(attachments[0]['verified'], 'yes')
                self.assertEqual(attachments[0]['public_url'], base + '/midia/')
                self.assertEqual(attachments[1]['verified'], 'no')
                self.assertNotIn('/wp-admin/', visited)
                self.assertNotIn('/danger/?action=delete', visited)
                self.assertEqual(json.loads((output / 'capture-report.json').read_text())['scope'], base + '/')
                with self.assertRaisesRegex(ValueError, 'fresh output'):
                    capture.capture(inventory, output, delay=0, allow_http=True)
        finally:
            server.shutdown()
            server.server_close()
            worker.join()

    def test_url_scope_rejects_credentials_cross_host_ports_actions_and_unapproved_http(self):
        base = 'https://www.example.org/'
        for url in ['http://example.org/', 'https://user:password@example.org/', 'https://example.org:9999/', 'https://external.invalid/', 'https://example.org/wp-login.php', 'https://example.org/?action=delete']:
            with self.assertRaises(ValueError):
                capture.validate_url(url, base)
        capture.validate_url('https://example.org/?p=10', base)
        capture.validate_url('http://127.0.0.1:8888/', 'http://127.0.0.1:8888/', allow_http=True)


if __name__ == '__main__':
    unittest.main()

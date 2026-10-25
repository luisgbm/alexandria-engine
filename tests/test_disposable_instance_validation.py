"""Keep real HTTPS editor checks explicitly scoped to a disposable origin."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    'validator', Path(__file__).resolve().parents[1] / 'scripts/validate-portable-instance.py')
validator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validator)


class OriginTests(unittest.TestCase):
    def test_loopback_remains_supported(self):
        validator.validate_origins({'public_url': 'http://localhost:18880',
                                    'admin_origin': 'http://localhost:18881'})

    def test_https_requires_exact_explicit_origin_and_both_https(self):
        config = {'public_url': 'https://stage.example.org',
                  'admin_origin': 'https://admin.example.org'}
        for authorization in [None, 'https://other.example.org']:
            with self.assertRaises(AssertionError):
                validator.validate_origins(config, authorization)
        validator.validate_origins(config, config['admin_origin'])
        with self.assertRaises(AssertionError):
            validator.validate_origins(dict(config, public_url='http://stage.example.org'),
                                       config['admin_origin'])

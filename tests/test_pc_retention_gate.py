import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    'gate', Path(__file__).resolve().parents[1] / 'scripts/verify-and-prune-history.py')
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class GateTests(unittest.TestCase):
    def test_failed_full_data_check_cannot_request_remote_deletion(self):
        with tempfile.TemporaryDirectory() as folder:
            arguments = ['gate', '--backup-root', folder, '--password-file', folder + '/password', '--apply']
            with patch.object(sys, 'argv', arguments), patch.object(gate.subprocess, 'run',
                    side_effect=subprocess.CalledProcessError(1, 'restic')) as run:
                with self.assertRaises(subprocess.CalledProcessError):
                    gate.main()
                self.assertEqual(run.call_count, 1)
                self.assertIn('--read-data', run.call_args.args[0])
                self.assertFalse((Path(folder) / 'pc-verification-receipt.json').exists())

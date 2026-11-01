"""Stdlib tests for delivery safety; they do not compile the CAD application."""
from pathlib import Path
import hashlib
import importlib.util
import sys
import tempfile
import unittest
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
import verify_delivery
import restore_arclet

class DeliveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
    def tearDown(self):
        self.temp.cleanup()
    def manifest(self, content):
        (self.root/'SHA256SUMS').write_text(content)
    def test_valid(self):
        (self.root/'a.txt').write_bytes(b'hello')
        self.manifest(hashlib.sha256(b'hello').hexdigest()+'  a.txt\n')
        self.assertEqual(verify_delivery.verify(self.root),1)
    def test_tampering(self):
        (self.root/'a').write_text('changed')
        self.manifest('0'*64+'  a\n')
        with self.assertRaises(ValueError):verify_delivery.verify(self.root)
    def test_missing(self):
        self.manifest('0'*64+'  missing\n')
        with self.assertRaises(ValueError):verify_delivery.verify(self.root)
    def test_traversal(self):
        for path in ['../escape','/absolute','C:/windows','folder\\file']:
            with self.subTest(path=path):
                self.manifest('0'*64+'  '+path+'\n')
                with self.assertRaises(ValueError):verify_delivery.verify(self.root)
    def test_duplicate(self):
        (self.root/'a').write_bytes(b'')
        line=hashlib.sha256(b'').hexdigest()+'  a\n';self.manifest(line*2)
        with self.assertRaises(ValueError):verify_delivery.verify(self.root)
    def test_empty_manifest(self):
        self.manifest('')
        with self.assertRaises(ValueError):verify_delivery.verify(self.root)
    def test_existing_destination_refused_before_git_or_network(self):
        existing=self.root/'existing';existing.mkdir();(existing/'keep').write_text('intact')
        with patch.object(restore_arclet.subprocess,'run') as run:
            with self.assertRaises(ValueError):restore_arclet.restore(existing)
            run.assert_not_called()
        self.assertEqual((existing/'keep').read_text(),'intact')
    def test_invalid_base_preserves_existing_source(self):
        with patch.object(restore_arclet,'verify',return_value=1):
            with patch.object(restore_arclet.subprocess,'run') as run:
                with self.assertRaises(ValueError):restore_arclet.restore(self.root/'new', self.root/'missing')
                run.assert_not_called()

if __name__=='__main__':unittest.main(verbosity=2)

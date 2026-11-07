#!/usr/bin/env python3
"""Exercise backup/restore scripts with a Docker double, never a real database."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class BackupScripts(unittest.TestCase):
    def test_default_template_and_shell_metacharacters_are_data(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            shutil.copytree(ROOT / "scripts", root / "scripts")
            (root / ".env").write_text((ROOT / ".env.example").read_text() +
                '\nFEDI3_TEST_COMMAND=$(touch SHOULD_NOT_EXIST); touch ALSO_NOT_CREATED\n')
            tools = root / "tools"
            tools.mkdir()
            docker = tools / "docker"
            docker.write_text('''#!/usr/bin/env python3
import sys, os, io, tarfile
from pathlib import Path
args=sys.argv[1:]
if 'ps' in args: print('test-container')
elif 'pg_dump' in args: print('-- test SQL dump')
elif 'psql' in args: Path('restored.sql').write_bytes(sys.stdin.buffer.read())
elif 'tar' in args and '-czf' in args:
 with tarfile.open(fileobj=sys.stdout.buffer,mode='w|gz') as archive:
  entry=tarfile.TarInfo('media/test.txt');data=b'media content';entry.size=len(data);archive.addfile(entry,io.BytesIO(data))
elif 'tar' in args and '-xzf' in args:
 with tarfile.open(fileobj=sys.stdin.buffer,mode='r|gz') as archive:
  for entry in archive:
   if entry.isfile():Path('restored-media.txt').write_bytes(archive.extractfile(entry).read())
''')
            docker.chmod(0o755)
            env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ["PATH"])
            def run(script, *args):
                subprocess.run(["bash", str(root / "scripts" / script), *map(str, args)], cwd=root, env=env,
                               check=True, capture_output=True, text=True)
            run("backup_relay_pg.sh")
            run("backup_relay_media.sh")
            run("restore_relay_pg.sh", next((root / "backups").glob("relay_pg_*.sql.gz")), "--yes")
            run("restore_relay_media.sh", next((root / "backups").glob("relay_media_*.tar.gz")), "--yes")
            self.assertEqual((root / "restored.sql").read_text(), "-- test SQL dump\n")
            self.assertEqual((root / "restored-media.txt").read_bytes(), b"media content")
            self.assertFalse((root / "SHOULD_NOT_EXIST").exists())
            self.assertFalse((root / "ALSO_NOT_CREATED").exists())
            value = subprocess.run(["bash", "-c", 'source scripts/lib/dotenv.sh; load_dotenv .env; printf "%s" "$FEDI3_RELAY_CSP"'], cwd=root, env=env,
                                   check=True, capture_output=True, text=True).stdout
            self.assertEqual(value, "default-src 'self'; img-src 'self' data: https:; style-src 'self' 'unsafe-inline'")
            with (root / ".env").open("a") as config:
                config.write('\nBACKUP_KEEP=items[$(touch BAD_RETENTION)]\n')
            result = subprocess.run(["bash", str(root / "scripts/backup_relay_pg.sh")], cwd=root, env=env, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / "BAD_RETENTION").exists())


if __name__ == "__main__":
    unittest.main()

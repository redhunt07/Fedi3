#!/usr/bin/env python3
"""Launch the packaged client on a fresh CI desktop and require a Flutter frame.

Run under dbus-run-session and xvfb-run on a disposable CI runner, which has no
Fedi3 profiles. The Linux runner maps its window only after Flutter's first frame.
This exercises the GUI's dynamic dependencies, not just dlopen of the Rust core.
"""
import argparse
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('appimage', type=Path)
    args = parser.parse_args()
    image = args.appimage.resolve(strict=True)
    image.chmod(image.stat().st_mode | 0o100)
    env = dict(os.environ, APPIMAGE_EXTRACT_AND_RUN='1', GDK_BACKEND='x11',
               GTK_MODULES='', NO_AT_BRIDGE='1')
    with tempfile.TemporaryFile(mode='w+') as log:
        process = subprocess.Popen([str(image)], env=env, stdout=log,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            deadline = time.monotonic() + 40
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f'AppImage exited during startup: {process.returncode}')
                tree = subprocess.run(['xwininfo', '-root', '-tree'], capture_output=True,
                                      text=True, check=True).stdout
                windows = re.findall(r'^\s*(0x[0-9a-f]+).*fedi3', tree, re.I | re.M)
                for window in windows:
                    state = subprocess.run(['xwininfo', '-id', window], capture_output=True,
                                           text=True, check=False).stdout
                    if 'Map State: IsViewable' in state:
                        time.sleep(3)
                        if process.poll() is not None:
                            raise RuntimeError('AppImage exited after its first frame')
                        print('Packaged client displayed its first Flutter frame and stayed alive')
                        return
                time.sleep(0.5)
            raise RuntimeError('Packaged client did not display its first Flutter frame')
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
            log.seek(0)
            print(log.read())


if __name__ == '__main__':
    main()

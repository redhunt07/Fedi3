#!/usr/bin/env python3
"""Keep maintained guides bilingual and local documentation links resolvable."""
from pathlib import Path
import re
import unittest
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[2]


class DocumentationContract(unittest.TestCase):
    def test_maintained_guides_have_both_languages(self):
        english = {p.name for p in (ROOT / 'docs').glob('*.md')}
        italian = {p.name for p in (ROOT / 'docs/it').glob('*.md')}
        self.assertEqual(english, italian)

    def test_local_link_targets_exist(self):
        pages = list(ROOT.glob('*.md')) + list((ROOT / 'docs').rglob('*.md'))
        pages.append(ROOT / 'app/README.md')
        for page in pages:
            # Fenced examples may contain illustrative Markdown links.
            prose = re.sub(r'```.*?```', '', page.read_text(), flags=re.S)
            for target in re.findall(r'\[[^\]]*\]\(([^)\s]+)(?:\s+[^)]*)?\)', prose):
                url = urlsplit(target.strip('<>'))
                if url.scheme or url.netloc or not url.path:
                    continue
                with self.subTest(page=str(page.relative_to(ROOT)), target=target):
                    self.assertTrue((page.parent / unquote(url.path)).exists())


if __name__ == '__main__':
    unittest.main()

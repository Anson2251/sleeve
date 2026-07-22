#!/usr/bin/env python3
"""Check that every Sleeve translation key used in Rust is in each language file."""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "src"
LANGUAGES = [ROOT / "assets" / "lang" / name for name in ("en.json", "zh-CN.json")]
KEY_PATTERN = re.compile(r'(?<![A-Za-z_])(?:crate::)?t[f]?!\(\s*"([^\"]+)"')
TR_PATTERN = re.compile(r'i18n::tr\("([^\"]+)"')


def source_keys():
    return {
        key
        for path in SOURCE.rglob("*.rs")
        for content in [path.read_text()]
        for key in KEY_PATTERN.findall(content) + TR_PATTERN.findall(content)
    }


def main():
    keys = source_keys()
    success = True
    for path in LANGUAGES:
        translations = json.loads(path.read_text())
        missing = sorted(keys - translations.keys())
        extra = sorted(translations.keys() - keys)
        if missing:
            success = False
            print(f"{path.relative_to(ROOT)} is missing keys:")
            print("\n".join(f"  {key}" for key in missing))
        if extra:
            success = False
            print(f"{path.relative_to(ROOT)} has unused keys:")
            print("\n".join(f"  {key}" for key in extra))
    if success:
        print(f"All {len(keys)} translation keys are present in both language files.")
    sys.exit(not success)


if __name__ == "__main__":
    main()

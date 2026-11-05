#!/usr/bin/env python3
"""Check that relative links and anchors resolve across the repo Markdown."""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

LINK_RE = re.compile(r"\[.*?\]\(([^)#]+)?(#([^)]+))?\)")


def anchors(text: str):
    out = set()
    for line in text.splitlines():
        m = re.match(r"#{1,6}\s+(.*)", line)
        if m:
            slug = re.sub(r"[^\w\s-]", "", m.group(1).lower().strip())
            slug = re.sub(r"[\s_]+", "-", slug)
            out.add(slug)
    # explicit links to section numbers like ## 16. Delivery... produce
    # slugs including leading numbers; GitHub keeps them.
    return out


def main() -> int:
    failures = 0
    files = list(ROOT.rglob("*.md"))
    for f in files:
        if "target" in f.parts or ".git" in f.parts:
            continue
        text = f.read_text(encoding="utf-8")
        for m in LINK_RE.finditer(text):
            path, _, frag = m.group(1) or "", m.group(2) or "", m.group(3) or ""
            if not path or path.startswith(("http://", "https://")):
                continue
            target = (f.parent / path).resolve()
            if not target.exists():
                print(f"{f.relative_to(ROOT)}: broken link -> {path}")
                failures += 1
                continue
            if frag and target.suffix in (".md",):
                t = target.read_text(encoding="utf-8")
                if frag not in anchors(t):
                    print(f"{f.relative_to(ROOT)}: missing anchor #{frag} in {target.relative_to(ROOT)}")
                    failures += 1
    print(f"{len(files)} markdown files, {failures} broken links/anchors")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())

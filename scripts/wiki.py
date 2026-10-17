#!/usr/bin/env python3
"""Turn docs/ into GitHub wiki pages, and optionally publish them.

docs/ is the source of truth. A GitHub wiki is a flat set of pages linked by
name, so this gives every page a unique name, rewrites links between pages to
those names (anchors kept), points links to source files at the file on
GitHub, and adds a sidebar and a footer.

    scripts/wiki.py build OUT       write the wiki pages into OUT
    scripts/wiki.py publish         build, then commit and push to the wiki

Standard library only. Publishing clones the wiki over SSH by default; set
WIKI_REMOTE to use another URL (CI uses an HTTPS URL with a token).
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile

REPO = "birdbree/rs2ool"
BRANCH = "main"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = os.path.join(ROOT, "docs")

# Wiki page name for each section's README, and the prefix for its pages.
SECTIONS = {
    "cache": ("Cache", "Cache"),
    "records": ("Records", "Record"),
    "client": ("Client-behaviour", "Client"),
    "pack": ("Pack-format", "Pack"),
    "conversion": ("Conversion", "Conversion"),
    "tools": ("Tools", "Tools"),
    "crates": ("Crates", "Crate"),
    "development": ("Development", "Development"),
}
TOP = {
    "README.md": "Home",
    "getting-started.md": "Getting-started",
    "architecture.md": "Architecture",
    "glossary.md": "Glossary",
}

LINK = re.compile(r"(!?)\[([^\]]*)\]\(([^)\s]+)\)")


def page_name(rel):
    """Wiki page name for a docs-relative path like `records/model.md`."""
    parts = rel.split("/")
    if len(parts) == 1:
        if rel in TOP:
            return TOP[rel]
        return rel[:-3].capitalize()
    section, file = parts
    name, prefix = SECTIONS[section]
    if file == "README.md":
        return name
    return f"{prefix}-{file[:-3]}"


def pages():
    """Every docs page, as docs-relative paths, sorted."""
    out = []
    for d, _, files in os.walk(DOCS):
        for f in files:
            if f.endswith(".md"):
                out.append(os.path.relpath(os.path.join(d, f), DOCS).replace(os.sep, "/"))
    return sorted(out)


def rewrite(rel, text, names):
    """Rewrite one page's links for the wiki."""
    here = os.path.dirname(os.path.join(DOCS, rel))

    def fix(m):
        bang, label, target = m.groups()
        if re.match(r"[a-z]+:", target) or target.startswith("#"):
            return m.group(0)
        path, _, anchor = target.partition("#")
        abs_path = os.path.normpath(os.path.join(here, path))
        hash_part = f"#{anchor}" if anchor else ""
        docs_rel = os.path.relpath(abs_path, DOCS).replace(os.sep, "/")
        if docs_rel in names:
            return f"{bang}[{label}]({names[docs_rel]}{hash_part})"
        # Anything else is a file or directory in the repository.
        repo_rel = os.path.relpath(abs_path, ROOT).replace(os.sep, "/")
        if repo_rel.startswith(".."):
            raise SystemExit(f"{rel}: link {target} leaves the repository")
        if not os.path.exists(abs_path):
            raise SystemExit(f"{rel}: link {target} points at nothing")
        kind = "tree" if os.path.isdir(abs_path) else "blob"
        return f"{bang}[{label}](https://github.com/{REPO}/{kind}/{BRANCH}/{repo_rel}{hash_part})"

    out, in_code = [], False
    for line in text.split("\n"):
        if line.startswith("```"):
            in_code = not in_code
        out.append(line if in_code else LINK.sub(fix, line))
    return "\n".join(out)


def sidebar(names):
    """A sidebar mirroring the docs home: the top pages, then each section
    with its pages in the order the section's README links them."""
    lines = ["**[Home](Home)**", ""]
    for rel in ["getting-started.md", "architecture.md", "glossary.md"]:
        lines.append(f"- [{title(rel)}]({names[rel]})")
    for section in SECTIONS:
        readme = f"{section}/README.md"
        lines += ["", f"**[{title(readme)}]({names[readme]})**", ""]
        order = []
        text = open(os.path.join(DOCS, readme), encoding="utf-8").read()
        # The section's page list (list items and table rows) sets the order;
        # links in running text would jumble it.
        listed = "\n".join(
            line for line in text.split("\n") if line.lstrip().startswith(("- [", "| ["))
        )
        for _, _, target in LINK.findall(listed):
            path = target.partition("#")[0]
            rel = os.path.normpath(os.path.join(section, path)).replace(os.sep, "/")
            if rel.startswith(section + "/") and rel in names and rel != readme and rel not in order:
                order.append(rel)
        rest = sorted(r for r in names if r.startswith(section + "/") and r != readme and r not in order)
        for rel in order + rest:
            lines.append(f"- [{title(rel)}]({names[rel]})")
    return "\n".join(lines) + "\n"


def title(rel):
    """A page's H1."""
    for line in open(os.path.join(DOCS, rel), encoding="utf-8"):
        if line.startswith("# "):
            return line[2:].strip()
    return page_name(rel)


def build(out):
    names = {rel: page_name(rel) for rel in pages()}
    clash = {}
    for rel, name in names.items():
        clash.setdefault(name.lower(), []).append(rel)
    dupes = {k: v for k, v in clash.items() if len(v) > 1}
    if dupes:
        raise SystemExit(f"page names collide: {dupes}")
    os.makedirs(out, exist_ok=True)
    for rel, name in names.items():
        text = open(os.path.join(DOCS, rel), encoding="utf-8").read()
        with open(os.path.join(out, name + ".md"), "w", encoding="utf-8") as f:
            f.write(rewrite(rel, text, names))
    with open(os.path.join(out, "_Sidebar.md"), "w", encoding="utf-8") as f:
        f.write(sidebar(names))
    with open(os.path.join(out, "_Footer.md"), "w", encoding="utf-8") as f:
        f.write(
            f"This wiki is generated from [`docs/`](https://github.com/{REPO}/tree/{BRANCH}/docs) "
            "in the repository. Edit it there; changes made here are overwritten.\n"
        )
    return len(names)


def publish():
    remote = os.environ.get("WIKI_REMOTE", f"git@github.com:{REPO}.wiki.git")
    sha = subprocess.run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"],
                         capture_output=True, text=True, check=True).stdout.strip()
    with tempfile.TemporaryDirectory() as tmp:
        wiki = os.path.join(tmp, "wiki")
        subprocess.run(["git", "clone", "-q", "--depth", "1", remote, wiki], check=True)
        for f in os.listdir(wiki):
            if f != ".git":
                p = os.path.join(wiki, f)
                shutil.rmtree(p) if os.path.isdir(p) else os.remove(p)
        n = build(wiki)
        git = ["git", "-C", wiki]
        subprocess.run(git + ["add", "-A"], check=True)
        if subprocess.run(git + ["diff", "--cached", "--quiet"]).returncode == 0:
            print(f"wiki already matches docs ({n} pages)")
            return
        subprocess.run(git + ["commit", "-q", "-m", f"Sync from docs at {sha}"], check=True)
        subprocess.run(git + ["push", "-q", "origin", "HEAD"], check=True)
        print(f"published {n} pages from {sha}")


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "build":
        print(f"wrote {build(sys.argv[2])} pages to {sys.argv[2]}")
    elif len(sys.argv) == 2 and sys.argv[1] == "publish":
        publish()
    else:
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()

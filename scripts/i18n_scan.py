#!/usr/bin/env python3
"""Busca textos de la interfaz sin traducir (o los envuelve en t!() con --apply).

Reglas: sólo fuera de comentarios, tests, const/static y atributos; se saltan los literales que
sirven para comparar (contains, starts_with, has, ==, =>), ids de egui, rutas y URLs.
"""
import re, sys, json, pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
EXTRA = [ROOT / "src/game/diagnose.rs"]
FILES = EXTRA + sorted([p for p in (ROOT / "src/gui").rglob("*.rs") if p.name not in ("logline.rs", "fonts.rs", "theme.rs", "render3d.rs", "state.rs") and p.relative_to(ROOT).as_posix() != "src/gui/mod.rs"])
LIT = re.compile(r'(?<![A-Za-z0-9_#])"((?:[^"\\\n]|\\.)*)"')
PH = re.compile(r'\{\{|\}\}|\{[^{}]*\}')
SKIP_BEFORE = re.compile(r'(Id::new\(\(?|id_salt\(|\.id\(|push_id\(|contains\(|starts_with\(|ends_with\(|strip_prefix\(|strip_suffix\(|has\(|find\(|split\(|split_once\(|==\s*|!=\s*|load_texture\(|get_temp::<[^>]*>\(|insert_temp\(|include_str!\(|include_bytes!\(|env!\(|option_env!\(|hyperlink_to\([^,]*,\s*|open_url\(|OpenUrl\(|matches!\([^,]*,\s*|monospace\(|eq_ignore_ascii_case\(|join\(|replace\(|(?<![a-z])t!\(|(?<![a-z_])tr\(|expect\(|unwrap_or\(\s*)\s*$')

def translatable(s):
    if "http" in s or s.startswith("/") or (" " not in s and re.search(r'\.(png|json|toml|jar|zip|mrpack|txt|md|log)\b', s)):
        return False
    bare = PH.sub("", s)
    letters = [c for c in bare if c.isalpha()]
    if len(letters) < 2:
        return False
    spaced = PH.sub("X", s).strip()
    return (" " in spaced) or any(ord(c) > 127 and c.isalpha() for c in bare) or bare.strip()[:1].isupper()

def code_regions(src):
    """Marca qué caracteres son código «normal» (no comentario, no test, no const/static)."""
    ok = [True] * len(src)
    cut = src.find("#[cfg(test)]")
    if cut != -1:
        for i in range(cut, len(src)):
            ok[i] = False
    # comentarios de línea
    for m in re.finditer(r'//[^\n]*', src):
        # no es comentario si está dentro de un string: aproximación (las URLs se saltan igual)
        line_start = src.rfind("\n", 0, m.start()) + 1
        before = src[line_start:m.start()]
        if before.count('"') % 2 == 1:
            continue
        for i in range(m.start(), m.end()):
            ok[i] = False
    # const / static (datos en tiempo de compilación) y atributos
    for m in re.finditer(r'^\s*(pub(\([^)]*\))?\s+)?(const|static)\s[^\n]*', src, re.M):
        if "fn " in m.group(0):
            continue
        j = src.find(";\n", m.start())
        for i in range(m.start(), j if j != -1 else m.end()):
            ok[i] = False
    for m in re.finditer(r'#\[[^\]]*\]', src):
        for i in range(m.start(), m.end()):
            ok[i] = False
    return ok

def is_match_pattern(src, end):
    rest = src[end:end + 40]
    return re.match(r'\s*(=>|\|[^|])', rest) is not None

def find_call_end(src, start):
    """Desde justo tras el literal de format!(, devuelve el índice del ')' que lo cierra."""
    depth, i, n = 1, start, len(src)
    while i < n:
        c = src[i]
        if c == '"':
            i += 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
        elif c == "'" and re.match(r"'(\\.|[^\\'])'", src[i:i + 4]):
            i += src[i:i + 4].index("'", 1)
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1

def scan(path, apply=False):
    src = path.read_text()
    ok = code_regions(src)
    found, edits = [], []
    for m in LIT.finditer(src):
        s, a, b = m.group(1), m.start(), m.end()
        if not ok[a] or not translatable(s):
            continue
        before = src[max(0, a - 60):a]
        if SKIP_BEFORE.search(before) or is_match_pattern(src, b):
            continue
        if src[max(0, a - 1):a] == "r":
            continue
        fmt = re.search(r'format!\(\s*$', before)
        found.append(s)
        if not apply:
            continue
        if fmt:
            fs = a - (len(before) - fmt.start())
            close = find_call_end(src, b)
            args = src[b:close]
            named = set(re.findall(r'([A-Za-z_][A-Za-z0-9_]*)\s*=(?!=)', args))
            inline = []
            for ph in re.findall(r'(?<!\{)\{([A-Za-z_][A-Za-z0-9_]*)(?::[^}]*)?\}', s):
                if ph not in named and ph not in inline:
                    inline.append(ph)
            extra = "".join(f", {v} = {v}" for v in inline)
            edits.append((fs, fs + len("format!("), "t!("))
            if extra:
                edits.append((close, close, extra))
        else:
            edits.append((a, a, "t!("))
            edits.append((b, b, ")"))
    if apply and edits:
        out = src
        for s0, e0, rep in sorted(edits, key=lambda x: (x[0], x[1]), reverse=True):
            out = out[:s0] + rep + out[e0:]
        path.write_text(out)
    return found

if __name__ == "__main__":
    apply = "--apply" in sys.argv
    allk = {}
    for f in FILES:
        for s in scan(f, apply):
            allk.setdefault(s, f.name)
    if "--json" in sys.argv:
        print(json.dumps(allk, ensure_ascii=False, indent=0))
    else:
        for k, f in allk.items():
            print(f"{f}\t{k}")
        print(len(allk), "textos", file=sys.stderr)

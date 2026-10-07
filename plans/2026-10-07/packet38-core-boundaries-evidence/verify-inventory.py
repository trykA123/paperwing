import difflib
import json
from pathlib import Path
import re
import subprocess


root = Path(__file__).resolve().parents[3]
evidence = Path(__file__).resolve().parent
baseline = (evidence / "baseline-revision.txt").read_text().strip()
before_lib = subprocess.check_output(
    ["git", "show", f"{baseline}:src-tauri/src/lib.rs"], cwd=root
)
before_handler = before_lib.split(b"tauri::generate_handler![", 1)[1].split(b"])", 1)[0]
after_handlers = b"\n".join(
    path.read_bytes().split(b"domain! {", 1)[1].split(b"}", 1)[0]
    for path in sorted((root / "src-tauri/src/commands").glob("*.rs"))
    if path.name != "mod.rs"
)
registration = rb"(?m)^\s*(?:#\[cfg\([^\n]+\)\]\s*)?([\w:]+),\s*$"
before_entries = re.findall(registration, before_handler)
after_entries = re.findall(registration, after_handlers)
assert sorted(before_entries) == sorted(after_entries)
names = sorted({entry.decode().rsplit("::", 1)[-1] for entry in after_entries})
after_names = "\n".join(names) + "\n"
(evidence / "commands-after.txt").write_text(after_names)

signature = re.compile(
    r"#\[tauri::command(?:\([^]]*\))?\]\s*"
    r"(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*([^{}]+)\{"
)
actual = {}
for path in (root / "src-tauri/src").rglob("*.rs"):
    for name, value in signature.findall(path.read_text()):
        actual[f"{path.relative_to(root)}:{name}"] = " ".join(value.split())
before_signatures = json.loads((evidence / "signatures-before.json").read_text())
ordered = {key: actual[key] for key in before_signatures if key in actual}
ordered.update({key: actual[key] for key in sorted(actual.keys() - ordered.keys())})
(evidence / "signatures-after.json").write_text(json.dumps(ordered, indent=2))

for name in ("commands", "signatures"):
    extension = "txt" if name == "commands" else "json"
    before = (evidence / f"{name}-before.{extension}").read_text()
    after = (evidence / f"{name}-after.{extension}").read_text()
    diff = "".join(difflib.unified_diff(before.splitlines(True), after.splitlines(True)))
    (evidence / f"{name}.diff").write_text(diff)
    assert not diff, f"{name} changed"

windows = [line for line in before_handler.splitlines(True) if b"#[cfg(windows)]" in line]
after_windows = [line for line in after_handlers.splitlines(True) if b"#[cfg(windows)]" in line]
assert windows == after_windows
(evidence / "windows-moved-items.txt").write_bytes(b"".join(windows))
protected = [
    path for path in (root / "src-tauri/src").rglob("*")
    if path.is_file() and (
        path.name in {"files.rs", "commit.rs", "stash.rs"}
        or path.name.startswith(("file_guard", "linux_"))
        or "linux_files" in path.parts
        or "stash" in path.parts
    ) and "commands" not in path.parts
]
for path in protected:
    original = subprocess.check_output(["git", "show", f"{baseline}:{path.relative_to(root)}"], cwd=root)
    assert original == path.read_bytes(), f"Protected file changed: {path}"
summary = {
    "uniqueCommandNames": len(names),
    "rawRegistrations": len(after_entries),
    "commandSignatures": len(actual),
    "commandDiffBytes": 0,
    "signatureDiffBytes": 0,
    "windowsRegistrationLinesByteIdentical": len(windows),
    "protectedFilesByteIdentical": len(protected),
}
(evidence / "inventory-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
print(json.dumps(summary, indent=2))

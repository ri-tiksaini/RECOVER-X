'''Enforces [LAW-2] Layer Purity: L6 UI must never import _recoverx_py directly.'''
from pathlib import Path


def test_ui_never_imports_native_extension_directly():
    ui_dir = Path("app/ui")
    for py_file in ui_dir.rglob("*.py"):
        src = py_file.read_text(encoding="utf-8")
        assert "_recoverx_py" not in src, f"LAW-2 violation: {py_file} imports _recoverx_py directly"

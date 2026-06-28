import ast
import sys
from pathlib import Path

LAYER_ROOTS = ("config", "domain", "infrastructure", "use_cases", "entrypoints")
FORBIDDEN_IMPORTS = {
    "config": ("domain", "infrastructure", "use_cases", "entrypoints"),
    "domain": ("config", "infrastructure", "use_cases", "entrypoints"),
    "infrastructure": ("use_cases", "entrypoints"),
    "use_cases": ("entrypoints",),
    "entrypoints": (),
}


def main() -> int:
    """
    Check source imports for forbidden layer dependencies.
    """
    violations: list[str] = []
    for path in sorted(Path("src").rglob("*.py")):
        source_layer = get_source_layer(path)
        if not source_layer:
            continue

        tree = ast.parse(path.read_text(), filename=str(path))
        for node in ast.walk(tree):
            imported_modules = get_imported_modules(node)
            for line_number, imported_module in imported_modules:
                imported_layer = get_imported_layer(imported_module)
                if imported_layer in FORBIDDEN_IMPORTS[source_layer]:
                    violations.append(
                        f"{path}:{line_number}: {source_layer} must not import {imported_layer}: {imported_module}"
                    )

    if violations:
        print("Layer import violations detected:")
        print("\n".join(violations))
        return 1

    return 0


def get_source_layer(path: Path) -> str:
    """
    Return the source layer for a Python file path.
    """
    parts = path.parts
    if len(parts) >= 2 and parts[0] == "src" and parts[1] in LAYER_ROOTS:
        return parts[1]
    return ""


def get_imported_modules(node: ast.AST) -> list[tuple[int, str]]:
    """
    Return absolute module names imported by one AST node.
    """
    if isinstance(node, ast.Import):
        return [(node.lineno, alias.name) for alias in node.names]

    if isinstance(node, ast.ImportFrom) and node.level == 0 and node.module:
        return [(node.lineno, node.module)]

    return []


def get_imported_layer(module_name: str) -> str:
    """
    Return the layer name from an absolute src module import.
    """
    parts = module_name.split(".")
    if len(parts) >= 2 and parts[0] == "src" and parts[1] in LAYER_ROOTS:
        return parts[1]
    return ""


if __name__ == "__main__":
    sys.exit(main())

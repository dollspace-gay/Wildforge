"""Small deterministic TOML writer for nested campaign declarations."""

import json
import math
from pathlib import Path


def scalar(value):
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=True)
    if isinstance(value, bool):
        return str(value).lower()
    if isinstance(value, (int, float)):
        if isinstance(value, float) and not math.isfinite(value):
            raise ValueError("campaign metadata must be finite")
        return repr(value)
    if isinstance(value, list) and all(not isinstance(item, dict) for item in value):
        return "[" + ", ".join(scalar(item) for item in value) + "]"
    raise ValueError(f"unsupported campaign value: {type(value).__name__}")


def render(document: dict) -> str:
    lines = []

    def table(values, prefix):
        for key, value in values.items():
            if not isinstance(value, dict) and not (isinstance(value, list) and value and isinstance(value[0], dict)):
                lines.append(f"{key} = {scalar(value)}")
        for key, value in values.items():
            name = ".".join((*prefix, key))
            if isinstance(value, dict):
                lines.extend(("", f"[{name}]"))
                table(value, (*prefix, key))
            elif isinstance(value, list) and value and isinstance(value[0], dict):
                for item in value:
                    lines.extend(("", f"[[{name}]]"))
                    table(item, (*prefix, key))

    table(document, ())
    return "\n".join(lines) + "\n"


def write(path: Path, document: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(render(document))

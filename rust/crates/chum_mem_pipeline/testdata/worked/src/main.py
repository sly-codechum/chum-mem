"""Entry point for the data processing pipeline."""

import os
import sys
from pathlib import Path
from typing import List, Optional

# WHY: We lazy-import heavy deps to keep CLI startup fast
from pipeline.core import Engine


class Config:
    """Holds runtime configuration for the pipeline."""

    def __init__(self, base_dir: str, workers: int = 4):
        self.base_dir = Path(base_dir).resolve()
        self.workers = workers

    def validate(self) -> bool:
        """Return True if the config is usable."""
        return self.base_dir.exists() and self.workers > 0


def run_pipeline(config: Config, targets: Optional[List[str]] = None) -> int:
    """Execute the full pipeline and return an exit code.

    NOTE: targets=None means process everything in base_dir.
    """
    engine = Engine(config)
    engine.load_plugins()
    results = engine.execute(targets or [])
    print(f"Processed {len(results)} items")
    return 0 if all(r.ok for r in results) else 1


if __name__ == "__main__":
    cfg = Config(base_dir=os.getcwd(), workers=int(sys.argv[1]) if len(sys.argv) > 1 else 4)
    cfg.validate()
    exit_code = run_pipeline(cfg)
    sys.exit(exit_code)

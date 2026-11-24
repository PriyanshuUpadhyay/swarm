"""Run with python3 -I scripts/test_public_safety_copy.py."""
from pathlib import Path
import unittest

REPO = Path(__file__).resolve().parents[1]


class PublicSafetyCopyTest(unittest.TestCase):
    def test_the_kit_copy_matches_the_repo_copy(self):
        # CI runs only the repo copy, so a drift in the kit copy that ships would stay unseen.
        repo_copy = REPO / "scripts/public-safety.sh"
        kit_copy = REPO / "skills/kit/scripts/public-safety.sh"
        self.assertEqual(repo_copy.read_bytes(), kit_copy.read_bytes(),
                         f"{kit_copy} differs from {repo_copy}; copy {repo_copy} over {kit_copy}")


if __name__ == "__main__":
    unittest.main()

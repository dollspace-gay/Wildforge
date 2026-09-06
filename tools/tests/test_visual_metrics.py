"""Shared readability gates retain exact bounds in ordinary and closeout campaigns."""

import math
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_visual_polish import distinguish_families, fog_endpoint_passes


class VisualMetricTests(unittest.TestCase):
    def test_fog_requires_the_endpoint_and_rejects_renewed_structure(self):
        pre = dict(expected_fog_blend=0.8, rms_contrast_4px=0.1,
                   rms_contrast_16px=0.1, silhouette_weber_magnitude=0.1)
        end = dict(pre, expected_fog_blend=0.999)
        self.assertTrue(fog_endpoint_passes(pre, end))
        self.assertFalse(fog_endpoint_passes(pre, dict(end, expected_fog_blend=0.998)))
        for metric in ('rms_contrast_4px', 'rms_contrast_16px', 'silhouette_weber_magnitude'):
            boundary = pre[metric] + 2.0 / 255.0
            self.assertTrue(fog_endpoint_passes(pre, dict(end, **{metric: boundary})))
            self.assertFalse(fog_endpoint_passes(pre, dict(end, **{metric: math.nextafter(boundary, math.inf)})))
            self.assertFalse(fog_endpoint_passes(pre, dict(end, **{metric: math.nan})))
        with self.assertRaises(KeyError):
            fog_endpoint_passes(pre, {})

    def test_families_require_two_other_distinct_materials_in_each_band(self):
        cases = ('marble', 'chalk', 'limestone')
        representatives = {
            (rock, band): {'rms_contrast_16px': index * 0.01, 'median_chroma': 0.0}
            for index, rock in enumerate(cases) for band in ('near', 'middle')
        }
        rows = distinguish_families(cases, representatives)
        self.assertEqual(len(rows), 6)
        self.assertTrue(all(row['passed'] for row in rows))
        self.assertTrue(all(row['rock'] not in row['distinguishable_from'] for row in rows))
        representatives[('chalk', 'near')] = representatives[('marble', 'near')]
        rows = distinguish_families(cases, representatives)
        failures = {(row['rock'], row['distance_band']) for row in rows if not row['passed']}
        self.assertEqual(failures, {('marble', 'near'), ('chalk', 'near')})


if __name__ == '__main__':
    unittest.main()

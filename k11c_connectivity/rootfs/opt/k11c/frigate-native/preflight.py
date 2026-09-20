# SPDX-License-Identifier: AGPL-3.0-or-later
"""Run in an isolated container of the TARGET official image, without an NPU."""
import ctypes
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, '/opt/frigate')
from pydantic import TypeAdapter
from frigate.config import FrigateConfig
from frigate.detectors.detector_types import api_types, DetectorConfig
from frigate.detectors.plugins.k11c_rknn import K11cRknn, verify_payload, runtime_class, timing_class

verify_payload()
assert api_types['k11c_rknn'] is K11cRknn
assert TypeAdapter(DetectorConfig).validate_python({'type': 'k11c_rknn'}).type == 'k11c_rknn'
assert TypeAdapter(DetectorConfig).validate_python({'type': 'k11c_rknn'}).timing_samples == 0
assert TypeAdapter(DetectorConfig).validate_python({'type': 'k11c_rknn', 'timing_samples': 256}).timing_samples == 256
assert 'k11c_rknn' in str(TypeAdapter(DetectorConfig).json_schema())
config = FrigateConfig(mqtt={'enabled': False}, cameras={},
                      detectors={'k11c': {'type': 'k11c_rknn'}},
                      model={'path': '/opt/k11c-native/model.rknn', 'model_type': 'yolox',
                             'width': 416, 'height': 416, 'input_tensor': 'nhwc',
                             'input_pixel_format': 'rgb', 'labelmap_path': '/labelmap/coco-80.txt'})
assert config.detectors['k11c'].type == 'k11c_rknn'
lib = ctypes.CDLL('/usr/lib/librknnrt.so')
for name in ('rknn_init', 'rknn_query', 'rknn_inputs_set', 'rknn_run',
             'rknn_outputs_get', 'rknn_outputs_release', 'rknn_destroy'):
    assert callable(getattr(lib, name))
assert callable(runtime_class())
assert timing_class()(0).want_sample() is False
assert callable(K11cRknn.calculate_grids_strides)
for name in ('ffmpeg', 'ffprobe'):
    subprocess.run(['/config/k11c-ffmpeg-vpu-r1/bin/' + name, '-version'],
                   check=True, timeout=15, stdout=subprocess.DEVNULL)
accels = subprocess.check_output(['/config/k11c-ffmpeg-vpu-r1/bin/ffmpeg', '-hwaccels'],
                                 timeout=15, stderr=subprocess.DEVNULL)
assert b'v4l2request' in accels.splitlines()
print('K11C_FFMPEG_PREFLIGHT_PASS binaries=executable backend=v4l2request hardware=not-tested', flush=True)
print('K11C_NATIVE_PREFLIGHT_PASS registry=schema+factory runtime=load-only no-inference', flush=True)

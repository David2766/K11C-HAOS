# SPDX-License-Identifier: AGPL-3.0-or-later
"""Additional Frigate detector; no HTTP transport or upstream source replacement."""
import hashlib
import importlib.util
import logging
from pathlib import Path
from typing import Literal

import numpy as np
from pydantic import Field
from frigate.detectors.detection_api import DetectionApi
from frigate.detectors.detector_config import BaseDetectorConfig, ModelTypeEnum
from frigate.detectors.plugins.rknn import Rknn

ROOT = Path('/opt/k11c-native')
MODEL_HASH = '7a061fc25dcda5b612bc217d7ad222436f9273abbbf5b2776b7ab07fb645f294'
LIB_HASH = 'd31fc19c85b85f6091b2bd0f6af9d962d5264a4e410bfb536402ec92bac738e8'
log = logging.getLogger(__name__)


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def verify_payload():
    compatible = (ROOT / 'compatible').read_bytes().split(b'\0')
    require(b'kickpi,k11c' in compatible and b'rockchip,rk3566' in compatible,
            'Connectivity did not supply a K11C / RK3566 device tree identity')
    for path, expected in ((ROOT / 'model.rknn', MODEL_HASH),
                           (Path('/usr/lib/librknnrt.so'), LIB_HASH)):
        require(hashlib.sha256(path.read_bytes()).hexdigest() == expected,
                'Native asset mismatch: ' + path.name)


def runtime_class():
    # Load only our own module, without inserting a directory ahead of Frigate's
    # Python packages or changing global open(), sys.modules, or SDK functions.
    spec = importlib.util.spec_from_file_location('k11c_native_runtime', ROOT / 'k11c_runtime.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.NativeRuntime


def timing_class():
    spec = importlib.util.spec_from_file_location('k11c_native_timing', ROOT / 'timing.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.NativeTiming


class K11cRknnConfig(BaseDetectorConfig):
    # Direct subclass: Frigate discovers ONLY direct BaseDetectorConfig children.
    type: Literal['k11c_rknn']
    timing_samples: int = Field(default=0, ge=0, le=1024, strict=True)
    timing_delay_seconds: int = Field(default=60, ge=0, le=600, strict=True)


class K11cRknn(DetectionApi):
    # Likewise, do not derive this registered class from Rknn.
    type_key = 'k11c_rknn'

    def __init__(self, config: K11cRknnConfig):
        super().__init__(config)
        self.runtime = None
        verify_payload()
        model = config.model
        require(model.model_type == ModelTypeEnum.yolox and
                (model.width, model.height) == (416, 416) and
                model.input_tensor == 'nhwc' and model.input_pixel_format == 'rgb' and
                model.path == str(ROOT / 'model.rknn'),
                'Use the supplied 416px NHWC RGB YOLOX model configuration')
        self.width, self.height = model.width, model.height
        self.calculate_grids_strides(expanded=False)
        self.runtime = runtime_class()(model.path)
        self.timing = timing_class()(config.timing_samples, config.timing_delay_seconds)
        log.info('K11C_NATIVE_READY transport=direct-c-api postprocess=upstream-rknn contexts=1')

    def detect_raw(self, tensor_input):
        # Frigate supplies the region tensor, not a JPEG. The buffer lease lasts
        # through upstream postprocessing, which modifies the YOLOX output views.
        if self.timing.want_sample():
            return self.timing.measure(self.runtime, tensor_input,
                                      lambda outputs: Rknn.post_process_yolox(
                                          self, outputs, self.grids, self.expanded_strides))
        tensor = np.ascontiguousarray(tensor_input)
        with self.runtime.borrow_outputs(tensor) as outputs:
            return Rknn.post_process_yolox(self, outputs, self.grids, self.expanded_strides)

    def __del__(self):
        runtime = getattr(self, 'runtime', None)
        if runtime is not None:
            runtime.close()

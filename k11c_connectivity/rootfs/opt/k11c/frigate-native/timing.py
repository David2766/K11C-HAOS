# SPDX-License-Identifier: AGPL-3.0-or-later
"""Opt-in, bounded measurements of real native detector calls. No image capture."""
import json
import logging
import math
import multiprocessing
import os
import statistics
import time

import numpy as np

log = logging.getLogger(__name__)
SDK_FIELDS = ('buffer_wait', 'queue_wait', 'inputs_set', 'run', 'outputs_get',
              'outputs_validate', 'outputs_release')
WALL_PARTS = ('prepare',) + SDK_FIELDS + ('native_other', 'postprocess', 'lease_return')
FIELDS = WALL_PARTS + ('total', 'native_thread_cpu', 'postprocess_thread_cpu', 'total_thread_cpu')


def summarize(values):
    ordered = sorted(values)
    return {name: round(value, 6) for name, value in (
        ('mean', statistics.fmean(values)), ('p50', statistics.median(ordered)),
        ('p95', ordered[math.ceil(len(ordered) * .95) - 1]),
        ('min', ordered[0]), ('max', ordered[-1]))}


class NativeTiming:
    def __init__(self, samples=0, delay_seconds=60):
        if type(samples) is not int or not 0 <= samples <= 1024:
            raise ValueError('timing_samples must be 0..1024')
        if type(delay_seconds) is not int or not 0 <= delay_seconds <= 600:
            raise ValueError('timing_delay_seconds must be 0..600')
        self.limit = self.remaining = samples
        self.delay = delay_seconds
        self.not_before = time.monotonic() + delay_seconds if samples else 0
        self.rows = []
        self.started = None
        self.record_cpu_ms = 0.0
        if samples:
            log.info('K11C_NATIVE_TIMING_ARMED pid=%d samples=%d delay_seconds=%d',
                     os.getpid(), samples, delay_seconds)

    def want_sample(self):
        return self.remaining > 0 and time.monotonic() >= self.not_before

    def measure(self, runtime, tensor_input, postprocess):
        """Same contiguous input, SDK lease and upstream postprocess as normal mode."""
        start, cpu_start = time.perf_counter(), time.thread_time()
        try:
            tensor = np.ascontiguousarray(tensor_input)
            prepared = time.perf_counter()
            native_cpu_start = time.thread_time()
            with runtime.borrow_outputs(tensor) as outputs:
                native_end, native_cpu_end = time.perf_counter(), time.thread_time()
                # Capture before postprocessing; never reuse a failed call's stages.
                stages = dict(runtime.last_timings_ms)
                post_start, post_cpu_start = time.perf_counter(), time.thread_time()
                result = postprocess(outputs)
                post_end, post_cpu_end = time.perf_counter(), time.thread_time()
            end, cpu_end = time.perf_counter(), time.thread_time()
        except BaseException:
            self.remaining = 0
            self.rows.clear()
            log.warning('K11C_NATIVE_TIMING_ABORTED pid=%d reason=inference_failed', os.getpid())
            raise

        # Timing failures must not replace successful detections or alter NMS.
        try:
            record_start = time.thread_time()
            row = {name: stages[name] for name in SDK_FIELDS}
            if any(not math.isfinite(value) or value < 0 for value in row.values()):
                raise ValueError('invalid SDK timing')
            native_ms = (native_end - prepared) * 1000
            residual = native_ms - sum(row.values())
            row.update(prepare=(prepared-start)*1000,
                       # Includes Python/ctypes setup and copying the small timing dict,
                       # NOT a tensor copy or a pure scheduler-wait measurement.
                       native_other=max(0.0, residual) + (post_start-native_end)*1000,
                       postprocess=(post_end-post_start)*1000,
                       lease_return=(end-post_end)*1000,
                       total=(end-start)*1000,
                       native_thread_cpu=(native_cpu_end-native_cpu_start)*1000,
                       postprocess_thread_cpu=(post_cpu_end-post_cpu_start)*1000,
                       total_thread_cpu=(cpu_end-cpu_start)*1000)
            if any(not math.isfinite(value) or value < 0 for value in row.values()):
                raise ValueError('invalid measured timing')
            if not math.isclose(sum(row[name] for name in WALL_PARTS), row['total'], abs_tol=.002):
                raise ValueError('non-additive measured timing')
            if self.started is None:
                self.started = start
            self.rows.append(row)
            self.remaining -= 1
            self.record_cpu_ms += (time.thread_time()-record_start)*1000
            if self.remaining == 0:
                report = {
                    'revision': 1, 'pid': os.getpid(),
                    'process': multiprocessing.current_process().name,
                    'samples': len(self.rows), 'requested_samples': self.limit,
                    'delay_seconds': self.delay, 'window_seconds': round(end-self.started, 6),
                    'measurement': 'native_detect_raw_wall_and_calling_thread_cpu',
                    'wall_parts': list(WALL_PARTS),
                    'ms': {name: summarize([sample[name] for sample in self.rows]) for name in FIELDS},
                    'record_thread_cpu_ms': round(self.record_cpu_ms, 6),
                    'note': 'SDK wall time is not pure NPU time. queue_wait is the per-context Python lock, not the Frigate queue or cross-context NPU scheduling. Thread CPU excludes library helper threads. Per-stage medians/p95 do not add. Total excludes summary construction/logging and the outer Frigate wrapper.'}
                self.rows.clear()
                log.info('K11C_NATIVE_TIMING_SUMMARY %s', json.dumps(report, separators=(',', ':'), allow_nan=False))
        except Exception:
            self.remaining = 0
            self.rows.clear()
            log.warning('K11C_NATIVE_TIMING_ABORTED pid=%d reason=invalid_measurement', os.getpid())
        return result

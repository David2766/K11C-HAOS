#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Minimal RKNN 2.3.2 C API binding for the pinned YOLOX model, not RKNN Lite."""
import ctypes as C
from contextlib import contextmanager
from pathlib import Path
from queue import Empty, LifoQueue
import threading
import time
import numpy as np


class Tensor(C.Structure):
    _fields_ = [('index', C.c_uint32), ('n_dims', C.c_uint32), ('dims', C.c_uint32*16),
                ('name', C.c_char*256), ('n_elems', C.c_uint32), ('size', C.c_uint32),
                ('fmt', C.c_int), ('type', C.c_int), ('qnt_type', C.c_int),
                ('fl', C.c_int8), ('zp', C.c_int32), ('scale', C.c_float),
                ('w_stride', C.c_uint32), ('size_with_stride', C.c_uint32),
                ('pass_through', C.c_uint8), ('h_stride', C.c_uint32)]


class Input(C.Structure):
    _fields_ = [('index', C.c_uint32), ('buf', C.c_void_p), ('size', C.c_uint32),
                ('pass_through', C.c_uint8), ('type', C.c_int), ('fmt', C.c_int)]


class Output(C.Structure):
    _fields_ = [('want_float', C.c_uint8), ('is_prealloc', C.c_uint8),
                ('index', C.c_uint32), ('buf', C.c_void_p), ('size', C.c_uint32)]


class Run(C.Structure):
    _fields_ = [('frame_id', C.c_uint64), ('non_block', C.c_int32),
                ('timeout_ms', C.c_int32), ('fence_fd', C.c_int32)]


class Counts(C.Structure):
    _fields_ = [('n_input', C.c_uint32), ('n_output', C.c_uint32)]


class Version(C.Structure):
    _fields_ = [('api', C.c_char*256), ('driver', C.c_char*256)]


class NativeRuntime:
    def __init__(self, model):
        self._context_lock = threading.Lock()
        self._request = threading.local()
        if C.sizeof(C.c_void_p) != 8:
            raise RuntimeError('Requires 64-bit userspace')
        self.ctx = C.c_uint64()
        self.lib = C.CDLL('/usr/lib/librknnrt.so')
        signatures = {
            'rknn_init': [C.POINTER(C.c_uint64), C.c_void_p, C.c_uint32, C.c_uint32, C.c_void_p],
            'rknn_query': [C.c_uint64, C.c_int, C.c_void_p, C.c_uint32],
            'rknn_inputs_set': [C.c_uint64, C.c_uint32, C.POINTER(Input)],
            'rknn_run': [C.c_uint64, C.POINTER(Run)],
            'rknn_outputs_get': [C.c_uint64, C.c_uint32, C.POINTER(Output), C.c_void_p],
            'rknn_outputs_release': [C.c_uint64, C.c_uint32, C.POINTER(Output)],
            'rknn_destroy': [C.c_uint64],
        }
        for name, args in signatures.items():
            function = getattr(self.lib, name)
            function.argtypes, function.restype = args, C.c_int
        model = Path(model).read_bytes()
        if not 0 < len(model) <= 16*1024*1024:
            raise ValueError('Invalid model size')
        self.model_buffer = C.create_string_buffer(model)
        try:
            self.check(self.lib.rknn_init(C.byref(self.ctx), self.model_buffer, len(model), 0, None), 'init')
            counts = self.query(0, Counts())
            if (counts.n_input, counts.n_output) != (1, 3):
                raise ValueError('Expected YOLOX 1 input / 3 outputs')
            input_attr = self.query(1, Tensor())
            shape = tuple(input_attr.dims[:input_attr.n_dims])
            if (shape, input_attr.fmt) not in (((1,3,416,416),0), ((1,416,416,3),1)) or input_attr.n_elems != 416*416*3:
                raise ValueError('Unexpected model input shape')
            self.attrs = [self.query(2, Tensor(index=i)) for i in range(3)]
            self.shapes = []
            for attr in self.attrs:
                dims = tuple(attr.dims[:attr.n_dims])
                if attr.fmt == 0 and dims in [(1,85,n,n) for n in (52,26,13)]:
                    normalized = dims
                elif attr.fmt == 1 and dims in [(1,n,n,85) for n in (52,26,13)]:
                    normalized = (1,85,dims[1],dims[2])
                else:
                    raise ValueError('Unexpected YOLOX output shape: ' + str(dims))
                if attr.n_elems != int(np.prod(normalized)):
                    raise ValueError('Output element count mismatch')
                self.shapes.append((dims, normalized))
            if sorted(shape[1][2] for shape in self.shapes) != [13,26,52]:
                raise ValueError('Duplicate or missing YOLOX output scale')
            version = self.query(5, Version())
            if not version.api.startswith(b'2.3.2') or not version.driver.startswith(b'0.9.8'):
                raise ValueError('Unexpected runtime/driver version')
            # Two leases, not a queue of images. A lease lasts through CPU
            # postprocessing, while the context lock covers only SDK calls.
            self._output_slots = LifoQueue(maxsize=2)
            for _ in range(2):
                self._output_slots.put_nowait(tuple(
                    np.empty(attr.n_elems, dtype=np.float32) for attr in self.attrs))
            print('RKNN_NATIVE api=%s driver=%s outputs=%s' %
                  (version.api.decode(), version.driver.decode(), self.shapes), flush=True)
            print('RKNN_OUTPUT_BUFFERS preallocated=2 want_float=1', flush=True)
        except Exception:
            self.close()
            raise

    @staticmethod
    def check(code, operation):
        if code != 0:
            raise RuntimeError('RKNN %s failed: %d' % (operation, code))

    def query(self, command, value):
        self.check(self.lib.rknn_query(self.ctx, command, C.byref(value), C.sizeof(value)), 'query')
        return value

    def inference(self, tensor):
        """Owned-output convenience API; production uses borrow_outputs directly."""
        with self.borrow_outputs(tensor) as outputs:
            return [array.copy() for array in outputs]

    @contextmanager
    def borrow_outputs(self, tensor):
        """Outputs are valid only inside this scope; do not retain their views."""
        self.last_timings_ms = {}
        start = time.perf_counter()
        try:
            slot = self._output_slots.get(timeout=5)
        except Empty as error:
            raise RuntimeError('RKNN output buffers busy') from error
        buffer_wait_ms = (time.perf_counter() - start) * 1000
        try:
            start = time.perf_counter()
            if not self._context_lock.acquire(timeout=5):
                raise RuntimeError('RKNN context busy')
            waited_ms = (time.perf_counter() - start) * 1000
            try:
                if not self.ctx.value:
                    raise RuntimeError('RKNN context closed')
                result = self._inference(tensor, slot)
                self.last_timings_ms.update(buffer_wait=buffer_wait_ms, queue_wait=waited_ms)
            finally:
                self._context_lock.release()
            yield result
        finally:
            self._output_slots.put_nowait(slot)

    @property
    def last_timings_ms(self):
        return getattr(self._request, 'timings', {})

    @last_timings_ms.setter
    def last_timings_ms(self, value):
        self._request.timings = value

    def _inference(self, tensor, slot):
        # The context lock covers input submission through SDK output release.
        # Publish only complete timings from this call, never stale failure data.
        stages = {}
        if tensor.shape != (1,416,416,3) or tensor.dtype != np.uint8 or not tensor.flags.c_contiguous:
            raise ValueError('Expected contiguous 416px NHWC uint8 input')
        source = Input(index=0, buf=tensor.ctypes.data, size=tensor.nbytes, type=3, fmt=1)
        start = time.perf_counter()
        self.check(self.lib.rknn_inputs_set(self.ctx, 1, C.byref(source)), 'inputs_set')
        stages['inputs_set'] = (time.perf_counter() - start) * 1000
        extension = Run(timeout_ms=5000, fence_fd=-1)
        start = time.perf_counter()
        self.check(self.lib.rknn_run(self.ctx, C.byref(extension)), 'run')
        stages['run'] = (time.perf_counter() - start) * 1000
        outputs = (Output*3)(*(Output(index=i, want_float=1, is_prealloc=1,
                                     buf=array.ctypes.data, size=array.nbytes)
                              for i, array in enumerate(slot)))
        start = time.perf_counter()
        self.check(self.lib.rknn_outputs_get(self.ctx, 3, outputs, None), 'outputs_get')
        stages['outputs_get'] = (time.perf_counter() - start) * 1000
        result = []
        try:
            start = time.perf_counter()
            for output, attr, (dims, normalized), buffer in zip(outputs, self.attrs, self.shapes, slot):
                if (output.buf != buffer.ctypes.data or output.size != attr.n_elems * 4
                        or output.is_prealloc != 1 or output.want_float != 1):
                    raise ValueError('Invalid float output buffer')
                array = buffer.reshape(dims)
                if attr.fmt == 1:
                    array = array.transpose(0,3,1,2)
                result.append(array)
            stages['outputs_validate'] = (time.perf_counter() - start) * 1000
        finally:
            start = time.perf_counter()
            # Ownership does not change, even if validation rejected SDK metadata.
            for output in outputs:
                output.is_prealloc = 1
            self.check(self.lib.rknn_outputs_release(self.ctx, 3, outputs), 'outputs_release')
            stages['outputs_release'] = (time.perf_counter() - start) * 1000
        result = sorted(result, key=lambda array: -array.shape[2])
        self.last_timings_ms = stages
        return result

    def release(self):
        self.close()

    def close(self):
        with self._context_lock:
            if self.ctx.value:
                context, self.ctx = self.ctx, C.c_uint64()
                self.check(self.lib.rknn_destroy(context), 'destroy')

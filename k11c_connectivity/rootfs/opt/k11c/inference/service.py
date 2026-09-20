#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Small local detection service. No Frigate import, DT alias, or policy changes."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from email import policy
from email.parser import BytesParser
import hmac
from http.server import BaseHTTPRequestHandler, HTTPServer
import io
import json
import math
import os
from pathlib import Path
import socket
import threading
import time
import warnings

import cv2
import numpy as np
from PIL import Image
from upload import fast_fields

MAX_BODY = 2 * 1024 * 1024
MAX_PIXELS = 4096 * 4096
THRESHOLD = 0.4
MAX_RESULTS = 20
SIZE = 416
INFERENCE_WORKERS = 2
# One additional connection slot serves health checks or rejects excess POSTs.
HTTP_WORKERS = INFERENCE_WORKERS + 1
MAX_HTTP_CONNECTIONS = HTTP_WORKERS + 4  # Bounded raw-socket handoff, no decoded-image queue.
# Set once, not via process-global catch_warnings contexts in concurrent workers.
warnings.filterwarnings('error', category=Image.DecompressionBombWarning)
LABELS = ('person,bicycle,car,motorcycle,airplane,bus,train,truck,boat,traffic light,'
          'fire hydrant,stop sign,parking meter,bench,bird,cat,dog,horse,sheep,cow,'
          'elephant,bear,zebra,giraffe,backpack,umbrella,handbag,tie,suitcase,frisbee,'
          'skis,snowboard,sports ball,kite,baseball bat,baseball glove,skateboard,'
          'surfboard,tennis racket,bottle,wine glass,cup,fork,knife,spoon,bowl,'
          'banana,apple,sandwich,orange,broccoli,carrot,hot dog,pizza,donut,cake,'
          'chair,couch,potted plant,bed,dining table,toilet,tv,laptop,mouse,remote,'
          'keyboard,cell phone,microwave,oven,toaster,sink,refrigerator,book,clock,'
          'vase,scissors,teddy bear,hair drier,toothbrush').split(',')


class RequestError(Exception):
    def __init__(self, status, code):
        self.status, self.code = status, code


def parse_upload(content_type, body, key):
    if not content_type.lower().startswith('multipart/form-data;'):
        raise RequestError(415, 'unsupported_media_type')
    if '\r' in content_type or '\n' in content_type:
        raise RequestError(400, 'invalid_content_type')
    fields = fast_fields(content_type, body, MAX_BODY)
    if fields is None:
        # Preserve the existing handling of encoded/legacy/ambiguous MIME.
        message = BytesParser(policy=policy.default).parsebytes(
            ('Content-Type: ' + content_type + '\r\nMIME-Version: 1.0\r\n\r\n').encode() + body)
        if not message.is_multipart() or message.defects:
            raise RequestError(400, 'invalid_multipart')
        fields = {}
        for part in message.iter_parts():
            name = part.get_param('name', header='content-disposition')
            if name not in ('api_key', 'image') or name in fields or part.is_multipart() or part.defects:
                raise RequestError(400, 'invalid_fields')
            fields[name] = part.get_payload(decode=True)
    if set(fields) != {'api_key', 'image'}:
        raise RequestError(400, 'missing_fields')
    if not hmac.compare_digest(fields['api_key'], key.encode()):
        raise RequestError(401, 'unauthorized')
    if not fields['image']:
        raise RequestError(400, 'empty_image')
    return fields['image']


def decode_image(data):
    try:
        with Image.open(io.BytesIO(data)) as image:
            width, height = image.size
            if width < 1 or height < 1 or width * height > MAX_PIXELS or max(width, height) > 4096:
                raise RequestError(413, 'image_too_large')
            if image.format != 'JPEG':
                raise RequestError(415, 'jpeg_required')
            return np.asarray(image.convert('RGB')).copy()
    except RequestError:
        raise
    except Exception as error:
        raise RequestError(400, 'invalid_image') from error


def decode_yolox(outputs):
    """Decode the pinned 416px YOLOX Nano, NCHW [1,85,H,W] at strides 8/16/32."""
    if not isinstance(outputs, (list, tuple)) or len(outputs) != 3:
        raise ValueError('Unexpected model output count')
    candidates = []
    for output, stride in zip(outputs, (8, 16, 32)):
        cells = SIZE // stride
        values = np.asarray(output)
        if values.shape != (1, 85, cells, cells) or not np.isfinite(values).all():
            raise ValueError('Unexpected or non-finite model tensor')
        rows = values[0].transpose(1, 2, 0).reshape(-1, 85)
        labels = np.argmax(rows[:, 5:], axis=1)
        confidence = rows[:, 4] * rows[np.arange(len(rows)), labels + 5]
        chosen = np.flatnonzero(confidence >= THRESHOLD)
        for index in chosen:
            row = rows[index]
            cx = (float(row[0]) + int(index) % cells) * stride
            cy = (float(row[1]) + int(index) // cells) * stride
            w, h = math.exp(float(row[2])) * stride, math.exp(float(row[3])) * stride
            candidates.append((int(labels[index]), float(confidence[index]), cx-w/2, cy-h/2, cx+w/2, cy+h/2))
    if not candidates:
        return []
    boxes = [[r[2], r[3], r[4]-r[2], r[5]-r[3]] for r in candidates]
    # Match the current Frigate YOLOX decoder's class-agnostic NMS threshold.
    keep = cv2.dnn.NMSBoxes(boxes, [r[1] for r in candidates], THRESHOLD, 0.4)
    return [candidates[int(i)] for i in np.asarray(keep).reshape(-1)]


def predictions(rows, width, height):
    result = []
    for label, confidence, x1, y1, x2, y2 in rows:
        if int(label) != label or not 0 <= label < len(LABELS):
            raise ValueError('Invalid label')
        if not all(math.isfinite(v) for v in (confidence, x1, y1, x2, y2)) or not 0 <= confidence <= 1:
            raise ValueError('Invalid numeric detection')
        if confidence < THRESHOLD:
            continue
        x1, x2 = [max(0., min(float(width), v * width / SIZE)) for v in (x1, x2)]
        y1, y2 = [max(0., min(float(height), v * height / SIZE)) for v in (y1, y2)]
        if x2 <= x1 or y2 <= y1:
            continue
        result.append(dict(label=LABELS[int(label)], confidence=float(confidence),
                           x_min=x1, y_min=y1, x_max=x2, y_max=y2))
    result.sort(key=lambda row: row['confidence'], reverse=True)
    return result[:MAX_RESULTS]


class RknnBackend:
    name = 'rknn'

    def __init__(self, model):
        from native import NativeRuntime
        self._request = threading.local()
        self.runtime = NativeRuntime(model)

    @property
    def inference_stages_ms(self):
        return getattr(self._request, 'timings', {})

    @inference_stages_ms.setter
    def inference_stages_ms(self, value):
        self._request.timings = value

    def run(self, rgb):
        self.inference_stages_ms = {}
        start = time.perf_counter()
        tensor = np.ascontiguousarray(cv2.resize(rgb, (SIZE, SIZE))[None, ...], dtype=np.uint8)
        prepare_ms = (time.perf_counter() - start) * 1000
        with self.runtime.borrow_outputs(tensor) as outputs:
            inference_ms = (time.perf_counter() - start) * 1000
            start = time.perf_counter()
            rows = decode_yolox(outputs)
            postprocess_ms = (time.perf_counter() - start) * 1000
        native_stages = getattr(self.runtime, 'last_timings_ms', None)
        if native_stages:
            stages = dict(prepare=prepare_ms, **native_stages)
            stages['other'] = inference_ms - sum(stages.values())
            self.inference_stages_ms = stages
        return rows, inference_ms, postprocess_ms

    def close(self):
        self.runtime.release()


class DetectionServer(HTTPServer):
    request_queue_size = 4

    def __init__(self, address, backend, key):
        if len(key) < 24:
            raise ValueError('API key must have at least 24 characters')
        self.backend, self.key, self.completed = backend, key, 0
        self.inference_slots = threading.BoundedSemaphore(INFERENCE_WORKERS)
        self._connection_slots = threading.BoundedSemaphore(MAX_HTTP_CONNECTIONS)
        self.state_lock = threading.Lock()
        self._connections = set()
        self.closing = threading.Event()
        self._pool = None
        super().__init__(address, DetectionHandler)
        self._pool = ThreadPoolExecutor(max_workers=HTTP_WORKERS, thread_name_prefix='k11c-http')

    def process_request(self, request, client_address):
        # Do not allow the executor's otherwise unbounded submission queue to grow.
        with self.state_lock:
            accepted = not self.closing.is_set() and self._connection_slots.acquire(blocking=False)
            if accepted:
                self._connections.add(request)
                try:
                    self._pool.submit(self._handle_connection, request, client_address)
                except BaseException:
                    self._connections.remove(request)
                    self._connection_slots.release()
                    self.shutdown_request(request)
                    raise
        if not accepted:
            try:
                request.settimeout(.2)
                body = b'{"success":false,"error_code":"server_busy"}'
                request.sendall(b'HTTP/1.0 503 Service Unavailable\r\nContent-Type: application/json\r\n'
                                b'Connection: close\r\nContent-Length: ' + str(len(body)).encode() + b'\r\n\r\n' + body)
            except OSError:
                pass
            finally:
                self.shutdown_request(request)

    def _handle_connection(self, request, client_address):
        try:
            if not self.closing.is_set():
                self.finish_request(request, client_address)
        except (BrokenPipeError, ConnectionResetError):
            pass  # Client disconnect or intentional socket close during shutdown.
        except Exception as error:
            # Never print HTTP request content or credentials from a traceback.
            print('HTTP_ERROR ' + type(error).__name__, flush=True)
        finally:
            self.shutdown_request(request)
            with self.state_lock:
                self._connections.remove(request)
                self._connection_slots.release()

    def server_close(self):
        with self.state_lock:
            self.closing.set()
            super().server_close()
            connections = tuple(self._connections)
        # Interrupt socket reads/writes, but never cancel an in-flight native call.
        for request in connections:
            try:
                request.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        if self._pool is not None:
            self._pool.shutdown(wait=True)


class DetectionHandler(BaseHTTPRequestHandler):
    def setup(self):
        super().setup()
        self.connection.settimeout(5)

    def log_message(self, *args):
        pass  # Do not log keys, URLs supplied by clients, or image bytes.

    def reply(self, code, data):
        body = json.dumps(data, allow_nan=False).encode()
        self.send_response(code)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path != '/health':
            return self.reply(404, dict(success=False, error_code='not_found'))
        with self.server.state_lock:
            completed = self.server.completed
        self.reply(200, dict(ready=True, backend=self.server.backend.name, completed=completed))

    def do_POST(self):
        if not self.server.inference_slots.acquire(blocking=False):
            return self.reply(503, dict(success=False, error_code='server_busy'))
        try:
            response = self._do_POST()
        finally:
            self.server.inference_slots.release()
        # A client can send its next request as soon as it receives the response.
        # Release compute capacity BEFORE publishing that response, not afterwards.
        if response is not None:
            self.reply(*response)

    def _do_POST(self):
        begin, cpu_begin, thread_begin = time.perf_counter(), time.process_time(), time.thread_time()
        try:
            if self.path != '/v1/vision/detection':
                raise RequestError(404, 'not_found')
            if self.headers.get('Transfer-Encoding'):
                raise RequestError(415, 'unsupported_transfer_encoding')
            lengths = self.headers.get_all('Content-Length', [])
            if len(lengths) != 1 or not lengths[0].isdigit():
                raise RequestError(400, 'invalid_content_length')
            length = int(lengths[0])
            if not 0 < length <= MAX_BODY:
                raise RequestError(413, 'payload_too_large')
            start = time.perf_counter()
            body = self.rfile.read(length)
            body_ms = (time.perf_counter() - start) * 1000
            if len(body) != length:
                raise RequestError(400, 'incomplete_body')
            start = time.perf_counter()
            jpeg = parse_upload(self.headers.get('Content-Type', ''), body, self.server.key)
            upload_ms = (time.perf_counter() - start) * 1000
            start = time.perf_counter()
            rgb = decode_image(jpeg)
            decode_ms = (time.perf_counter() - start) * 1000
            if self.server.closing.is_set():
                return
            rows, infer_ms, post_ms = self.server.backend.run(rgb)
            start = time.perf_counter()
            detected = predictions(rows, rgb.shape[1], rgb.shape[0])
            predictions_ms = (time.perf_counter() - start) * 1000
            timings = dict(decode=decode_ms, inference=infer_ms, postprocess=post_ms,
                           total=(time.perf_counter()-begin)*1000,
                           process_cpu=(time.process_time()-cpu_begin)*1000,
                           worker_thread_cpu=(time.thread_time()-thread_begin)*1000)
            request_stages = dict(body_read=body_ms, upload_parse=upload_ms,
                                  predictions=predictions_ms)
            request_stages['other'] = (timings['total'] - decode_ms - infer_ms - post_ms
                                       - sum(request_stages.values()))
            timings['request_stages'] = request_stages
            stages = getattr(self.server.backend, 'inference_stages_ms', None)
            if stages:
                timings['inference_stages'] = stages
            with self.server.state_lock:
                self.server.completed += 1
            return 200, dict(success=True, backend=self.server.backend.name,
                             predictions=detected, timings_ms=timings)
        except RequestError as error:
            return error.status, dict(success=False, error_code=error.code)
        except (BrokenPipeError, ConnectionResetError):
            pass
        except Exception as error:
            print('BACKEND_ERROR ' + type(error).__name__, flush=True)
            return 503, dict(success=False, error_code='inference_failed')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--model', type=Path, required=True)
    parser.add_argument('--port', type=int, default=8099)
    args = parser.parse_args()
    key = os.environ['K11C_API_KEY']
    backend = RknnBackend(args.model)
    server = DetectionServer(('0.0.0.0', args.port), backend, key)
    print('K11C_API_READY backend=rknn model=' + args.model.name, flush=True)
    try:
        server.serve_forever()
    finally:
        server.server_close()
        backend.close()


if __name__ == '__main__':
    main()

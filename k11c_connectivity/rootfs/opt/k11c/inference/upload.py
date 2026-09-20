# SPDX-License-Identifier: AGPL-3.0-or-later
"""Byte-preserving fast path for ordinary Frigate multipart uploads.

Return None outside the fast path; the caller retains its original MIME parser.
This is parsing compatibility, never an inference/CPU fallback. No file I/O.
"""
from email import policy
from email.message import EmailMessage
from email.parser import BytesHeaderParser
import logging

from python_multipart import MultipartParser
from python_multipart.exceptions import FormParserError


class LegacyFormat(Exception):
    pass


def fast_fields(content_type, body, limit):
    outer = EmailMessage(policy=policy.default)
    try:
        outer['Content-Type'] = content_type
        boundary = outer.get_boundary()
        if not boundary or not boundary.isascii() or not 1 <= len(boundary) <= 70:
            return None
        boundary_bytes = boundary.encode('ascii')
        closing = b'\r\n--' + boundary_bytes + b'--'
        if (not body.startswith(b'--' + boundary_bytes + b'\r\n')
                or body.count(closing) != 1
                or not (body.endswith(closing + b'\r\n') or body.endswith(closing))):
            return None
        if not 0 < len(body) <= limit:
            return None
        fields, state = {}, {}
        ended = False

        def begin():
            if len(fields) >= 2:
                raise LegacyFormat()
            state.clear()
            state.update(headers=bytearray(), field=bytearray(), value=bytearray(), data=[])

        def header_field(data, start, end):
            state['field'].extend(data[start:end])

        def header_value(data, start, end):
            state['value'].extend(data[start:end])

        def header_end():
            state['headers'].extend(state['field'] + b': ' + state['value'] + b'\r\n')
            state['field'].clear()
            state['value'].clear()

        def headers_finished():
            part = BytesHeaderParser(policy=policy.default).parsebytes(bytes(state['headers']) + b'\r\n')
            name = part.get_param('name', header='content-disposition')
            if (part.defects or name not in ('api_key', 'image') or name in fields
                    or part.get_content_disposition() != 'form-data'
                    or part.get_content_maintype() in ('multipart', 'message')
                    or part.get('Content-Transfer-Encoding') is not None
                    or any(len(part.get_all(h, [])) > 1 for h in ('Content-Disposition', 'Content-Type'))):
                raise LegacyFormat()
            state['name'] = name

        def data(data, start, end):
            state['data'].append(data[start:end])

        def part_end():
            fields[state['name']] = b''.join(state['data'])

        def end():
            nonlocal ended
            ended = True

        parser = MultipartParser(boundary_bytes, dict(on_part_begin=begin,
            on_header_field=header_field, on_header_value=header_value,
            on_header_end=header_end, on_headers_finished=headers_finished,
            on_part_data=data, on_part_end=part_end, on_end=end),
            max_size=limit, max_header_count=16, max_header_size=8192)
        # Do not emit third-party diagnostics about untrusted header contents.
        parser.logger = logging.Logger('k11c.multipart', level=logging.CRITICAL + 1)
        consumed = parser.write(body)
        parser.finalize()
        # finalize() in the pinned parser is not a completeness check.
        if consumed != len(body) or not ended or set(fields) != {'api_key', 'image'}:
            return None
        return fields
    except (LegacyFormat, FormParserError, UnicodeError, ValueError):
        return None

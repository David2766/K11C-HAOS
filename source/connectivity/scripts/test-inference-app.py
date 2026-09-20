#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Run with the SHIPPED private Python, against installed production files.

Only hardware I/O is faked. HTTP/JPEG/numpy/ctypes loading and entrypoint order
are real. No production test switches or alternate backend are installed.
"""
import ctypes
from contextlib import contextmanager
import hashlib
import http.client
import importlib
import io
import json
import os
from pathlib import Path
import socket
import stat
import struct
import sys
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, '/opt/k11c/inference')
import app
import native
import service
import vendor_profile
import numpy as np
from PIL import Image

KEY = 'fixture-key-never-use-in-deployment-0123456789'


class Backend:
    name = 'fixture'
    def run(self, rgb):
        return [(0, .9, 104, 52, 312, 208)], 1., 1.
    def close(self):
        pass


class Tests(unittest.TestCase):
    def test_private_runtime(self):
        import cv2, PIL, ssl
        self.assertEqual((cv2.__version__, np.__version__, PIL.__version__), ('4.11.0', '1.26.4', '10.4.0'))
        ctypes.CDLL('/usr/lib/librknnrt.so')
        self.assertTrue(sys.executable.startswith('/opt/k11c/npu-runtime/'))
        for path, expected in app.ASSETS.items():
            self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), expected)

    def test_http(self):
        backend = Backend()
        server = service.DetectionServer(('127.0.0.1', 0), backend, KEY)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        buffer = io.BytesIO()
        Image.new('RGB', (832,208), (220,80,20)).save(buffer, 'JPEG')
        def request(key=KEY, image=buffer.getvalue()):
            boundary = 'k11c-test'
            body = (b'--k11c-test\r\nContent-Disposition: form-data; name="api_key"\r\n\r\n' + key.encode() +
                    b'\r\n--k11c-test\r\nContent-Disposition: form-data; name="image"; filename="test.jpg"\r\nContent-Type: image/jpeg\r\n\r\n' +
                    image + b'\r\n--k11c-test--\r\n')
            conn = http.client.HTTPConnection(*server.server_address, timeout=5)
            if len(body) > service.MAX_BODY:
                # The server rejects Content-Length before reading the payload.
                # Avoid a client-side BrokenPipe race while still testing 413.
                conn.putrequest('POST', '/v1/vision/detection')
                conn.putheader('Content-Type','multipart/form-data; boundary='+boundary)
                conn.putheader('Content-Length',str(len(body)))
                conn.endheaders()
            else:
                conn.request('POST', '/v1/vision/detection', body, {'Content-Type':'multipart/form-data; boundary='+boundary})
            response = conn.getresponse()
            import json
            status, value = response.status, json.loads(response.read())
            conn.close()
            return status, value
        try:
            status, value = request()
            self.assertEqual(status, 200)
            self.assertEqual([value['predictions'][0][n] for n in ('x_min','y_min','x_max','y_max')], [208,26,624,104])
            self.assertEqual(request(key='wrong')[0], 401)
            self.assertEqual(request(image=b'invalid')[0], 400)
            self.assertEqual(request(image=b'x'*(service.MAX_BODY+1))[0], 413)
            with patch.object(backend, 'run', side_effect=RuntimeError('fixture')):
                status, value = request()
            self.assertEqual(status, 503)
            self.assertNotIn('predictions', value)
            self.assertEqual(server.completed, 1)
        finally:
            server.shutdown(); thread.join(3); server.server_close()

    def test_rgb_unchanged(self):
        rgb = np.full((208,832,3), (220,80,20), np.uint8)
        class Runtime:
            def __init__(self, model): pass
            @contextmanager
            def borrow_outputs(self, tensor):
                yield self.inference(tensor)
            def inference(self, tensor):
                np.testing.assert_array_equal(tensor[0,0,0], [220,80,20])
                assert tensor.shape == (1,416,416,3) and tensor.dtype == np.uint8
                return [np.zeros((1,85,n,n), np.float32) for n in (52,26,13)]
        with patch.object(native, 'NativeRuntime', Runtime):
            self.assertEqual(service.RknnBackend(Path('unused')).run(rgb)[0], [])

    def test_key(self):
        with tempfile.TemporaryDirectory() as tmp:
            key_file = Path(tmp) / 'key'
            with patch.object(app, 'KEY_FILE', key_file):
                for value in ('', 'short', 'x'*129, 'x'*32+'\nsecret', ' '+KEY):
                    # Leading/trailing whitespace may be stripped; internal whitespace may not.
                    key_file.write_text(value)
                    if value == ' '+KEY:
                        self.assertEqual(app.read_key(), KEY)
                    else:
                        with self.assertRaises(RuntimeError): app.read_key()
                key_file.write_text(KEY)
                self.assertEqual(app.read_key(), KEY)

    def test_bridge(self):
        for address, valid in [('172.30.32.1',True),('0.0.0.0',False),('127.0.0.1',False),('8.8.8.8',False)]:
            response = bytes(20)+socket.inet_aton(address)+bytes(232)
            with patch.object(app.fcntl, 'ioctl', return_value=response) as ioctl:
                if valid:
                    self.assertEqual(app.bridge_address(), address)
                else:
                    with self.assertRaises(RuntimeError): app.bridge_address()
                self.assertEqual(ioctl.call_args.args[2].split(b'\0')[0], b'hassio')

    def devices(self, case):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            def path(p): return root / str(p).lstrip('/')
            def write(p, value):
                file=path(p);file.parent.mkdir(parents=True,exist_ok=True);file.write_text(value);return file
            write('/opt/k11c/modules/6.18.39-haos/npu/bundle.json',
                  Path('/opt/k11c/modules/6.18.39-haos/npu/bundle.json').read_text())
            if case == 'manifest_kernel':
                manifest = path('/opt/k11c/modules/6.18.39-haos/npu/bundle.json')
                data = json.loads(manifest.read_text()); data['kernel_release'] = '6.18.52-haos'
                manifest.write_text(json.dumps(data))
            npu=path('/sys/bus/platform/devices/fde40000.npu'); npu.mkdir(parents=True)
            driver=path('/sys/bus/platform/drivers/RKNPU');driver.mkdir(parents=True)
            (npu/'driver').symlink_to(driver)
            write('/sys/module/rknpu/srcversion', 'OLD' if case=='module' else 'CD8FDCD74670016E17E2BFD')
            write('/sys/class/regulator/regulator.1/name', 'vdd_npu')
            write('/sys/class/regulator/regulator.1/microvolts', '500000' if case=='voltage' else '900000')
            write('/sys/bus/platform/devices/fde40000.npu/devfreq/fde40000.npu/cur_freq', '400000000' if case=='clock' else '198000000')
            target = 600000000 if case.startswith('600_') else (400000000 if case.startswith('400_') else 200000000)
            opp = path('/device-tree/npu-opp-table/opp-%d' % target); opp.mkdir(parents=True)
            (opp/'opp-hz').write_bytes(struct.pack('>Q', target))
            voltage = (1000000, 1000000, 1000000) if target == 600000000 else (900000, 900000, 1000000)
            (opp/'opp-microvolt').write_bytes(struct.pack('>III', *voltage))
            assigned = path('/device-tree/npu@fde40000/assigned-clock-rates'); assigned.parent.mkdir(parents=True)
            assigned.write_bytes(struct.pack('>I', target))
            if target == 400000000:
                hz = {'400_exact':400000000, '400_low':198000000, '400_high':600000000}.get(case,396000000)
                write('/sys/bus/platform/devices/fde40000.npu/devfreq/fde40000.npu/cur_freq', str(hz))
            if case == '400_voltage': write('/sys/class/regulator/regulator.1/microvolts', '950000')
            if case == '400_assigned': assigned.write_bytes(struct.pack('>I', 200000000))
            if case == '400_opp_hz': (opp/'opp-hz').write_bytes(struct.pack('>Q',200000000))
            if case == '400_opp_uv': (opp/'opp-microvolt').write_bytes(struct.pack('>I',900000))
            if case == '400_extra': (opp.parent/'opp-200000000').mkdir()
            if case == '400_unknown': opp.rename(opp.parent/'opp-800000000')
            if target == 600000000:
                hz = {'600_exact':600000000, '600_floor':580000000, '600_low':579999999,
                      '600_high':600000001}.get(case,594000000)
                write('/sys/bus/platform/devices/fde40000.npu/devfreq/fde40000.npu/cur_freq', str(hz))
                uv = {'600_voltage':950000, '600_voltage_edge':999999, '600_high_uv':1000001}.get(case,1000000)
                write('/sys/class/regulator/regulator.1/microvolts', str(uv))
            if case == '600_assigned': assigned.write_bytes(struct.pack('>I',400000000))
            if case == '600_opp_hz': (opp/'opp-hz').write_bytes(struct.pack('>Q',400000000))
            if case == '600_opp_uv': (opp/'opp-microvolt').write_bytes(struct.pack('>III',900000,900000,1000000))
            if case == '600_extra': (opp.parent/'opp-400000000').mkdir()
            if case == '600_unknown': opp.rename(opp.parent/'opp-800000000')
            for name, minor in [('card4',4), ('renderD133',133)]:
                if case=='missing_card' and name=='card4':continue
                entry=write('/sys/class/drm/'+name+'/dev', '226:'+str(minor)).parent
                (entry/'device').symlink_to(npu)
                write('/dev/dri/'+name, '')
            real_stat = Path.stat
            def info(p, *args, **kw):
                if str(p).startswith(str(path('/dev/dri'))):
                    if case=='denied': raise PermissionError('fixture')
                    minor=4 if p.name=='card4' else 133
                    return SimpleNamespace(st_mode=stat.S_IFCHR|0o600,
                                           st_rdev=os.makedev(1 if case=='major' else 226,minor))
                return real_stat(p,*args,**kw)
            identity=SimpleNamespace(release='6.19' if case=='kernel' else '6.18.39-haos')
            if case.startswith('vendor_'):
                import shutil
                shutil.rmtree(path('/device-tree/npu-opp-table'))
                spec=json.loads(vendor_profile.PROFILE.read_text())
                for node,props in spec['nodes'].items():
                    for key,value in props.items():
                        p=path('/device-tree'+node+'/'+key);p.parent.mkdir(parents=True,exist_ok=True)
                        p.write_bytes(bytes.fromhex(value))
                    path('/device-tree'+node+'/name').write_bytes(
                        node.rsplit('/',1)[-1].split('@',1)[0].encode()+b'\0')
                if case=='vendor_bad_name': path('/device-tree/otp@fe38c000/name').write_bytes(b'wrong\0')
                if case=='vendor_extra_property': path('/device-tree/npu-opp-table/unexpected').write_bytes(b'')
                if case=='vendor_extra_node':
                    write('/device-tree/npu-opp-table/opp-1200000000/name','opp-1200000000\0')
                path('/device-tree/compatible').write_bytes(b'kickpi,k11c\0rockchip,rk3566\0')
                state=dict(revision=1,bin=0,pvtm=93910,selector=3,cold=0,target_hz=900000000,
                           clock_hz=900000000,voltage_uv=900000,min_uv=900000,max_uv=1000000,error=0)
                if case=='vendor_idle': state['clock_hz']=198000000
                if case=='vendor_voltage': state['voltage_uv']=500000
                if case=='vendor_clock': state['clock_hz']=600000000
                if case=='vendor_old_request': state.update(target_hz=600000000,clock_hz=600000000)
                if case=='vendor_cold': state.update(cold=1,target_hz=800000000,clock_hz=800000000,voltage_uv=950000,min_uv=950000)
                if case=='vendor_error': state['error']=-5
                if case=='vendor_selector': state['selector']=0
                if case=='vendor_blank': state['pvtm']=0
                write('/sys/bus/platform/devices/fde40000.npu/vendor_policy',' '.join('%s=%d'%p for p in state.items()))
                write('/sys/bus/platform/devices/fde40000.npu/devfreq/fde40000.npu/available_frequencies',
                      '200000000 297000000 400000000 600000000 700000000 800000000 900000000')
                write('/sys/module/k11c_rk3568_otp/version','r22.1')
                write('/sys/module/k11c_rk3568_otp/srcversion','17ADBE45E3464BE801709BF')
                d=path('/sys/bus/platform/drivers/k11c-rk3568-otp');d.mkdir(parents=True)
                (d/'module').symlink_to(path('/sys/module/k11c_rk3568_otp'))
                o=path('/sys/bus/platform/devices/fe38c000.otp');o.mkdir(parents=True)
                (o/'driver').symlink_to(d)
                if case=='vendor_dt': path('/device-tree/npu-opp-table/opp-600000000/opp-hz').write_bytes(bytes(8))
            with patch.object(app,'Path',side_effect=path), patch.object(app,'NPU',npu), \
                 patch.object(vendor_profile,'Path',side_effect=path),patch.object(vendor_profile,'NPU',npu), \
                 patch.object(vendor_profile,'DT',path('/device-tree')), \
                 patch.object(app.os,'uname',return_value=identity), patch.object(Path,'stat',info):
                return app.verify_devices()

    def test_devices(self):
        nodes=self.devices('good')
        self.assertTrue(nodes['card'].endswith('card4'))
        self.assertTrue(nodes['render'].endswith('renderD133'))
        self.assertEqual(set(self.devices('400_good')), {'card','render'})
        self.assertEqual(set(self.devices('400_exact')), {'card','render'})
        for case in ('600_good','600_exact','600_floor'):
            self.assertEqual(set(self.devices(case)), {'card','render'})
        for case in ('vendor_good','vendor_idle','vendor_cold'):
            self.assertEqual(set(self.devices(case)), {'card','render'})
        for case in ('kernel','manifest_kernel','module','voltage','clock','missing_card','major','denied',
                     '400_low','400_high','400_voltage','400_assigned','400_opp_hz',
                     '400_opp_uv','400_extra','400_unknown',
                     '600_low','600_high','600_voltage','600_voltage_edge','600_high_uv',
                     '600_assigned','600_opp_hz','600_opp_uv','600_extra','600_unknown',
                     'vendor_voltage','vendor_clock','vendor_old_request','vendor_error','vendor_selector','vendor_blank','vendor_dt',
                     'vendor_bad_name','vendor_extra_property','vendor_extra_node'):
            with self.subTest(case=case), self.assertRaises((RuntimeError,PermissionError)):
                self.devices(case)

    def test_start_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            state=Path(tmp)/'url';ready=Path(tmp)/'ready';model=Path(tmp)/'model'
            model.write_bytes(b'fixture')
            assets={model:hashlib.sha256(b'fixture').hexdigest()}
            events=[]
            class Runtime(Backend):
                def __init__(self, model): events.append('model')
                def close(self):events.append('release')
            class Server:
                def __init__(self, address, backend, key):
                    assert address==('172.30.32.1',8099) and key==KEY
                    events.append('listen')
                def serve_forever(self):
                    assert state.read_text().startswith('http://172.30.32.1:8099/')
                    events.append('serve')
                def server_close(self):events.append('close')
            with patch.object(app,'read_key',return_value=KEY), patch.object(app,'READY',ready), \
                 patch.object(app,'STATE',state),patch.object(app,'ASSETS',assets), \
                 patch.object(app.time,'sleep'),patch.object(app,'bridge_address',return_value='172.30.32.1'), \
                 patch.object(app,'verify_devices',return_value={'card':'card4','render':'renderD133'}) as devices, \
                 patch.object(service,'RknnBackend',Runtime),patch.object(service,'DetectionServer',Server):
                with self.assertRaises(RuntimeError):app.start()
                devices.assert_not_called();self.assertEqual(events,[])
                ready.touch();model.write_bytes(b'corrupt')
                with self.assertRaises(RuntimeError):app.start()
                devices.assert_not_called();self.assertEqual(events,[])
                model.write_bytes(b'fixture');app.start()
                self.assertEqual(events,['model','listen','serve','close','release'])
                self.assertFalse(state.exists())
                events.clear()
                with patch.object(Server,'serve_forever',side_effect=SystemExit(0)):
                    with self.assertRaises(SystemExit):app.start()
                self.assertEqual(events,['model','listen','close','release'])
                self.assertFalse(state.exists());events.clear()
                with patch.object(service,'DetectionServer',side_effect=OSError('port busy')):
                    with self.assertRaises(OSError):app.start()
                self.assertEqual(events,['model','release'])
                self.assertFalse(state.exists());events.clear()
                with patch.object(service,'RknnBackend',side_effect=RuntimeError('fixture')):
                    with self.assertRaises(RuntimeError):app.start()
                self.assertEqual(events,[]);self.assertFalse(state.exists())


def main():
    result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(Tests))
    if not result.wasSuccessful(): raise SystemExit(1)
    mutations=[
        (app,'        validate_runtime()','        pass','test_devices'),
        (app,'        validate_dt()','        pass','test_devices'),
        (vendor_profile,"state['error'] == 0",'True','test_devices'),
        (vendor_profile,"state['min_uv'] <= state['voltage_uv']","0 <= state['voltage_uv']",'test_devices'),
        (vendor_profile,"state['selector'] == selector",'True','test_devices'),
        (vendor_profile,'hz <= 900000000','hz <= 600000000','test_devices'),
        (vendor_profile,'target == normal_target','True','test_devices'),
        (vendor_profile,"bundle['kernel_release'] == kernel",'True','test_devices'),
        (app,"set(nodes) == {'card', 'render'}","'render' in nodes",'test_devices'),
        (app,'900000 <= uv <= 1000000','0 <= uv <= 1000000','test_devices'),
        (app,'low <= hz <= high','0 <= hz <= 999999999','test_devices'),
        (app,'len(entries) == 1','len(entries) >= 1','test_devices'),
        (app,'target != 400000000 or uv == 900000','True','test_devices'),
        (app,"== struct.pack('>Q', target)",'!= b"impossible"','test_devices'),
        (app,"== struct.pack('>III', *voltage)",'!= b"impossible"','test_devices'),
        (app,"== struct.pack('>I', target)",'!= b"impossible"','test_devices'),
        (app,'target != 600000000 or uv == 1000000','True','test_devices'),
        (app,'target != 600000000 or uv == 1000000','target == 600000000 or uv == 1000000','test_devices'),
        (app,'hashlib.sha256(path.read_bytes()).hexdigest() == expected','True','test_start_order'),
        (app,"require(READY.is_file(), 'NPU loader","require(True, 'NPU loader",'test_start_order'),
        (app,"bool(re.fullmatch(r'[A-Za-z0-9_-]{32,128}', key))",'True','test_key'),
        (service,"if not hmac.compare_digest(fields['api_key'], key.encode()):",'if False:','test_http'),
    ]
    for module,old,new,test in mutations:
        original=module.__dict__.copy();text=Path(module.__file__).read_text()
        assert old in text,old
        exec(compile(text.replace(old,new,1),module.__file__,'exec'),module.__dict__)
        result=unittest.TestResult();Tests(test).run(result)
        module.__dict__.update(original)
        assert not result.wasSuccessful(),'Missed mutation: '+old
        print('MUTATION_DETECTED '+old)
    print('APP_API_LOCAL_PASS tests=7 mutations=%d hardware=NOT_TESTED' % len(mutations))


if __name__=='__main__':main()

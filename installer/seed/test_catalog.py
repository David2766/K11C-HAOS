"""Local compatibility publication: real state guards and guard mutations."""
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
import catalog

class CatalogTest(unittest.TestCase):
    def test_verified_profile_required(self):
        with tempfile.TemporaryDirectory() as temp:
            config = Path(temp)/'config.yaml'
            config.write_bytes(b'input fixture')
            slug = '157e89e9_k11c_connectivity'
            apps = {'system': {slug: dict(version='0.5.6', slug='k11c_connectivity', repository='157e89e9',
                                         startup='system', arch=['aarch64'], image='ghcr.io/david2766/k11c-haos-connectivity')},
                    'user': {slug: dict(version='0.5.6', image='ghcr.io/david2766/k11c-haos-connectivity',
                                      options={'enabled': True}, boot='auto', protected=False)}}
            seed = dict(schema=1, board='kickpi,k11c', arch='aarch64', haos='18.3', kernel='6.18.52-haos',
                        official_raw_sha256='a'*64, catalog_commit='b'*40, app_version='0.5.6',
                        image_digest='manifest', image_id='config', supervisor='2026.09.2', docker='29.7.2',
                        factory_data_directory_verified=True, supervisor_options_write_verified=True)
            for key in ('factory_data_directory_verified', 'supervisor_options_write_verified'):
                for value in (None, False):
                    bad = dict(seed)
                    if value is None: del bad[key]
                    else: bad[key] = value
                    with self.assertRaises(AssertionError): catalog.profile(bad, apps, config, '2.3.4')
            result = catalog.profile(seed, apps, config, '2.3.4')
            self.assertNotIn('asset', result)
            self.assertNotIn('data_bytes', result)
            self.assertTrue(result['locally_verified'])
            with self.assertRaises(AssertionError): catalog.profile(seed, apps, config, '1.7')
            apps['user'][slug]['options']['enabled'] = False
            with self.assertRaises(AssertionError): catalog.profile(seed, apps, config, '2.3.4')

if __name__ == '__main__':
    if sys.argv[1:] == ['--mutation']:
        source = Path(catalog.__file__).read_text()
        original = catalog.profile
        for key in ('factory_data_directory_verified', 'supervisor_options_write_verified'):
            for replacement in ('True', "seed.get('"+key+"') is not True"):
                condition = "seed.get('"+key+"') is True"
                assert source.count(condition) == 1
                namespace = {'__name__': 'mutated_catalog'}
                exec(compile(source.replace(condition, replacement), catalog.__file__, 'exec'), namespace)
                try:
                    catalog.profile = namespace['profile']
                    result = unittest.TextTestRunner(stream=io.StringIO()).run(
                        unittest.defaultTestLoader.loadTestsFromTestCase(CatalogTest))
                    assert not result.wasSuccessful(), 'Profile guard mutation survived'
                finally: catalog.profile = original
                print('MUTATION_REJECTED='+key+':'+replacement)
    else: unittest.main()

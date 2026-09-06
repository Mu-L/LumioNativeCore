import copy
from pathlib import Path
import tempfile
import subprocess
import unittest
from check_repository import validate, REGISTRY


def package(name, dependencies=(), types=('lib',)):
    return {'id': name, 'name': name, 'manifest_path': f'/repo/{name}/Cargo.toml',
            'targets': [{'crate_types': list(types)}], 'dependencies': list(dependencies)}


def metadata(*packages):
    return {'workspace_members': [p['id'] for p in packages], 'packages': list(packages)}


def dep(name, path=None, kind=None, **extra):
    return {'name': name, 'path': path, 'kind': kind, 'source': None if path else REGISTRY,
            'req': '*', **extra}


class PolicyTests(unittest.TestCase):
    def test_unknown_member_is_not_omitted(self):
        self.assertTrue(validate(metadata(package('unlisted'))))

    def test_reverse_optional_target_dependency_is_rejected(self):
        data = metadata(package('lumio-kernel', [dep('lumio-job', '/repo/lumio-job', optional=True, target='cfg(windows)')]), package('lumio-job'))
        self.assertTrue(validate(data))

    def test_build_dependency_is_checked(self):
        self.assertTrue(validate(metadata(package('lumio-kernel', [dep('vendor', kind='build')]))))

    def test_native_artifact_is_detected(self):
        self.assertTrue(validate(metadata(package('lumio-platform', types=('cdylib',))), 'artifacts'))

    def test_allowed_edge_and_renaming(self):
        data = metadata(package('lumio-platform'), package('lumio-kernel', [dep('lumio-platform', '/repo/lumio-platform', rename='clock')]))
        self.assertEqual(validate(data), [])

    def test_supplier_requires_exact_version_and_registry(self):
        valid = metadata(package('lumio-spatial', [dep('rstar', req='=0.12.2')]))
        self.assertEqual(validate(valid), [])
        bad = copy.deepcopy(valid)
        bad['packages'][0]['dependencies'][0]['source'] = 'git+https://example.invalid/replacement'
        self.assertTrue(validate(bad))

    def test_external_cannot_impersonate_workspace_name(self):
        self.assertTrue(validate(metadata(package('lumio-kernel', [dep('lumio-platform')]), package('lumio-platform'))))

    def test_cargo_parses_single_quotes_and_member_glob(self):
        # Real parser fixture, not a second hand-written TOML parser.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Cargo.toml').write_text("[workspace]\nmembers=['crates/*']\nresolver='2'\n")
            crate = root / 'crates' / 'bad'
            (crate / 'src').mkdir(parents=True)
            (crate / 'src' / 'lib.rs').write_text('pub fn value() -> u32 { 1 }\n')
            (crate / 'Cargo.toml').write_text("[package]\nname='lumio-platform'\nversion='0.0.0'\n[lib]\ncrate-type=['cdylib']\n")
            import json
            result = subprocess.run(['cargo', 'metadata', '--format-version', '1', '--no-deps'], cwd=root, check=True, text=True, capture_output=True)
            self.assertTrue(validate(json.loads(result.stdout), 'artifacts'))


if __name__ == '__main__':
    unittest.main()

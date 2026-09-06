#!/usr/bin/env python3
"""Validate Cargo's parsed workspace, not hand-parsed TOML or a partial tree."""
import argparse
import json
from pathlib import Path
import subprocess
import sys

ALLOWED = {
    'lumio-platform': set(),
    'lumio-kernel': {'lumio-platform'},
    'lumio-job': {'lumio-kernel', 'lumio-platform'},
    'lumio-spatial': {'lumio-kernel'},
    'lumio-timer': set(),
    'lumio-codec': {'lumio-kernel'},
    'lumio-diagnostics': {'lumio-kernel', 'lumio-platform'},
    'lumio-test-support': {'lumio-kernel', 'lumio-job', 'lumio-platform'},
    'xtask': set(),
}
# Reviewed direct suppliers. Versions are exact; transitive graph is in Cargo.lock.
EXTERNAL = {'lumio-spatial': {'rstar': '=0.12.2'}}
REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'


def validate(metadata, mode='all'):
    violations = []
    ids = set(metadata['workspace_members'])
    packages = [p for p in metadata['packages'] if p['id'] in ids]
    if len(packages) != len(ids) or not packages:
        return ['incomplete or empty workspace metadata']
    paths = {str(Path(p['manifest_path']).parent.resolve()): p['name'] for p in packages}
    graph = {p['name']: set() for p in packages}
    for package in packages:
        name = package['name']
        if mode in ('all', 'artifacts'):
            for target in package['targets']:
                forbidden = {'cdylib', 'staticlib'}.intersection(target['crate_types'])
                if forbidden:
                    violations.append(f'{name}: forbidden crate types {sorted(forbidden)}')
        if mode not in ('all', 'dag'):
            continue
        if name not in ALLOWED:
            violations.append(f'unregistered workspace crate: {name}')
        for dependency in package['dependencies']:
            if dependency['kind'] == 'dev':
                continue
            # Includes optional, target-specific and build dependencies. Cargo
            # metadata exposes declarations even when a feature is disabled.
            path = dependency.get('path')
            internal = paths.get(str(Path(path).resolve())) if path else None
            if internal:
                graph[name].add(internal)
                if internal not in ALLOWED.get(name, set()):
                    violations.append(f'forbidden dependency: {name} -> {internal}')
            else:
                approved = EXTERNAL.get(name, {}).get(dependency['name'])
                if path or dependency.get('source') != REGISTRY or approved != dependency['req']:
                    violations.append(f'unapproved supplier: {name} -> {dependency["name"]} {dependency["req"]}')
    visiting, visited = set(), set()

    def walk(name):
        if name in visiting:
            violations.append(f'dependency cycle at {name}')
            return
        if name in visited:
            return
        visiting.add(name)
        for child in graph[name]:
            walk(child)
        visiting.remove(name)
        visited.add(name)

    if mode in ('all', 'dag'):
        for name in graph:
            walk(name)
    return violations


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('mode', choices=('all', 'dag', 'artifacts'), default='all', nargs='?')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    try:
        result = subprocess.run(['cargo', 'metadata', '--format-version', '1', '--no-deps', '--all-features'],
                                cwd=root, check=True, text=True, capture_output=True)
        errors = validate(json.loads(result.stdout), args.mode)
    except (OSError, subprocess.CalledProcessError, KeyError, ValueError) as error:
        print(f'metadata unavailable: {error}', file=sys.stderr)
        return 2
    for error in errors:
        print(f'FAIL {error}', file=sys.stderr)
    if errors:
        return 1
    print(f'repository {args.mode} checks passed (actual Cargo workspace, all declarations)')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())

#!/usr/bin/env python3
"""Validate Compose contracts without reading production dotenv files."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import yaml

ROOT = Path(__file__).resolve().parents[2]


def main():
    with tempfile.TemporaryDirectory(prefix='fedi3-compose-test-') as name:
        stage = Path(name)
        files = ['docker-compose.yml', 'deploy/turn/docker-compose.yml',
                 'deploy/p2p_infra/docker-compose.yml']
        for file in files:
            target = stage / file
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / file, target)
        values = (ROOT / '.env.example').read_text()
        values += '\nFEDI3_DOMAIN=relay.example.invalid\nTURN_EXTERNAL_IP=198.51.100.1\n'
        (stage / '.env').write_text(values)
        env = {k: v for k, v in os.environ.items()
               if not k.startswith(('FEDI3_', 'TURN_', 'P2P_', 'POSTGRES_', 'MEILI_'))}

        def config(file):
            # Explicit test dotenv + isolated project dir: no production secrets.
            result = subprocess.run(['docker', 'compose', '--project-directory', str(stage),
                                     '--env-file', str(stage / '.env'), '-f', str(stage / file),
                                     'config', '--format', 'json'],
                                    env=env, capture_output=True, text=True, check=True)
            return json.loads(result.stdout)['services']

        root = config(files[0])
        relay = root['relay']['environment']
        pg = root['postgres']['environment']
        assert relay['FEDI3_RELAY_DB_URL'] == ''
        assert relay['FEDI3_RELAY_PG_PASSWORD'] == pg['POSTGRES_PASSWORD']
        assert relay['FEDI3_RELAY_PG_USER'] == pg['POSTGRES_USER']
        assert relay['FEDI3_RELAY_PG_DB'] == pg['POSTGRES_DB']
        turn = config(files[1])['coturn']
        for service in (root['turn'], turn):
            assert not service['image'].endswith(':latest')
            assert set(service['environment']) == {'TURN_PORT'}
            assert str(service['mem_limit']) not in ('0', '') and service['healthcheck']['test']
            assert '--external-ip=198.51.100.1' in service['command']
        p2p = config(files[2])['fedi3_p2p_infra']
        for key in ['FEDI3_P2P_EXTERNAL_ADDR', 'FEDI3_P2P_PEER_ID_FILE',
                    'FEDI3_P2P_RELAY_MAX_CIRCUITS', 'FEDI3_P2P_RELAY_MAX_CIRCUITS_PER_PEER',
                    'FEDI3_P2P_RELAY_MAX_CIRCUIT_BYTES', 'FEDI3_P2P_RELAY_MAX_CIRCUIT_SECS']:
            assert root['p2p_infra']['environment'][key] == p2p['environment'][key]
        assert p2p['healthcheck']['test'][-1] == '--healthcheck'
        assert p2p['stop_grace_period'] in ('15s', 15_000_000_000)

    class UniqueLoader(yaml.SafeLoader):
        pass

    def unique_mapping(loader, node, deep=False):
        result = {}
        for key_node, value_node in node.value:
            key = loader.construct_object(key_node, deep=deep)
            if key in result:
                raise ValueError('Duplicate YAML key: ' + str(key))
            result[key] = loader.construct_object(value_node, deep=deep)
        return result

    UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, unique_mapping)
    spec = yaml.load((ROOT / 'docs/openapi.yaml').read_text(), Loader=UniqueLoader)
    normalized = [re.sub(r'\{[^}]+\}', '{}', path) for path in spec['paths']]
    assert len(normalized) == len(set(normalized)), 'Ambiguous OpenAPI path templates'

    def check_refs(value):
        if isinstance(value, dict):
            ref = value.get('$ref')
            if ref and ref.startswith('#/'):
                target = spec
                for part in ref[2:].split('/'):
                    target = target[part.replace('~1', '/').replace('~0', '~')]
            for child in value.values():
                check_refs(child)
        elif isinstance(value, list):
            for child in value:
                check_refs(child)

    check_refs(spec)
    print('Compose credentials/health/limits and OpenAPI contracts passed')


if __name__ == '__main__':
    main()

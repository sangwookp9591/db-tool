"""Summarize accepted native resource samples; leak points remain per-run evidence."""
import argparse
import json
import math
from pathlib import Path
import statistics
from resource_opt import slope


def stats(values):
    values = sorted(v for v in values if v is not None)
    if not values:
        return {'n': 0, 'p50': None, 'p95': None, 'min': None, 'max': None}
    assert all(math.isfinite(v) for v in values), 'Non-finite resource sample'
    return dict(n=len(values), p50=statistics.median(values), p95=values[math.ceil(.95 * len(values)) - 1], min=values[0], max=values[-1])


def summarize(path):
    raw = json.loads(path.read_text())
    groups = {}
    leaks = []
    excluded = []
    for run in raw['runs']:
        identity = dict(artifact=run['artifact'], workers=run['workers'])
        for sample in run['samples']:
            if not sample['accepted']:
                excluded.append(dict(**identity, repeat=run['repeat'], stage=sample['stage'], iteration=sample['iteration']))
                continue
            if sample['iteration']:
                continue
            processes = sample['processes']
            entries = list(processes)
            for role in ('total', 'app', 'drivers'):
                selected = [p for p in processes if role == 'total' or p['role'].startswith('driver-') == (role == 'drivers')]
                entries.append(dict(role=role, rss_kib=sum(p['rss_kib'] for p in selected),
                    phys_footprint_bytes=sum(p['phys_footprint_bytes'] for p in selected),
                    cpu_interval_percent=sum(p['cpu_interval_percent'] for p in selected) if all(p['cpu_interval_percent'] is not None for p in selected) else None,
                    idle_interval_percent=sum(p['idle_interval_percent'] for p in selected) if all(p.get('idle_interval_percent') is not None for p in selected) else None))
            for process in entries:
                key = (run['artifact'], run['workers'], sample['stage'], process['role'])
                group = groups.setdefault(key, dict(rss_mib=[], footprint_mib=[], cpu_percent=[]))
                group['rss_mib'].append(process['rss_kib'] / 1024)
                group['footprint_mib'].append(process['phys_footprint_bytes'] / 1048576)
                group['cpu_percent'].append(process.get('idle_interval_percent') if sample['stage'] == 'idle-60' else process.get('cpu_interval_percent'))
        for stage in ('repeat-query', 'repeat-tabs'):
            for role in ('total', 'host', 'WebContent', 'Networking', 'GPU', 'driver-postgres', 'driver-neo4j'):
                points = []
                for sample in run['samples']:
                    if sample['stage'] != stage or not sample['accepted']:
                        continue
                    processes = [p for p in sample['processes'] if role == 'total' or p['role'] == role]
                    if processes:
                        points.append((sample['iteration'], sum(p['rss_kib'] for p in processes) / 1024))
                if points:
                    leaks.append(dict(**identity, repeat=run['repeat'], stage=stage, role=role, points=points, slope_mib_per_cycle=slope(points), warm_slope_mib_per_cycle=slope([(x, y) for x, y in points if x >= 15])))
    rows = [dict(artifact=key[0], workers=key[1], stage=key[2], role=key[3], **{name: stats(values) for name, values in group.items()}) for key, group in groups.items()]
    return dict(source=str(path), variant=raw['variant'], artifacts=raw['artifacts'], rows=rows, leaks=leaks, excluded=excluded)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('inputs', nargs='*', type=Path)
    parser.add_argument('--self-check', action='store_true')
    args = parser.parse_args()
    if args.self_check:
        assert stats([5, 1, None, 3, 2, 4]) == dict(n=5, p50=3, p95=5, min=1, max=5)
        assert stats([])['n'] == 0 and stats([])['p50'] is None
        import tempfile
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'sample.json'
            processes = [dict(role=role, rss_kib=size, phys_footprint_bytes=size * 1024, cpu_interval_percent=0, idle_interval_percent=.1)
                         for role, size in [('host', 1024), ('driver-postgres', 2048), ('driver-neo4j', 3072)]]
            path.write_text(json.dumps(dict(variant='check', artifacts=[], runs=[dict(artifact=0, workers='default', repeat=1,
                samples=[dict(accepted=True, iteration=0, stage='idle-60', processes=processes)])])))
            rows = {row['role']: row for row in summarize(path)['rows']}
            assert [rows[role]['rss_mib']['p50'] for role in ('app', 'drivers', 'total')] == [1, 5, 6]
            assert rows['total']['cpu_percent']['p50'] == .1 * 3
        print('resource_report self-check passed')
    else:
        assert args.inputs, 'At least one resource result is required'
        print(json.dumps([summarize(path) for path in args.inputs], indent=2))

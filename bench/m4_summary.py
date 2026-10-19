#!/usr/bin/env python3
"""Summarize immutable M4 measurements without changing their acceptance flags."""
import json
from pathlib import Path
import sys


def timing_valid(points, stage):
    if stage not in ('recycled-10', 'activated'):
        return True
    if 'recycled-ready' not in points or 'recycled-10' not in points:
        return False
    elapsed = points['recycled-10']['at_seconds']-points['recycled-ready']['at_seconds']
    return 8 <= elapsed <= 15


def summarize(runs):
    rows = []
    pairs = []
    controls = [i for i, run in enumerate(runs) if run['mode'] == 'control' and run.get('status') == 'complete']
    for i, run in enumerate(runs):
        points = {p['stage']: p for p in run['samples']}
        row = dict(index=i, mode=run['mode'], status=run.get('status'),
                   footprint_mib={name: p['app_mib'] for name, p in points.items()},
                   bridge_requests=len(run['bridge']), bridge_failed=sum(not p['ok'] for p in run['bridge']),
                   exit_verified=run.get('exit_verified'),
                   activation_2raf_ms=run.get('ui', {}).get('activation_2raf_ms'),
                   restoration=run.get('ui', {}).get('restoration'))
        if 'recycled-10' in points:
            row['ten_second_stage_valid'] = timing_valid(points, 'recycled-10')
            row['post_ready_wait_seconds'] = points['recycled-10']['at_seconds']-points['recycled-ready']['at_seconds']
        if 'recycled-ready' in points:
            ready = points['recycled-ready']
            row['ready_delta_mib'] = {name: ready['app_mib']-points[name]['app_mib'] for name in ('startup', 'idle-10', 'idle-60')}
            row['ready_point_plus_50'] = {name: ready['accepted'] and points[name]['accepted'] and delta <= 50 for name, delta in row['ready_delta_mib'].items()}
        row['full_scenario_valid'] = run.get('status') == 'complete' and all(p['accepted'] for p in points.values()) and row.get('ten_second_stage_valid', False)
        if 'recycled-10' in points and 'startup' in points:
            after = points['recycled-10']
            row['delta_mib'] = {name: after['app_mib']-points[name]['app_mib'] for name in ('startup', 'idle-10', 'idle-60')}
            row['plus_50_valid'] = {name: row['ten_second_stage_valid'] and after['accepted'] and points[name]['accepted'] and delta <= 50 for name, delta in row['delta_mib'].items()}
            row['webcontent_pid_changed'] = ([p['pid'] for p in points['startup']['processes'] if p['role'] == 'WebContent'] != [p['pid'] for p in after['processes'] if p['role'] == 'WebContent'])
        rows.append(row)
        if run['mode'] != 'control' and controls:
            control = min(controls, key=lambda n: abs(n-i))
            reference = {p['stage']: p for p in runs[control]['samples']}
            comparisons = []
            for stage, point in points.items():
                if stage not in reference:
                    continue
                before = reference[stage]
                load_delta = abs(point['load_before']-before['load_before'])
                comparisons.append(dict(stage=stage, load_delta=load_delta,
                    accepted=point['accepted'] and before['accepted'] and load_delta <= 3 and timing_valid(points, stage) and timing_valid(reference, stage),
                    control_mib=before['app_mib'], candidate_mib=point['app_mib'], delta_mib=point['app_mib']-before['app_mib']))
            pairs.append(dict(control=control, candidate=i, points=comparisons))
    return dict(runs=rows, pairs=pairs)


if __name__ == '__main__':
    if sys.argv[1:] == ['--self-check']:
        def run(mode, load, ok=True):
            return dict(mode=mode, status='complete', bridge=[], samples=[dict(stage='startup', load_before=load, accepted=ok, app_mib=100)])
        assert summarize([run('control', 8), run('reload', 11)])['pairs'][0]['points'][0]['accepted']
        assert not summarize([run('control', 8), run('reload', 12)])['pairs'][0]['points'][0]['accepted']
        assert not summarize([run('control', 8, False), run('replace', 8)])['pairs'][0]['points'][0]['accepted']
        assert timing_valid({'recycled-ready': {'at_seconds': 10}, 'recycled-10': {'at_seconds': 22}}, 'activated')
        assert not timing_valid({'recycled-ready': {'at_seconds': 10}, 'recycled-10': {'at_seconds': 114}}, 'recycled-10')
        print('M4 pair-gate self-check passed')
    else:
        source = Path(sys.argv[1])
        print(json.dumps(summarize(json.loads(source.read_text())['runs']), indent=2))

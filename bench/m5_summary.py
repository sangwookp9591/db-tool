#!/usr/bin/env python3
"""Score M5 only at activation; report hidden timer/IPC delays separately."""
import argparse
import json
from pathlib import Path


def summarize(raw):
    rows = []
    for run in raw['runs']:
        points = {p['stage']:p for p in run['samples']}
        ui = run.get('ui', {})
        startup, activated = points.get('startup'), points.get('activated')
        idle10, closed, ready = points.get('idle-10'), points.get('tabs-closed-10'), points.get('recycled-ready')
        closed_at = closed.get('ui',{}).get('at') if closed else None
        ready_at = ready.get('ui',{}).get('at') if ready else None
        delta = activated['app_mib']-startup['app_mib'] if activated and startup else None
        restoration = ui.get('restoration', {})
        rows.append(dict(mode=run['mode'], completed=run.get('status')=='complete', exit_verified=run.get('exit_verified', False),
                         startup_mib=startup['app_mib'] if startup else None,
                         activated_mib=activated['app_mib'] if activated else None, delta_mib=delta,
                         idle10_mib=idle10['app_mib'] if idle10 else None,
                         delta_from_idle10_mib=activated['app_mib']-idle10['app_mib'] if activated and idle10 else None,
                         close_checkpoint_to_ready_ms=ready_at-closed_at if isinstance(ready_at,(int,float)) and isinstance(closed_at,(int,float)) else None,
                         activation_2raf_ms=ui.get('activation_2raf_ms'), host_timer_ms=ui.get('host_timer_ms'),
                         hidden_wait_ms=ui.get('hidden_wait_ms'), ipc_delivery_ms=ui.get('ipc_delivery_ms'),
                         preserved=restoration.get('sessions') is True and restoration.get('snapshot') is True,
                         accepted=bool(startup and activated and startup['accepted'] and activated['accepted']),
                         memory_pass=delta is not None and delta <= 50,
                         paint_pass=isinstance(ui.get('activation_2raf_ms'), (int,float)) and 0 <= ui['activation_2raf_ms'] <= 300))
    pairs = []
    for i in range(0,len(rows)-1,2):
        pair = rows[i:i+2]
        assert {row['mode'] for row in pair} == {'control','replace'}
        replace = next(row for row in pair if row['mode']=='replace')
        differences = {}
        points = [{p['stage']:p for p in run['samples']} for run in raw['runs'][i:i+2]]
        for stage in ['startup','tabs-closed-10','activated']:
            if all(stage in p for p in points):
                differences[stage] = abs(points[0][stage]['load_before']-points[1][stage]['load_before'])
        valid = len(differences)==3 and max(differences.values())<=3 and all(row['completed'] and row['accepted'] and row['exit_verified'] for row in pair)
        pairs.append(dict(pair=i//2+1, observations=pair, load_differences=differences, valid=valid,
                          passes=valid and replace['memory_pass'] and replace['paint_pass'] and replace['preserved']))
    return dict(score_eligible=raw['score_eligible'], product_policy=raw.get('product_policy',False),
                pairs=pairs, runs=rows, attempted_pairs=len(pairs),
                complete_pairs=sum(all(row['completed'] for row in pair['observations']) for pair in pairs),
                valid_pairs=sum(pair['valid'] for pair in pairs),
                adopted=raw['score_eligible'] and raw.get('product_policy',False) and sum(pair['valid'] for pair in pairs)>=5 and all(pair['passes'] for pair in pairs if pair['valid']))


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input',nargs='?')
    parser.add_argument('--self-check',action='store_true')
    args=parser.parse_args()
    if args.self_check:
        def run(mode,delta=50,paint=300,delay=177267):
            stages=['startup','tabs-closed-10','activated']
            return dict(mode=mode,status='complete',exit_verified=True,
                        samples=[dict(stage=stage,app_mib=100+(delta if stage=='activated' else 0),accepted=True,load_before=5) for stage in stages],
                        ui=dict(activation_2raf_ms=paint,hidden_wait_ms=delay,host_timer_ms=10000,ipc_delivery_ms=delay-10000,restoration=dict(sessions=True,snapshot=True)))
        raw=dict(score_eligible=True,product_policy=True,runs=[run(mode) for _ in range(5) for mode in ['control','replace']])
        assert summarize(raw)['adopted'], 'Hidden wait must not be a scoring gate'
        extra=[run('control'),run('replace')]; extra[1]['samples'][0]['load_before']=9
        expanded={**raw,'runs':raw['runs']+extra}
        assert summarize(expanded)['valid_pairs']==5 and summarize(expanded)['adopted']
        expanded['runs'][1]=run('replace',50.01)
        assert not summarize(expanded)['adopted'], 'An invalid attempt cannot erase a valid failed metric'
        raw['runs'][1]=run('replace',50.01)
        assert not summarize(raw)['adopted']
        raw['runs'][1]=run('replace',50,301)
        assert not summarize(raw)['adopted']
        raw['runs'][1]=run('replace');raw['runs'][1]['ui']['restoration']['snapshot']=False
        assert not summarize(raw)['adopted']
        print('M5 boundary scoring self-check passed')
    else:
        assert args.input
        print(json.dumps(summarize(json.loads(Path(args.input).read_text())),indent=2))

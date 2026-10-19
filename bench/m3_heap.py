#!/usr/bin/env python3
"""CDP snapshot summaries and shortest strong root paths; never count weak edges as owners."""
import argparse
from collections import Counter, deque
import json
import sys
import tempfile
from pathlib import Path


def inspect(path, cuts=False):
    raw = json.loads(Path(path).read_text())
    meta = raw['snapshot']['meta']
    nf, ef = meta['node_fields'], meta['edge_fields']
    assert 'detachedness' in nf, 'Snapshot does not expose DOM detachedness'
    n, e, strings = raw['nodes'], raw['edges'], raw['strings']
    nw, ew = len(nf), len(ef)
    ni = {name: nf.index(name) for name in nf}
    ei = {name: ef.index(name) for name in ef}
    nt, et = meta['node_types'][0], meta['edge_types'][0]
    count = len(n)//nw
    starts = [0]*(count+1)
    for index in range(count): starts[index+1] = starts[index] + n[index*nw+ni['edge_count']]*ew
    def title(index): return strings[n[index*nw+ni['name']]]
    def kind(index): return nt[n[index*nw+ni['type']]]
    def edges(index):
        for edge in range(starts[index],starts[index+1],ew):
            typ = et[e[edge+ei['type']]]
            name = e[edge+ei['name_or_index']]
            yield e[edge+ei['to_node']]//nw, str(name) if typ in ('element','hidden') else strings[name], typ
    parent = [-1]*count
    link = [None]*count
    queue = deque([0]); parent[0] = 0
    while queue:
        source = queue.popleft()
        for dest, name, typ in edges(source):
            if typ == 'weak' or parent[dest] != -1: continue
            parent[dest], link[dest] = source, f'{typ}:{name}'
            queue.append(dest)
    def root_path(index):
        result = []
        while index and parent[index] != -1:
            result.append(f'{link[index]} -> {title(index)[:180]}#{n[index*nw+ni["id"]]}')
            index = parent[index]
        return list(reversed(result))
    groups = Counter(); detached = []; largest = []
    for index in range(count):
        name, typ = title(index), kind(index)
        groups[(typ,name)] += n[index*nw+ni['self_size']]
        if typ in ('array','object','native') and n[index*nw+ni['self_size']] >= 100000:
            largest.append(dict(name=name[:160],id=n[index*nw+ni['id']],selfBytes=n[index*nw+ni['self_size']],path=root_path(index)))
        if n[index*nw+ni['detachedness']] == 2 and typ == 'native':
            detached.append(dict(name=name,id=n[index*nw+ni['id']],selfBytes=n[index*nw+ni['self_size']],path=root_path(index)))
    maps = []; seen_maps = set()
    for source in range(count):
        if not title(source).startswith('system / Context'): continue
        for dest,name,typ in edges(source):
            if name not in ('handles','lastRuns','lastResults','lastResultsTab','tabResults','graphProbes','runs','scenarios'): continue
            target = dest
            if title(dest) == 'Object':
                target = next((d for d,k,t in edges(dest) if k == 'current'),dest)
            table = next((d for d,k,t in edges(target) if k == 'table'),None)
            if (name,target) in seen_maps: continue
            seen_maps.add((name,target))
            maps.append(dict(name=name,type=title(target),selfBytes=n[target*nw+ni['self_size']],tableBytes=n[table*nw+ni['self_size']] if table is not None else None,tableEntries=sum(k != 'map' for d,k,t in edges(table))//2 if table is not None else None,properties=[k for d,k,t in edges(target) if t in ('element','property') and k != '__proto__'],path=root_path(target)))
    cut_results = []
    if cuts:
        for blocked in [('actions',), ('paint',), ('actions','paint')]:
            seen = bytearray(count); seen[0] = 1; pending = deque([0])
            while pending:
                source = pending.popleft()
                for dest,key,typ in edges(source):
                    if typ == 'weak' or (typ == 'context' and key in blocked) or seen[dest]: continue
                    seen[dest] = 1; pending.append(dest)
            removed = [i for i in range(count) if parent[i] != -1 and not seen[i]]
            cut_results.append(dict(contextEdges=blocked,exclusiveBytes=sum(n[i*nw+ni['self_size']] for i in removed),nodes=len(removed),detachedNative=sum(kind(i)=='native' and n[i*nw+ni['detachedness']]==2 for i in removed)))
    return dict(cuts=cut_results,file=str(path),nodes=count,selfBytes=sum(n[i+ni['self_size']] for i in range(0,len(n),nw)),detachedNativeCount=len(detached),detached=detached[:20],detachedSelfBytes=sum(d['selfBytes'] for d in detached),largestSelf=sorted(largest,key=lambda d:d['selfBytes'],reverse=True)[:12],topSelf=[dict(type=typ,name=name[:160],bytes=size) for (typ,name),size in groups.most_common(25)],maps=maps)


def self_check():
    # The weak root shortcut must not hide the real strong owner of a detached node.
    strings = ['root', 'system / Context', 'Map', 'table', '<div>', 'owner', 'actions', 'graphProbes', 'map']
    snapshot = dict(snapshot=dict(meta=dict(node_fields=['type','name','id','self_size','edge_count','detachedness'], edge_fields=['type','name_or_index','to_node'], node_types=[['synthetic','hidden','object','native']], edge_types=[['internal','context','weak'] ])), strings=strings,
        nodes=[0,0,1,0,2,0, 1,1,3,16,2,0, 2,2,5,16,1,0, 1,3,7,76,1,0, 3,4,9,48,0,2],
        edges=[0,5,6, 2,5,24, 1,6,24, 1,7,12, 0,3,18, 0,8,0])
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory)/'fixture.heapsnapshot'; path.write_text(json.dumps(snapshot)); result=inspect(path)
    assert result['detachedNativeCount'] == 1 and result['detachedSelfBytes'] == 48
    assert any('context:actions' in step for step in result['detached'][0]['path'])
    assert result['maps'][0]['name'] == 'graphProbes' and result['maps'][0]['tableEntries'] == 0
    print('m3 heap parser self-check passed')


if __name__ == '__main__':
    if '--self-check' in sys.argv:
        self_check(); raise SystemExit(0)
    parser = argparse.ArgumentParser(); parser.add_argument('snapshots',nargs='+'); parser.add_argument('--output',required=True); parser.add_argument('--cuts', action='store_true')
    args=parser.parse_args()
    result=[inspect(path, args.cuts) for path in args.snapshots]
    Path(args.output).write_text(json.dumps(result,indent=2))
    for point in result: print(point['file'],point['nodes'],point['selfBytes'],point['detachedNativeCount'])

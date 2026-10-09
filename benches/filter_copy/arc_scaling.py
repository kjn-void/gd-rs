#!/usr/bin/env python3
"""Fresh paired Arc pipeline/core-count measurements on a single host."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import statistics
import sys

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / 'target/arc-scaling'
spec = importlib.util.spec_from_file_location('helper', ROOT / 'benches/text_workflow/compare.py')
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
spec = importlib.util.spec_from_file_location('arrays', ROOT / 'benches/filter_copy/arrays.py')
arrays = importlib.util.module_from_spec(spec)
spec.loader.exec_module(arrays)
ALL = ['current', 'fused-par-drop', 'chunks', 'chunks-par-drop', 'chunks4-par-drop', 'indexed-par-drop']


def summary(report):
    rows=[]
    for case in report['cases']:
        for strategy in report['metadata']['strategies']:
            runs=[r for r in case['measurements'] if r['strategy']==strategy]
            medians=[statistics.median(b['wall_ns'] for b in r['batches']) for r in runs]
            cpus=[statistics.median(b['cpu_cores'] for b in r['batches']) for r in runs]
            phases=next(p['batches'] for p in case['phases'] if p['strategy']==strategy)
            rows.append({**{k:case[k] for k in ['text_bytes','workers','selectivity']},'strategy':strategy,
                         'median_ns':statistics.median(medians),'min_round_ns':min(medians),'max_round_ns':max(medians),
                         'cpu_cores':statistics.median(cpus),
                         'construction_ns':statistics.median(b['construction_ns'] for b in phases),
                         'cleanup_ns':statistics.median(b['cleanup_ns'] for b in phases)})
    return rows


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workers',type=int,nargs='+',default=[1,2,4,6,8,12])
    parser.add_argument('--strategies',nargs='+',choices=ALL,default=ALL)
    parser.add_argument('--rounds',type=int,default=5)
    parser.add_argument('--samples',type=int,default=7)
    parser.add_argument('--sample-ms',type=int,default=50)
    parser.add_argument('--offset',type=int,default=0)
    parser.add_argument('--skip-build',action='store_true')
    parser.add_argument('--build-only',action='store_true')
    parser.add_argument('--allow-contended',action='store_true')
    parser.add_argument('--verify-full',action='store_true')
    parser.add_argument('--case',type=int,nargs=3,action='append',metavar=('BYTES','WORKERS','PERCENT'))
    parser.add_argument('--oracle',type=Path,default=ROOT/'target/filter-copy-oracle.json')
    parser.add_argument('--output',type=Path,default=OUT/'results.json')
    args=parser.parse_args()
    if any(w<1 or w>(os.cpu_count() or 1) for w in args.workers) or min(args.rounds,args.samples,args.sample_ms)<1:
        parser.error('invalid dimensions')
    OUT.mkdir(parents=True,exist_ok=True)
    manifest=ROOT/'benches/filter_copy/arc_scaling/Cargo.toml'
    env=os.environ.copy()
    env.pop('CARGO_ENCODED_RUSTFLAGS',None)
    env['RUSTFLAGS']='-C target-cpu=native'
    if not args.skip_build:
        with (OUT/'build.log').open('w') as log:
            helper.run(['cargo','build','--release','--locked','--offline','--manifest-path',manifest,'--target-dir',OUT/'build'],env=env,stdout=log,stderr=helper.subprocess.STDOUT)
    if args.build_only:return
    binary=OUT/'build/release/arc-scaling'
    source_paths=sorted([*ROOT.glob('src/**/*.rs'), ROOT/'Cargo.toml',ROOT/'Cargo.lock',
                         ROOT/'benches/filter_copy/arc_scaling.rs',Path(__file__),manifest,manifest.with_name('Cargo.lock'),
                         ROOT/'benches/filter_copy/arrays.py', ROOT/'benches/text_workflow/compare.py'])
    sources={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in source_paths}
    binary_hash=hashlib.sha256(binary.read_bytes()).hexdigest()
    expected=json.loads(args.oracle.read_text())
    assert expected['rows']==1_000_000
    meta={'utc':datetime.now(timezone.utc).isoformat(),'host':platform.node(),'platform':platform.platform(),
          'cpu':helper.text(['sysctl','-n','machdep.cpu.brand_string']),'cacheline_bytes':int(helper.text(['sysctl','-n','hw.cachelinesize'])),
          'cpu_topology':helper.text(['sysctl','hw.physicalcpu','hw.logicalcpu','hw.perflevel0.name','hw.perflevel0.physicalcpu','hw.perflevel1.name','hw.perflevel1.physicalcpu','hw.perflevel2.name','hw.perflevel2.physicalcpu']),
          'ram_bytes':int(helper.text(['sysctl','-n','hw.memsize'])),'rustc':helper.text(['rustc','-Vv']),
          'revision':helper.text(['git','rev-parse','HEAD']),'process_niceness':os.nice(0),
          'source_sha256':sources,'binary_sha256':binary_hash,'oracle_sha256':hashlib.sha256(args.oracle.read_bytes()).hexdigest(),
          'rows':1_000_000,'text_bytes':[16,128],'workers':args.workers,'selectivity':[10,50,90],
          'strategies':args.strategies,'rounds':args.rounds,'samples':args.samples,'sample_ms':args.sample_ms,
          'rust_flags':'release -O3; thin LTO; codegen-units=1; target-cpu=native; locked; offline; gd no-default-features + rayon',
          'ownership':'one contiguous ordered Vec<Arc<Record>> target; same record allocations shared with source; source remains alive during timed destruction',
          'timing':'source, pool startup and verification excluded; filtering, Arc clones, allocation, concatenation and completed cleanup included; CPU process clock spans only each timed batch; phase diagnostics separate from primary timing',
          'affinity':'OS scheduling; exact-size persistent Rayon pool; no affinity',
          'contended_diagnostics':args.allow_contended,'invocation':sys.argv,'measurement_complete':False,
          'thermal_before':helper.text(['pmset','-g','therm'])}
    report={'metadata':meta,'edge_verification':[],'full_verification':[],'cases':[],'load_samples':[]}
    args.output.parent.mkdir(parents=True,exist_ok=True)
    def invoke(rows,length,workers,percent,mode,offset=0):
        command=[binary,rows,length,workers,percent,args.rounds,args.samples,args.sample_ms,offset,','.join(args.strategies),mode]
        result=json.loads(helper.text(command))
        oracle=expected['lengths'][str(length)][str(percent)] if rows==1_000_000 else arrays.oracle(rows,length,[percent])[str(percent)]
        for value in result['verification']:
            if any(value[k]!=v for k,v in oracle.items()) or not value['refcounts_restored']:
                raise RuntimeError('ordered oracle/reference-count check failed')
        if mode=='verify' and not result['source_drop_checked']:raise RuntimeError('source-drop check failed')
        return {'rows':rows,'text_bytes':length,'workers':workers,'selectivity':percent,**result}
    try:
        for workers in sorted(set([1,max(args.workers)])):
            for length in [16,128]:
                for rows in [0,1,31,1001]:
                    for percent in [0,10,50,90,100]:
                        report['edge_verification'].append(invoke(rows,length,workers,percent,'verify'))
        print('Boundary, ownership and reference-count checks passed',flush=True)
        cases=[(n,w,p) for n in [16,128] for w in args.workers for p in [10,50,90]]
        if args.case:cases=[c for c in cases if list(c) in args.case]
        for position,(length,workers,percent) in enumerate(cases):
            helper.load_guard(report,args.allow_contended)
            if args.verify_full:
                report['full_verification'].append(invoke(1_000_000,length,workers,percent,'verify'))
            result=invoke(1_000_000,length,workers,percent,'time',position+args.offset)
            report['cases'].append(result)
            report['summary']=summary(report)
            args.output.write_text(json.dumps(report,indent=2)+'\n')
            print(f'{length} B, {workers} workers, {percent}%: {position+1}/{len(cases)} cases complete',flush=True)
        helper.load_guard(report,args.allow_contended)
        meta['measurement_complete']=True
    finally:
        meta['source_unchanged']=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in sources.items())
        meta['binary_unchanged']=hashlib.sha256(binary.read_bytes()).hexdigest()==binary_hash
        meta['thermal_after']=helper.text(['pmset','-g','therm'])
        args.output.write_text(json.dumps(report,indent=2)+'\n')
    assert meta['source_unchanged'] and meta['binary_unchanged']
    print(f'Wrote {args.output}',flush=True)

if __name__=='__main__':main()

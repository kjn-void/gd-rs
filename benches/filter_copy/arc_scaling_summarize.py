#!/usr/bin/env python3
"""Validate and render the current M6 Arc scaling and GD comparison."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import statistics

ROOT=Path(__file__).resolve().parents[2]
NAMES={'gd':'GD memcpy','std':'C++ STL std::string','compact':'gd-rs CompactString','fixed':'gd-rs fixed buffer','arc_chunks':'gd-rs Arc','arc_chunks4':'gd-rs Arc'}
OVERVIEW_START='<!-- arc-scaling-m6:start -->'
OVERVIEW_END='<!-- arc-scaling-m6:end -->'
STRATEGIES=['current','fused-par-drop','chunks-par-drop','chunks4-par-drop']
LINKS='Sources: [Arc scaling harness](../../benches/filter_copy/arc_scaling.rs), [scaling runner](../../benches/filter_copy/arc_scaling.py), [gd-rs shared-record API](../../src/table/shared_record.rs), [Rust comparison driver](../../benches/filter_copy/driver.rs), [C++ comparison driver](../../benches/cpp-reference/filter_copy.cpp), [comparison runner](../../benches/filter_copy/compare.py). The C++ driver has no shared-pointer counterpart. gd-rs Arc shares payloads while the other four variants copy them.'


def gm(values):return math.exp(statistics.mean(math.log(v) for v in values))
def key(r):return r['text_bytes'],r['workers'],r['selectivity'],r['implementation']
def check_common(path,partial=False):
 d=json.loads(path.read_text());m=d['metadata']
 assert all(m[k] for k in ['measurement_complete','source_unchanged','gd_source_unchanged','binaries_unchanged'])
 assert m['rows']==1_000_000 and m['process_niceness']==0 and len(m['implementations'])==6
 expected={key(r):(r['count'],r['digest']) for r in d['verification']}
 for r in d['verification']+d['edge_verification']:
  assert r['source_drop_checked'] and r['shared_records'] if r['implementation'].startswith('arc') else r['independent_target']
 for r in d['measurements']:
  assert (r['count'],r['digest'])==expected[key(r)] and len(r['samples_ns'])==m['samples']
 for case,value in expected.items():assert value==expected[(*case[:3],'gd')]
 ix={key(r):r for r in d['summary']}
 for k,r in ix.items():
  runs=[v for v in d['measurements'] if key(v)==k]
  assert sorted(v['round'] for v in runs)==list(range(1,m['rounds']+1))
  assert statistics.median(statistics.median(v['samples_ns']) for v in runs)==r['median_ns']
 if not partial:assert len(ix)==72 and len(expected)==72 and len(d['edge_verification'])==480
 return d,ix


def comparison_overview(data, ix):
 """Summarize the current implementation and its comparison with GD."""
 impls=[i for i in data['metadata']['implementations'] if i!='arc']
 lines=[OVERVIEW_START,'','## M6 comparison','',LINKS,'',
  'This comparison measures five representations of one million records containing three integers and two strings. '
  'Both strings contain either 16 or 128 bytes; a numeric filter selects 10%, 50% or 90% of the rows into one ordered target, using one or eight workers. '
  'gd-rs Arc uses chunked filtering, preallocated worker-local handle buffers, ordered concatenation and joined parallel cleanup. '
  'At one worker it uses serial filtering and cleanup. '
  'These are diagnostics under the host’s current background load; source allocation is excluded and completed target cleanup is included.','',
  '**Speed relative to GD** (higher is faster; equal weight per case). The first four variants deep-copy payloads; gd-rs Arc shares them.','',
  '| Group | '+' | '.join(NAMES[i] for i in impls)+' |','|---|'+'---:|'*len(impls)]
 for label,w,n in [('All 12 cases',None,None),('1 worker, 16 B',1,16),('1 worker, 128 B',1,128),('8 workers, 16 B',8,16),('8 workers, 128 B',8,128)]:
  lines.append('| '+label+' | '+' | '.join(f'{gm(ix[t,c,p,"gd"]["median_ns"]/ix[t,c,p,i]["median_ns"] for t in [16,128] for c in [1,8] for p in [10,50,90] if (w is None or w==c) and (n is None or n==t)):.3f}×' for i in impls)+' |')
 lines+=['','![M6 performance relative to GD memcpy](measurements/filter-copy-arc-m6.png)','',
  'See [Arc scaling on M6](arc-scaling-m6-results.md) for absolute timings, 1/2/4/6/8/12-worker results, phase measurements, confirmation runs and implementation details. '
  'The [fixed-array SoA experiment](filter-copy-arrays-m6-results.md) separately compares GD row memcpy with deep copies of five columns containing constant size text fields and metadata.','',OVERVIEW_END,'']
 return lines


def update_overview(path, data, ix):
 path.write_text('# Whole-record filtering into one target\n\n'+'\n'.join(comparison_overview(data,ix)))


def main():
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--cores',type=Path,required=True);p.add_argument('--comparison',type=Path,required=True)
 p.add_argument('--confirmations',type=Path,required=True);p.add_argument('--regression',type=Path,required=True);p.add_argument('--selection',type=Path,required=True)
 p.add_argument('--report',type=Path,default=ROOT/'docs/high-level/arc-scaling-m6-results.md')
 p.add_argument('--overview',type=Path,default=ROOT/'docs/high-level/filter-copy-results.md')
 p.add_argument('--output-dir',type=Path,default=ROOT/'docs/high-level/measurements')
 a=p.parse_args();cores=json.loads(a.cores.read_text());cm=cores['metadata']
 assert all(cm[k] for k in ['measurement_complete','source_unchanged','binary_unchanged'])
 assert cm['process_niceness']==0 and cm['contended_diagnostics'] and len(cores['cases'])==36
 assert cm['strategies']==list(STRATEGIES) and cm['rounds']==4 and cm['samples']==7
 spec=importlib.util.spec_from_file_location('scaling',ROOT/'benches/filter_copy/arc_scaling.py');runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)
 assert runner.summary(cores)==cores['summary']
 ci={(r['text_bytes'],r['workers'],r['selectivity'],r['strategy']):r for r in cores['summary']}
 assert set(ci)=={(n,w,p,s) for n in [16,128] for w in [1,2,4,6,8,12] for p in [10,50,90] for s in cm['strategies']}
 for case in cores['cases']:
  for strategy in cm['strategies']:
   runs=[r for r in case['measurements'] if r['strategy']==strategy]
   assert sorted(r['round'] for r in runs)==list(range(1,cm['rounds']+1))
   assert all(len(r['batches'])==cm['samples'] and all(b['wall_ns']>0 and b['cpu_cores']>0 for b in r['batches']) for r in runs)
  expected={(r['count'],r['digest']) for r in case['verification']};assert len(expected)==1
  assert case['refcounts_restored'] and all(r['refcounts_restored'] for r in case['verification'])
 for case in cores['full_verification']+cores['edge_verification']:assert case['source_drop_checked']
 data,ix=check_common(a.comparison);confirmation,rx=check_common(a.confirmations,True)
 regression,gx=check_common(a.regression,True)
 assert len(rx)==24 and len(gx)==6 and {k[:3] for k in gx}=={(128,8,90)}
 assert not set(rx)&set(gx)
 rx.update(gx)
 assert cm['oracle_sha256']==data['metadata']['oracle_sha256']
 for k,h in cm['source_sha256'].items():
  if k in data['metadata']['source_sha256']:assert h==data['metadata']['source_sha256'][k]
 for case in cores['cases']:
  reference=ix[case['text_bytes'],1,case['selectivity'],'gd']
  verified=next(r for r in data['verification'] if key(r)==key(reference))
  assert all((r['count'],r['digest'])==(verified['count'],verified['digest']) for r in case['verification'])
 for k in ['source_sha256','binary_sha256','oracle_sha256','implementations','samples','sample_ms','rounds']:
  assert data['metadata'][k]==confirmation['metadata'][k]==regression['metadata'][k]
 selection=json.loads(a.selection.read_text());fast=selection['implementation'];strategy=selection['strategy']
 assert data['metadata']['arc_parallel']==fast
 ns=[16,128];ps=[10,50,90];workers=[1,2,4,6,8,12];impls=[i for i in data['metadata']['implementations'] if i!='arc']
 def cr(n,w,p,s,field='median_ns'):return ci[n,w,p,s][field]
 def rel(impl,w=None,n=None):
  return gm(ix[t,c,p,'gd']['median_ns']/ix[t,c,p,impl]['median_ns'] for t in ns for c in [1,8] for p in ps if (w is None or w==c) and (n is None or n==t))
 scale=lambda s,w:gm(cr(n,1,p,s)/cr(n,w,p,s) for n in ns for p in ps)
 fast_cpu=statistics.mean(cr(n,8,p,strategy,'cpu_cores') for n in ns for p in ps)
 chunk_factor=4 if fast=='arc_chunks4' else 1
 lines=['# Arc scaling and GD comparison on Apple M6','',LINKS,'',
  f'gd-rs Arc is **{scale(strategy,8):.3f}× faster with eight workers than with one** in the core sweep. '
  f'In the comparison with GD, its geometric-mean speed is **{rel(fast):.3f}× GD memcpy** across all 12 cases. '
  'Each string size, selection rate and worker count receives equal weight within its comparison. '
  'Arc shares records; GD and the other three representations deep-copy their payloads.','',
  f'Measured from {cm["utc"]} (UTC).','',
  '**Contended diagnostics:** these runs retain current host load and OS scheduling. The primary results and separate confirmations are both retained.','',
  '## Current implementation','',LINKS,'',
  '`SharedRecordTable<Record>` stores one contiguous `Vec<Arc<Record>>`. Each record contains three `u64` fields and two `CompactString` fields. '
  'Rayon processes source chunks in parallel, evaluating the numeric predicate and cloning each matched Arc handle into a worker-local vector. '
  'The local vectors are concatenated in source order into one destination table. The implementation uses two safe public APIs:','',
  '- `par_filter_chunked(chunk_rows, predicate)` gives each source chunk a local vector reserved to its source length, avoiding local growth. It concatenates the matched handles in source order without additional Arc clones.',
  '- `par_drop()` consumes the result and releases its handles on Rayon workers, joining before returning. Ordinary `drop` remains sequential. Small targets below 4,096 handles stay on the caller; larger targets use a minimum cleanup grain of 4,096.', '',
  f'The M6 comparison uses **{chunk_factor} source chunk{"s" if chunk_factor>1 else ""} per worker** (approximately {1_000_000//(8*chunk_factor):,} source rows per chunk at eight workers). '
  'At one worker it uses serial filtering and cleanup. Reserving each local vector to its source-chunk length avoids growth during filtering; '
  'concatenation moves handles without cloning them again. Joined parallel cleanup distributes reference-count decrements across the Rayon workers.','',
  f'Primary-batch CPU time divided by elapsed time averages **{fast_cpu:.2f} active-core equivalents** at eight workers. '
  'This measures concurrent CPU activity, including scheduling and any worker spinning; it does not measure useful work alone. Each record has its own reference count: there is no single global counter shared by all rows. '
  'Per-record atomic operations and pointer chasing still remain; greater CPU use does not imply linear speedup. '
  'Moving reference-count updates between caller and worker cores can also create cache-coherence traffic, but this experiment does not isolate that cost with hardware counters.','',
  'The destination remains readable after source destruction and preserves input order. No record or string payload is copied. '
  'Construction, temporary buffers, concatenation and completed cleanup are all timed. The source stays alive during timing, so target cleanup releases handles without freeing record or string payloads.','',
  '## Scaling across cores','',LINKS,'',
  '| Workers | gd-rs Arc speed vs 1 worker | gd-rs Arc active CPU cores |',
  '|---:|---:|---:|']
 for w in workers:
  lines.append(f'| {w} | {scale(strategy,w):.3f}× | {statistics.mean(cr(n,w,p,strategy,"cpu_cores") for n in ns for p in ps):.2f} |')
 lines+=['','![gd-rs Arc performance relative to GD memcpy](measurements/arc-scaling-m6.png)', '',
  'The graph uses the comparison at one and eight workers, where GD was also measured. Higher is faster, with GD memcpy at 1×. The table above shows the separate Arc-only sweep at all six worker counts.','',
  'CPU-core equivalents are process CPU seconds divided by wall seconds inside each primary batch, excluding source construction and verification. '
  'The process clock sums work across threads; it does not identify particular physical cores or core types. The M6 has two Super, four Performance and six Efficiency cores. '
  'Eight workers give the best measured aggregate time; twelve use more CPU but take longer. Without affinity or hardware counters, this test cannot separate core placement from memory-system and scheduling costs.','',
  '| Text bytes | Selected | gd-rs Arc build ms | gd-rs Arc cleanup ms |',
  '|---:|---:|---:|---:|']
 for n in ns:
  for p in ps:
   values=[cr(n,8,p,strategy,phase)/1e6 for phase in ['construction_ns','cleanup_ns']]
   lines.append(f'| {n} | {p}% | '+' | '.join(f'{v:.3f}' for v in values)+' |')
 lines+=['','Stage timings come from seven separately instrumented operations after the primary batches; they are diagnostic and are not added to the primary timing samples.','',
  '## Comparison with GD and standard C++ containers','',LINKS,'',
  'The tables and graphs show GD row memcpy, standard C++ `std::vector`/`std::string`, gd-rs CompactString, gd-rs fixed buffers, and gd-rs Arc. These representations were measured together. '
  'One million source rows contain three numbers and two 16- or 128-byte strings; 10%, 50% or 90% of rows are selected into one ordered target. '
  'The first four variants deep-copy values. gd-rs Arc shares records and times handle release while the source remains alive. C++ and the other Rust variants use caller-thread cleanup.','',
  '**Speed relative to GD** (`GD time / variant time`; higher is faster). Geometric means weight each case equally.','',
  '| Group | '+' | '.join(NAMES[i] for i in impls)+' |', '|---|'+'---:|'*len(impls)]
 for label,w,n in [('All 12 cases',None,None),('1 worker, 16 B',1,16),('1 worker, 128 B',1,128),('8 workers, 16 B',8,16),('8 workers, 128 B',8,128)]:
  lines.append('| '+label+' | '+' | '.join(f'{rel(i,w,n):.3f}×' for i in impls)+' |')
 lines+=['','![M6 performance relative to GD memcpy](measurements/filter-copy-arc-m6.png)','',
  '| Text bytes | Workers | Selected | '+' | '.join(NAMES[i]+' ms' for i in impls)+' |',
  '|---:|---:|---:|'+'---:|'*len(impls)]
 for n in ns:
  for w in [1,8]:
   for p in ps:lines.append(f'| {n} | {w} | {p}% | '+' | '.join(f'{ix[n,w,p,i]["median_ns"]/1e6:.3f}' for i in impls)+' |')
 lines+=['',
  '## Confirmation measurements','',LINKS,'',
  'The case with the largest relative round-median range in each text-size/worker group was repeated. '
  'The 128-byte, eight-worker, 90%-selection case was also repeated to check timing variability. Primary and repeat results are reported separately.','',
  '| Text bytes | Workers | Selected | Primary gd-rs Arc / GD speed | Repeat gd-rs Arc / GD speed |',
  '|---:|---:|---:|---:|---:|']
 for n,w,p in sorted({k[:3] for k in rx}):
  vals=[index[n,w,p,'gd']['median_ns']/index[n,w,p,fast]['median_ns'] for index in [ix,rx]]
  lines.append(f'| {n} | {w} | {p}% | '+' | '.join(f'{v:.3f}×' for v in vals)+' |')
 lines+=['',f'At 128 bytes, eight workers and 90% selection, gd-rs Arc takes **{ix[128,8,90,fast]["median_ns"]/1e6:.3f} ms** in the primary run and **{rx[128,8,90,fast]["median_ns"]/1e6:.3f} ms** in the repeat. '
  'This demonstrates substantial run-to-run variability. '
  'Host scheduling, allocation layout and cache state are possible contributors; this experiment does not isolate them. '
  'Read the reported ratios as workload-specific diagnostics under contention, with the repeat measurements showing their practical uncertainty.']
 lines+=['','## Reproduction and validation','',LINKS,'',
  f'- Host `{cm["host"]}`; `{cm["cpu"]}`; {cm["ram_bytes"]//2**30} GiB; {cm["cacheline_bytes"]}-byte cache lines; niceness zero; no affinity.',
  f'- Compiler: `{cm["rustc"].splitlines()[0]}`; comparison C++: `{data["metadata"]["cxx"].splitlines()[0]}`.',
  f'- Core sweep: {cm["rounds"]} rotated rounds, {cm["samples"]} batches, calibrated to at least {cm["sample_ms"]} ms; the Arc pipeline uses the public APIs described above.',
  f'- Full comparison: {data["metadata"]["rounds"]} rotated process rounds, {data["metadata"]["samples"]} batches, at least {data["metadata"]["sample_ms"]} ms; native release builds; Rust thin LTO, C++ IPO, without sanitizers.',
  '- Million-row source-destruction checks and boundary/ownership checks pass for every reported implementation, using an independent Python value oracle.',
  '- Every timed process verifies the complete ordered result outside its timing. Reference counts return to one after Arc target cleanup. Source and executable fingerprints are retained and remained unchanged.',
  '- Separate AddressSanitizer tests passed for Arc filtering, cleanup and process-clock FFI; no sanitizer timings enter these results. Leak detection was disabled because Apple ASan does not support it.',
  '- Library tests cover ordering, duplicate handles, copy-on-write, final-owner destruction, empty input and predicate panics. Full repository CI and Rust 1.86 checks passed.', '',
  '[Core raw data](measurements/arc-scaling-m6.json), [GD comparison](measurements/filter-copy-arc-m6.json), '
  '[confirmations](measurements/filter-copy-arc-m6-confirmations.json), [additional 90%-selection repeat](measurements/filter-copy-arc-m6-regression.json).','',
  '```sh',
  'python3 benches/filter_copy/arc_scaling.py --strategies chunks-par-drop \\',
  '  --workers 1 2 4 6 8 12 --rounds 4 --samples 7 --sample-ms 50 --verify-full --allow-contended',
  f'python3 benches/filter_copy/compare.py --arc-parallel {fast} --rounds 6 --allow-contended',
  '# Exact oracle/output paths and confirmation case arguments are in each raw file\'s invocation metadata.',
  '```','']
 import matplotlib
 matplotlib.use('Agg')
 import matplotlib.pyplot as plt
 import numpy as np
 a.output_dir.mkdir(parents=True,exist_ok=True)
 fig,axes=plt.subplots(1,2,figsize=(12,4.5),sharey=True,layout='constrained')
 for ax,n in zip(axes,ns):
  ax.plot([1,8],[rel(fast,w,n) for w in [1,8]],marker='o',color='#172d42',label='gd-rs Arc')
  ax.axhline(1,color='#666666',linestyle='--',label='GD memcpy = 1×')
  for w in [1,8]:
   value=rel(fast,w,n)
   ax.annotate(f'{value:.2f}×',(w,value),xytext=(0,8 if value>=1 else -18),textcoords='offset points',ha='center')
  ax.set_title(f'{n}-byte strings');ax.set_xlabel('Workers');ax.set_ylabel('Performance vs GD memcpy (higher is better)');ax.set_xticks([1,8]);ax.set_xlim(.5,8.5);ax.tick_params(labelleft=True);ax.grid(alpha=.25)
  ax.set_ylim(0, max(rel(fast,w,t) for w in [1,8] for t in ns)*1.15)
 axes[0].legend(fontsize=8);fig.suptitle('M6 · gd-rs Arc shares payloads; GD memcpy deep-copies them\nGeometric mean over 10%, 50%, 90% selection · cleanup included · contended diagnostics')
 fig.savefig(a.output_dir/'arc-scaling-m6.png',dpi=180);plt.close(fig)
 fig,axes=plt.subplots(2,2,figsize=(14,9),sharey=True,layout='constrained');colors=['#277da1','#f8961e','#43aa8b','#9b5de5','#172d42']
 for ax,(w,n) in zip(axes.flat,[(1,16),(1,128),(8,16),(8,128)]):
  positions=np.arange(3);step=.16
  for j,i in enumerate(impls):
   values=[ix[n,w,p,'gd']['median_ns']/ix[n,w,p,i]['median_ns'] for p in ps]
   bars=ax.bar(positions+(j-(len(impls)-1)/2)*step,values,step*.92,label=NAMES[i],color=colors[j])
   ax.bar_label(bars,labels=[f'{v:.2f}×' for v in values],padding=3,fontsize=7,bbox=dict(facecolor='white',edgecolor='none',pad=.1,alpha=.9))
  ax.axhline(1,color='#666666',linestyle='--',linewidth=1)
  ax.set_xticks(positions,['10%','50%','90%']);ax.set_title(f'{w} worker(s) · {n}-byte strings');ax.set_ylabel('Performance vs GD memcpy (higher is better)');ax.set_xlabel('Rows selected');ax.grid(axis='y',alpha=.2);ax.set_axisbelow(True)
  ax.tick_params(labelleft=True)
  ax.set_ylim(0,max(ix[t,c,p,'gd']['median_ns']/ix[t,c,p,i]['median_ns'] for t in ns for c in [1,8] for p in ps for i in impls)*1.15)
 handles,labels=axes.flat[0].get_legend_handles_labels();fig.legend(handles,labels,loc='outside lower center',ncol=3,fontsize=9)
 fig.suptitle('M6 · 1,000,000 rows · GD memcpy = 1× · higher is faster\nFirst four deep-copy values; gd-rs Arc shares payloads · cleanup included · contended diagnostics')
 fig.savefig(a.output_dir/'filter-copy-arc-m6.png',dpi=180);plt.close(fig)
 a.report.write_text('\n'.join(lines))
 update_overview(a.overview,data,ix)
 print(f'Arc 1-to-8-worker scaling {scale(strategy,8):.3f}×; overall Arc/GD {rel(fast):.3f}×')

if __name__=='__main__':main()

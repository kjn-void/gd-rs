#!/usr/bin/env python3
"""Validate and render the M6 Arc scaling investigation and fresh comparison."""
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
STRATEGIES={'current':'Original','fused-par-drop':'Parallel cleanup','chunks-par-drop':'Chunks + parallel cleanup','chunks4-par-drop':'4× chunks + parallel cleanup'}
LINKS='Sources: [Arc scaling harness](../../benches/filter_copy/arc_scaling.rs), [scaling runner](../../benches/filter_copy/arc_scaling.py), [gd-rs shared-record API](../../src/table/shared_record.rs), [Rust comparison driver](../../benches/filter_copy/driver.rs), [C++ comparison driver](../../benches/cpp-reference/filter_copy.cpp), [comparison runner](../../benches/filter_copy/compare.py). The C++ driver has no shared-pointer counterpart; The displayed Arc variant shares payloads while the other four variants copy them.'


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
 """Fresh cohort summary shared with the historical report renderer."""
 impls=[i for i in data['metadata']['implementations'] if i!='arc'];fast=data['metadata']['arc_parallel']
 gain=gm(ix[n,8,p,'arc']['median_ns']/ix[n,8,p,fast]['median_ns'] for n in [16,128] for p in [10,50,90])
 lines=[OVERVIEW_START,'','## Updated M6 comparison: parallel Arc cleanup','',LINKS,'',
  'The current M6 comparison shows five representations, with gd-rs Arc using chunked filtering and parallel cleanup. '
  f'The improved Arc path is **{gain:.3f}× faster at eight workers**, averaged geometrically over both string sizes and all three selection rates. '
  'All six variants were rerun together, including GD memcpy and standard STL containers. Five of six eight-worker cases improve; the large-string 90%-selection case regresses in the primary cohort and is repeated separately. '
  'These are current-load diagnostics; source allocation is excluded and completed target cleanup is included.','',
  '**Speed relative to GD** (higher is faster; equal weight per case). The first four variants deep-copy payloads; gd-rs Arc shares them.','',
  '| Group | '+' | '.join(NAMES[i] for i in impls)+' |','|---|'+'---:|'*len(impls)]
 for label,w,n in [('All 12 cases',None,None),('1 worker, 16 B',1,16),('1 worker, 128 B',1,128),('8 workers, 16 B',8,16),('8 workers, 128 B',8,128)]:
  lines.append('| '+label+' | '+' | '.join(f'{gm(ix[t,c,p,"gd"]["median_ns"]/ix[t,c,p,i]["median_ns"] for t in [16,128] for c in [1,8] for p in [10,50,90] if (w is None or w==c) and (n is None or n==t)):.3f}×' for i in impls)+' |')
 lines+=['','![M6 performance relative to GD memcpy](measurements/filter-copy-arc-m6.png)','',
  'See the [Arc scaling investigation](arc-scaling-m6-results.md) for all absolute timings, 1/2/4/6/8/12-worker results, phase measurements, confirmation runs, and why the APIs help. '
  'Only the latest Arc implementation appears in the current tables and graphs. The original remains in the raw data and explanatory before-and-after analysis.','',
  'Earlier results are available in the [historical three-host report](filter-copy-three-host-results.md) and the [fixed-array SoA experiment](filter-copy-arrays-m6-results.md). The improved Arc path has only been measured on M6.','',OVERVIEW_END,'']
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
 improvement=gm(cr(n,8,p,'current')/cr(n,8,p,strategy) for n in ns for p in ps)
 scale=lambda s,w:gm(cr(n,1,p,s)/cr(n,w,p,s) for n in ns for p in ps)
 current_cpu=statistics.mean(cr(n,8,p,'current','cpu_cores') for n in ns for p in ps)
 fast_cpu=statistics.mean(cr(n,8,p,strategy,'cpu_cores') for n in ns for p in ps)
 current_drop=statistics.mean(cr(n,8,p,'current','cleanup_ns')/(cr(n,8,p,'current','cleanup_ns')+cr(n,8,p,'current','construction_ns')) for n in ns for p in ps)
 chunk_factor=4 if fast=='arc_chunks4' else 1
 lines=['# Arc scaling and refreshed GD comparison on Apple M6','',LINKS,'',
  f'The selected Arc pipeline is **{improvement:.3f}× faster than the original at eight workers** in the core sweep. '
  f'Going from one to eight workers improves the original pipeline by **{scale("current",8):.3f}×** and the improved pipeline by **{scale(strategy,8):.3f}×**, '
  'with equal weight for both string sizes and all three selection rates.','',
  f'Measured from {cm["utc"]} (UTC).','',
  '**Contended diagnostics:** these runs retain current host load and OS scheduling. The primary results and separate confirmations are both retained.','',
  '## What changed and why','',LINKS,'',
  'The original `par_filter` already evaluated predicates and cloned Arc handles on Rayon workers. Its output was then dropped on the calling thread. '
  'Every matched row therefore incurred a serial atomic reference-count decrement during timed cleanup. Rayon\'s unindexed collection also used temporary local buffers and a final concatenation.','',
  'Two explicit, safe APIs were added to `SharedRecordTable<S>`:','',
  '- `par_filter_chunked(chunk_rows, predicate)` gives each source chunk a local vector reserved to its source length, avoiding local growth. It concatenates the matched handles in source order without additional Arc clones.',
  '- `par_drop()` consumes the result and releases its handles on Rayon workers, joining before returning. Ordinary `drop` remains sequential. Small targets below 4,096 handles stay on the caller; larger targets use a minimum cleanup grain of 4,096.', '',
  f'The M6 comparison uses **{chunk_factor} source chunk{"s" if chunk_factor>1 else ""} per worker** (approximately {1_000_000//(8*chunk_factor):,} source rows per chunk at eight workers). '
  'Both one and four chunks per worker were measured; selection used the lower geometric-mean time over all six eight-worker cases. '
  'Their measured difference is small, so this grain is a choice for this workload rather than a universal optimum. '
  'At one worker the original serial filter and cleanup are used for both Arc variants; small differences there are measurement variation.','',
  f'In the separate phase diagnostics, serial cleanup accounted for **{current_drop*100:.1f}%** of original eight-worker elapsed time on average. '
  f'Primary-batch CPU time divided by elapsed time increased from **{current_cpu:.2f}** to **{fast_cpu:.2f}** active-core equivalents. '
  'This shows substantially more concurrent CPU activity, including scheduling and any worker spinning; it does not measure useful work alone. Each record has its own reference count: there is no single global counter shared by all rows. '
  'Per-record atomic operations and pointer chasing still remain; greater CPU use does not imply linear speedup. '
  'Moving reference-count updates between caller and worker cores can also create cache-coherence traffic, but this experiment does not isolate that cost with hardware counters.','',
  f'Parallel cleanup alone improves the eight-worker geometric mean by **{gm(cr(n,8,p,"current")/cr(n,8,p,"fused-par-drop") for n in ns for p in ps):.3f}×**. '
  f'Adding reserved chunks gives a further **{gm(cr(n,8,p,"fused-par-drop")/cr(n,8,p,strategy) for n in ns for p in ps):.3f}×**. '
  'The cleanup change therefore accounts for most of the measured gain. At 10% selection the combined gain is larger than at 90%; moving more handles and reference counts still costs time.','',
  'Initial screening also tried filtering row indices and then gathering Arc handles. That two-pass path did not beat the original at eight workers; the final sweep therefore focuses on fused filtering, chunk size and cleanup. '
  'The shorter pilot samples are archived separately and excluded from final means.','',
  'Both Arc targets still contain one contiguous vector of shared handles, remain readable after source destruction, and preserve input order. '
  'No record or string payload is copied. Construction, temporary buffers, concatenation and completed cleanup are all timed.','',
  '## Scaling across cores','',LINKS,'',
  '| Workers | gd-rs Arc speed vs 1 worker | gd-rs Arc active CPU cores |',
  '|---:|---:|---:|']
 for w in workers:
  lines.append(f'| {w} | {scale(strategy,w):.3f}× | {statistics.mean(cr(n,w,p,strategy,"cpu_cores") for n in ns for p in ps):.2f} |')
 lines+=['','![gd-rs Arc performance relative to GD memcpy](measurements/arc-scaling-m6.png)', '',
  'The graph uses the fresh comparison at one and eight workers, where GD was also measured. Higher is faster, with GD memcpy at 1×. The table above retains the separate Arc-only sweep at all six worker counts.','',
  'CPU-core equivalents are process CPU seconds divided by wall seconds inside each primary batch, excluding source construction and verification. '
  'The process clock sums work across threads; it does not identify particular physical cores or core types. The M6 has two Super, four Performance and six Efficiency cores. '
  'Eight workers give the best measured aggregate time for the selected pipeline; twelve use more CPU but take longer. Without affinity or hardware counters, this test cannot separate core placement from memory-system and scheduling costs.','',
  '| Text bytes | Selected | gd-rs Arc build ms | gd-rs Arc cleanup ms |',
  '|---:|---:|---:|---:|']
 for n in ns:
  for p in ps:
   values=[cr(n,8,p,strategy,phase)/1e6 for phase in ['construction_ns','cleanup_ns']]
   lines.append(f'| {n} | {p}% | '+' | '.join(f'{v:.3f}' for v in values)+' |')
 lines+=['','Stage timings come from seven separately instrumented operations after the primary batches; they are diagnostic and are not added to the primary timing samples.','',
  '## Fresh comparison with GD and standard C++ containers','',LINKS,'',
  'The tables and graphs show GD row memcpy, standard C++ `std::vector`/`std::string`, gd-rs CompactString, gd-rs fixed buffers, and the latest gd-rs Arc pipeline. All were measured together; the original Arc control is retained in the raw data and explanation. '
  'One million source rows contain three numbers and two 16- or 128-byte strings; 10%, 50% or 90% of rows are selected into one ordered target. '
  'The first four variants deep-copy values. gd-rs Arc shares records and times handle release while the source remains alive. C++ and the other Rust variants retain their existing caller-thread cleanup.','',
  '**Speed relative to GD** (`GD time / variant time`; higher is faster). Geometric means weight each case equally.','',
  '| Group | '+' | '.join(NAMES[i] for i in impls)+' |', '|---|'+'---:|'*len(impls)]
 for label,w,n in [('All 12 cases',None,None),('1 worker, 16 B',1,16),('1 worker, 128 B',1,128),('8 workers, 16 B',8,16),('8 workers, 128 B',8,128)]:
  lines.append('| '+label+' | '+' | '.join(f'{rel(i,w,n):.3f}×' for i in impls)+' |')
 lines+=['','![Updated M6 comparison](measurements/filter-copy-arc-m6.png)','',
  '| Text bytes | Workers | Selected | '+' | '.join(NAMES[i]+' ms' for i in impls)+' |',
  '|---:|---:|---:|'+'---:|'*len(impls)]
 for n in ns:
  for w in [1,8]:
   for p in ps:lines.append(f'| {n} | {w} | {p}% | '+' | '.join(f'{ix[n,w,p,i]["median_ns"]/1e6:.3f}' for i in impls)+' |')
 gain=gm(ix[n,8,p,'arc']['median_ns']/ix[n,8,p,fast]['median_ns'] for n in ns for p in ps)
 lines+=['',f'In this fresh comparison the improved eight-worker Arc path is **{gain:.3f}× faster** than the original Arc path. '
  'The large-string 90%-selection case regressed in this primary cohort, despite improving in the paired core sweep; its process-round timings show substantial variability. '
  'It receives an additional confirmation below. '
  'These timings are independent of the earlier three-host and fixed-array cohorts; older measurements are not pooled into the new means.','',
  '## Confirmation measurements','',LINKS,'',
  'The case with the largest relative round-median range across any variant in each text-size/worker group was repeated with all six variants. '
  'The surprising 128-byte, eight-worker, 90%-selection regression was also repeated separately. Primary numbers remain unchanged.','',
  '| Text bytes | Workers | Selected | Primary gd-rs Arc / GD speed | Repeat gd-rs Arc / GD speed |',
  '|---:|---:|---:|---:|---:|']
 for n,w,p in sorted({k[:3] for k in rx}):
  vals=[index[n,w,p,'gd']['median_ns']/index[n,w,p,fast]['median_ns'] for index in [ix,rx]]
  lines.append(f'| {n} | {w} | {p}% | '+' | '.join(f'{v:.3f}×' for v in vals)+' |')
 regression_gain=rx[128,8,90,'arc']['median_ns']/rx[128,8,90,fast]['median_ns']
 verdict='did not reproduce' if regression_gain>1 else 'persisted'
 lines+=['',f'The high-selection regression **{verdict}** in the repeat: improved/original speed was **{regression_gain:.3f}×**, '
  f'versus **{ix[128,8,90,"arc"]["median_ns"]/ix[128,8,90,fast]["median_ns"]:.3f}×** in the primary run. '
  'Separate-process timings vary more than the core sweep, where all pipelines share the same source allocation within each case. '
  'Host scheduling, allocation layout and cache state are possible contributors; this experiment does not isolate them. '
  'The improvement is therefore an aggregate result for this workload, not a guarantee for every selection rate.','',
  f'The 128-byte, one-worker, 90%-selection repeat gives a **{rx[128,1,90,"arc"]["median_ns"]/rx[128,1,90,fast]["median_ns"]:.3f}×** difference between the Arc variants even though they execute the same serial code. '
  'That control demonstrates substantial process-to-process variability, not a serial optimization. '
  'The paired core sweep is therefore the stronger evidence for the implementation change; the separately launched six-variant ratios should be read as noisy diagnostics rather than precise hardware limits.','']
 lines+=['','## Reproduction and validation','',LINKS,'',
  f'- Host `{cm["host"]}`; `{cm["cpu"]}`; {cm["ram_bytes"]//2**30} GiB; {cm["cacheline_bytes"]}-byte cache lines; niceness zero; no affinity.',
  f'- Compiler: `{cm["rustc"].splitlines()[0]}`; comparison C++: `{data["metadata"]["cxx"].splitlines()[0]}`.',
  f'- Core sweep: {cm["rounds"]} rotated rounds, {cm["samples"]} batches, calibrated to at least {cm["sample_ms"]} ms; all four pipelines use the public APIs.',
  f'- Full comparison: {data["metadata"]["rounds"]} rotated process rounds, {data["metadata"]["samples"]} batches, at least {data["metadata"]["sample_ms"]} ms; native release builds; Rust thin LTO, C++ IPO, without sanitizers.',
  f'- Core sweep: {len(cores["full_verification"])*len(cm["strategies"])} million-row source-drop strategy checks; {len(cores["edge_verification"])*len(cm["strategies"])} boundary strategy checks. Full comparison: {len(data["verification"])} million-row and {len(data["edge_verification"])} boundary checks; independent Python value oracle.',
  '- Every timed process verifies the complete ordered result outside its timing. Reference counts return to one after Arc target cleanup. Source and executable fingerprints are retained and remained unchanged.',
  '- Separate AddressSanitizer tests passed for the Arc pipelines and process-clock FFI; no sanitizer timings enter these results. Leak detection was disabled because Apple ASan does not support it.',
  '- Library tests cover ordering, duplicate handles, copy-on-write, final-owner destruction, empty input and predicate panics. Full repository CI and Rust 1.86 checks passed.', '',
  '[Core raw data](measurements/arc-scaling-m6.json), [fresh comparison](measurements/filter-copy-arc-m6.json), '
  '[confirmations](measurements/filter-copy-arc-m6-confirmations.json), [high-selection regression repeat](measurements/filter-copy-arc-m6-regression.json), [pipeline selection](measurements/arc-scaling-m6-selection.json). '
  'The initial five-pipeline screening and matching source snapshots are retained in the [pilot archive](measurements/archive/arc-scaling-pilot/README.md).','',
  '```sh',
  'python3 benches/filter_copy/arc_scaling.py --strategies current fused-par-drop chunks-par-drop chunks4-par-drop \\',
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
 print(f'Core improvement {improvement:.3f}×; fresh comparison Arc improvement {gain:.3f}×; overall improved/GD {rel(fast):.3f}×')

if __name__=='__main__':main()

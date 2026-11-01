#!/usr/bin/env python3
"""Bounded offline CLI benchmarks; synthetic data only, inherited seccomp, no RPC."""
import copy
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
MIB = 1024 * 1024
AS_CAP = 256 * MIB
REPEATS = 3

def memory():
    return {line.split(':')[0]: int(line.split()[1]) for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith(('MemTotal:', 'MemAvailable:'))}

def main():
    available = memory()
    if available['MemAvailable'] < 1024 * 1024:
        raise SystemExit('Insufficient available memory for bounded benchmark; no workload started')
    spec = importlib.util.spec_from_file_location('guard', ROOT / 'scripts/phase7z_regression.py')
    guard = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(guard)
    def constrain():
        guard.block_network()
        resource.setrlimit(resource.RLIMIT_AS, (AS_CAP, AS_CAP))
        resource.setrlimit(resource.RLIMIT_CPU, (5, 5))
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    epoch = datetime.datetime(2026, 11, 1, tzinfo=datetime.timezone.utc)
    def report(i=0):
        return {'schema_version':'1.0','network':'devnet','operation':'account_lookup','timestamp_utc':(epoch+datetime.timedelta(seconds=i)).isoformat(timespec='milliseconds').replace('+00:00','Z'),'status':'not_found','context':{'slot':i+1},'data':{'address':'11111111111111111111111111111111','account':None},'error':None}
    results=[]
    with tempfile.TemporaryDirectory(prefix='rialo-performance-') as directory:
        folder=Path(directory)
        def put(name, value=None, raw=None, padded=None):
            p=folder/name
            b=raw if raw is not None else json.dumps(value,separators=(',',':')).encode()
            if padded is not None:
                assert len(b)<=padded
                b+=b' '*(padded-len(b))
            p.write_bytes(b)
            return p
        def measure(name, paths, expected=0, reason=None, check=None, details=True):
            # Extra headroom check before each stage; never intentionally approach system OOM.
            if memory()['MemAvailable'] < 512*1024:
                raise RuntimeError('Available memory fell below safety threshold; stopping')
            trials=[]
            source_hashes={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in set(paths)}
            for repeat in range(REPEATS):
                metric=folder/'measurement.txt'
                cmd=['/usr/bin/time','-f','%e %M','-o',str(metric),str(ROOT/'target/debug/report_analysis'),'--json']
                if details: cmd.append('--temporal-details')
                cmd += [str(p) for p in paths]
                begin=time.perf_counter()
                p=subprocess.run(cmd,capture_output=True,timeout=12,preexec_fn=constrain)
                elapsed=time.perf_counter()-begin
                lines=metric.read_text().strip().splitlines()
                sec,rss=lines[-1].split()
                try: value=json.loads(p.stdout)
                except ValueError: value=None
                ok=p.returncode==expected and not p.stderr and isinstance(value,dict)
                if reason:ok=ok and value.get('error',{}).get('reason')==reason
                if check and ok:ok=bool(check(value))
                trials.append({'wall_seconds':round(elapsed,6),'gnu_time_seconds':float(sec),'peak_rss_kib':int(rss),'exit_code':p.returncode,'output_bytes':len(p.stdout),'output_sha256':hashlib.sha256(p.stdout).hexdigest(),'passed':ok})
                if not ok:
                    raise RuntimeError(f'Unexpected result at {name}: exit={p.returncode}; stopping further load')
            unchanged=all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in source_hashes.items())
            assert unchanged
            accepted=value.get('total_reports') if expected==0 else None
            results.append({'name':name,'input_files':len(paths),'input_total_bytes':sum(p.stat().st_size for p in paths),'expected_exit':expected,'expected_reason':reason,'trials':trials,'median_wall_seconds':statistics.median(x['wall_seconds'] for x in trials),'max_peak_rss_kib':max(x['peak_rss_kib'] for x in trials),'output_reproducible':len({x['output_sha256'] for x in trials})==1,'accepted_unique_reports':accepted,'duplicate_reports':value.get('duplicates_skipped') if expected==0 else None,'internal_json_parse_success_count':None,'internal_json_parse_failure_count':None,'parser_count_note':'UNVERIFIED: CLI has no per-file parser telemetry. Accepted counts are CLI output, not inferred parser-call instrumentation.','input_files_unchanged':unchanged,'passed':True})
        small=[put(f'small-{i}.json',report(i)) for i in range(10)]
        measure('10_reports',small,check=lambda v:v['total_reports']==10 and len(v['comparisons'])==9 and all(c['temporal']['interval_ms']==1000 for c in v['comparisons']))
        medium=[put(f'medium-{i}.json',report(i)) for i in range(100)]
        measure('100_reports_rejected',medium,2,'file_count_limit')
        large=[put(f'large-{i}.json',report(i)) for i in range(1000)]
        measure('1000_reports_rejected',large,2,'file_count_limit')
        maximum=[put(f'max-{i}.json',report(i),padded=128*1024) for i in range(64)]
        measure('maximum_64_files_8mib',maximum,check=lambda v:v['total_reports']==64 and len(v['comparisons'])==63 and all(c['temporal']['interval_ms']==1000 for c in v['comparisons']))
        measure('exact_single_1mib',[put('one-mib.json',report(),padded=MIB)],check=lambda v:v['total_reports']==1)
        measure('empty_file',[put('empty.json',raw=b'')],2,'invalid_json')
        measure('empty_json_object',[put('object.json',{})],2,'unsupported_schema')
        measure('malformed_json',[put('malformed.json',raw=b'{')],2,'invalid_json')
        missing=report();missing.pop('context')
        measure('missing_context',[put('missing.json',missing)],2,'missing_fields')
        measure('per_file_1mib_plus_one',[put('huge.json',raw=b' '*(MIB+1))],2,'file_size_limit')
        measure('65_files',[small[0]]*65,2,'file_count_limit')
        measure('total_over_8mib',[maximum[0]]*64+[maximum[1]],2,'file_count_limit')
        # At most 64 files; isolate total-byte check from count check.
        measure('total_9mib_9files',[put('total-one.json',report(),padded=MIB)]*9,2,'total_size_limit')
        measure('duplicates',[small[0]]*10,check=lambda v:v['total_reports']==1 and v['duplicates_skipped']==9)
        badtime=report();badtime['timestamp_utc']='2026-02-30T00:00:00.000Z'
        measure('invalid_timestamp',[put('badtime.json',badtime)],2,'invalid_timestamp')
        measure('reversed_order',list(reversed(small)),check=lambda v:v['comparisons'][0]['from']==report(0)['timestamp_utc'] and v['comparisons'][-1]['to']==report(9)['timestamp_utc'])
        # Deterministic second public-key-shaped synthetic identifier, no private key generation.
        alphabet='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
        def base58(data):
            n=int.from_bytes(data,'big');s=''
            while n:n,r=divmod(n,58);s=alphabet[r]+s
            return s
        other=report(1);other['data']['address']=base58(bytes([1])*32)
        measure('different_accounts',[small[0],put('other.json',other)],check=lambda v:len(v['comparisons'])==2 and all(not c['comparable'] for c in v['comparisons']))
        dense=report();dense['ignored_numeric_array']=[0]*400000
        measure('dense_json_value_under_1mib',[put('dense.json',dense)],check=lambda v:v['total_reports']==1)
        deep=report();deep['ignored_nested']=[[[0]]]
        for _ in range(32):deep['ignored_nested']=[deep['ignored_nested']]
        measure('excessive_nesting',[put('deep.json',deep)],2,'nesting_limit')
        bigint=json.dumps(report(),separators=(',',':')).replace('"slot":1','"slot":18446744073709551616').encode()
        measure('oversized_integer',[put('integer.json',raw=bigint)],2,'invalid_number')
        rows=[{'signature':base58(bytes([i%255+1])*63+bytes([i//255+1])),'slot':1,'executed':True,'error_present':False,'execution':'Succeeded'} for i in range(256)]
        points=[]
        for i in range(16):
            v=report(i);v['operation']='transaction_status';v['status']='success';v['data']={'transactions':rows}
            points.append(put(f'points-{i}.json',v))
        measure('4096_points_output_cap',points,2,'output_size_limit')
        measure('4096_points_legacy_output_cap',points,2,'output_size_limit',details=False)
    result={'date':'2026-11-01','assessment':'PARTIAL','reason':'Supported-bound workloads and all negative tests passed; 100/1000 report processing is outside max64 and was rejected, not benchmarked as successful processing. Near-OOM testing intentionally not performed.',
            'environment':{'os':platform.system(),'architecture':platform.machine(),'memory_initial_kib':available,'memory_final_kib':memory(),'cgroup_memory_limit':'UNVERIFIED: cgroup memory.max/current files unavailable','binary_profile':'dev debug build','binary_sha256':hashlib.sha256((ROOT/'target/debug/report_analysis').read_bytes()).hexdigest()},
            'safety':{'child_address_space_limit_bytes':AS_CAP,'cpu_seconds_limit':5,'wall_timeout_seconds':12,'core_dump_limit':0,'minimum_initial_mem_available_kib':1024*1024,'minimum_stage_mem_available_kib':512*1024,'network':'Inherited Linux seccomp; no RPC/HTTP; all synthetic local data'},
            'method':'GNU time peak RSS of CLI child only (KiB); Python wall time includes process launch/guard; 3 independent executions per case, deterministic output hashes compared',
            'cases':results,'new_validation_cases_passed':len(results),'new_validation_cases_failed':0,'trial_executions':len(results)*REPEATS,'max_peak_rss_kib':max(x['max_peak_rss_kib'] for x in results),'all_outputs_reproducible':all(x['output_reproducible'] for x in results),
            'unverified':['Successful processing of 100/1000 report files (unsupported)','Internal per-file JSON parser counters','Approaching actual process/host OOM','Worst-case across all JSON structures','Process-wide memory guarantee','Release-build performance','HTTP live performance'],'production_code_changes':False,'new_rpc_requests':0,'github_notion_writes':0}
    (ROOT/'docs/cdk-readonly-performance-results-2026-11-01.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({'assessment':result['assessment'],'cases':len(results),'executions':result['trial_executions'],'max_rss_kib':result['max_peak_rss_kib'],'summary':[{k:x[k] for k in ['name','input_files','input_total_bytes','median_wall_seconds','max_peak_rss_kib','expected_reason']} for x in results]},indent=2))
if __name__=='__main__':main()

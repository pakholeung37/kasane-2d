#!/usr/bin/env python3
"""Native M2 acceptance: no Godot, no scene tree, no GPU required."""
import argparse
import hashlib
import json
import math
import platform
from pathlib import Path
import subprocess
import tempfile
import time
from validate_core import source_fingerprint
ROOT = Path(__file__).resolve().parents[1]

def validate(build, base):
    base.mkdir(parents=True,exist_ok=True)
    report=dict(status='failed',scope='native project/filesystem acceptance',godot_required=False,
                git_revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
                submodules=subprocess.check_output(['git','submodule','status'],cwd=ROOT,text=True),
                source_sha256=source_fingerprint(),platform=platform.platform(),checks=[],moc_version=5,
                coordinate_units='runtime units; source pixels; pixels_per_unit=100',canvas=dict(width=640,height=480,origin=[271,193]))
    def run(command,label,expected=0):
        result=subprocess.run(list(map(str,command)),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=120)
        (base/(label+'.log')).write_text(result.stdout)
        if (result.returncode==0)!=(expected==0):raise RuntimeError(f'{label}: {result.stdout[-5000:]}')
        return result
    try:
        project_build=build/'document'
        tool=project_build/'kasane_document_tool'
        run([project_build/'kasane_document_tests',base/'unit'],'native-tests')
        report['checks'].extend(json.loads((base/'unit/report.json').read_text())['checks'])
        run([tool,'copy',ROOT/'samples/m2-complete',base/'before'],'copy')
        (base/'before/project').rename(base/'moved')
        run([tool,'inspect',base/'moved',base/'after'],'reopen')
        run([tool,'edit',base/'moved',base/'edited-before'],'edit')
        run([tool,'inspect',base/'edited-before/project',base/'edited-after'],'edited-reopen')
        for left,right in [('before','after'),('edited-before','edited-after')]:
            for filename in ('source.json','samples.json','runtime/model.moc3','runtime/model.model3.json'):
                a,b=(base/left/filename).read_bytes(),(base/right/filename).read_bytes()
                if a!=b:raise RuntimeError(f'Native roundtrip mismatch: {left}/{right}/{filename}')
                report['checks'].append(dict(name=f'{left}/{right}/{filename}',status='passed',expected=hashlib.sha256(a).hexdigest(),actual=hashlib.sha256(b).hexdigest()))
        # Actual cross-process lock exclusion, not just a simulated busy result.
        ready=base/'lock-ready.json'
        holder=subprocess.Popen(list(map(str,[tool,'hold-lock',base/'moved',ready])),stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        try:
            deadline=time.monotonic()+10
            while not ready.exists():
                if holder.poll() is not None or time.monotonic()>deadline:raise RuntimeError('Native lock holder failed')
                time.sleep(0.02)
            result=run([tool,'save',base/'moved',base/'moved'],'lock-exclusion',expected=1)
            if 'PROJECT_BUSY' not in result.stdout:raise RuntimeError('Expected a busy project diagnostic')
        finally:
            holder.communicate('\n',timeout=10)
        run([tool,'save',base/'moved',base/'moved'],'lock-release')
        report['checks'].append(dict(name='cross-process lock exclusion and release',status='passed',expected=True,actual=True))
        numeric_comparisons = []
        for provider in ('purism', 'official'):
            maximum, pixel_maximum = 0.0, 0.0
            for package, sample_file in [('before/runtime','before/samples.json'),('after/runtime','after/samples.json'),
                                         ('edited-before/runtime','edited-before/samples.json'),('edited-after/runtime','edited-after/samples.json')]:
                samples = json.loads((base/sample_file).read_text())
                input_text = str(len(samples)) + '\n' + '\n'.join(' '.join(map(str,sample['parameters'])) for sample in samples) + '\n'
                result = subprocess.run([str(build/f'kasane_document_{provider}_probe'), str(base/package/'model.moc3')],
                                        input=input_text, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60)
                (base/f'{provider}-{package.replace("/","-")}.log').write_text(result.stdout)
                if result.returncode: raise RuntimeError(f'{provider} rejected {package}: {result.stderr}')
                runtime = json.loads(next(line for line in result.stdout.splitlines() if line.startswith('{')))
                (base/f'{provider}-{package.replace("/","-")}.json').write_text(json.dumps(runtime,indent=2)+'\n')
                report[provider+'_version'] = runtime['core_version']
                if len(runtime['samples']) != len(samples): raise RuntimeError('Runtime sample count mismatch')
                for sample_index, (expected_sample, actual_sample) in enumerate(zip(samples, runtime['samples'])):
                    expected_drawables = expected_sample['drawables']
                    if len(expected_drawables) != len(actual_sample): raise RuntimeError('Drawable count mismatch')
                    ids = [d['id'] for d in expected_drawables]
                    for expected, actual in zip(expected_drawables, actual_sample):
                        for key in ('runtime_id','texture_slot','draw_order','render_order','double_sided','inverted_mask','blend_mode','indices'):
                            if expected[key] != actual[key]: raise RuntimeError(f'{provider}/{package}/{sample_index}/{expected["id"]}/{key} mismatch')
                        if [ids.index(m) for m in expected['masks']] != actual['mask_indices']: raise RuntimeError('Masks mismatch')
                        for key in ('positions','uvs','opacity','multiply_color','screen_color'):
                            def flatten(value):
                                if isinstance(value,list):
                                    return [n for child in value for n in flatten(child)]
                                return [value]
                            wanted, obtained = flatten(expected[key]), flatten(actual[key])
                            if len(wanted) != len(obtained): raise RuntimeError('Channel length mismatch')
                            for index,(a,b) in enumerate(zip(wanted,obtained)):
                                error = abs(a-b)
                                pixel_error = error*100 if key=='positions' else 0
                                passed = math.isfinite(a) and math.isfinite(b) and error<=1e-5+1e-5*max(abs(a),abs(b)) and pixel_error<=0.05
                                numeric_comparisons.append(dict(provider=provider,package=package,sample=sample_index,object=expected['id'],
                                                                field=key,index=index,expected=a,actual=b,error=error,pixel_error=pixel_error,status='passed' if passed else 'failed'))
                                maximum=max(maximum,error); pixel_maximum=max(pixel_maximum,pixel_error)
                                if not passed: raise RuntimeError(f'{provider}/{package}/{sample_index}/{expected["id"]}/{key}: {a} != {b}')
            report['checks'].append(dict(name=provider+' runtime comparison',status='passed',expected='numeric tolerance 1e-5 + 1e-5 relative; <=0.05 source pixels',
                                         actual=dict(max_error=maximum,max_pixel_error=pixel_maximum,samples=300)))
        (base/'comparisons.json').write_text(json.dumps(numeric_comparisons,indent=2)+'\n')
        report['numeric_comparisons'] = len(numeric_comparisons)
        report['status']='passed'
    except Exception as exc:
        report['error']=str(exc)
        print(exc)
    finally:
        report['files']=[dict(path=str(p.relative_to(base)),sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in sorted(base.rglob('*')) if p.is_file()]
        (base/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
    return report

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--core-build',type=Path,default=ROOT/'target/kasane/core-regression/build')
    parser.add_argument('--output-dir',type=Path,default=ROOT/'target/kasane/native-project-regression')
    args=parser.parse_args()
    args.output_dir.mkdir(parents=True,exist_ok=True)
    base=Path(tempfile.mkdtemp(prefix='run-',dir=args.output_dir)).resolve()
    report=validate(args.core_build.resolve(),base)
    (args.output_dir/'latest.json').write_text(json.dumps(dict(report=str(base/'report.json'),status=report['status']),indent=2)+'\n')
    print(f'Native project {report["status"]}: {len(report["checks"])} checks; {base / "report.json"}')
    return 0 if report['status']=='passed' else 1
if __name__=='__main__':raise SystemExit(main())

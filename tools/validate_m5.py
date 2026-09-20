#!/usr/bin/env python3
"""M5 acceptance against an independently packaged editor and both existing Core runtimes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

from editor_agent import submit, atomic_write
from validate_m3 import compare_samples, read_probe

ROOT = Path(__file__).resolve().parents[1]


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def command(args, log, *, cwd=ROOT, timeout=120):
    run = subprocess.run(list(map(str, args)), cwd=cwd, text=True, stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT, timeout=timeout)
    log.write_text(run.stdout)
    if run.returncode or 'SCRIPT ERROR:' in run.stdout or 'ERROR:' in run.stdout:
        raise RuntimeError(f'{log}: process failed ({run.returncode})\n{run.stdout[-1500:]}')
    return run.stdout


def script(body):
    return 'extends RefCounted\nfunc run(w):\n' + '\n'.join('\t' + line for line in body.splitlines()) + '\n'


def normalize_drawables(drawables):
    ids = [d['id'] for d in drawables]
    return [dict(d, mask_indices=[ids.index(x) for x in d['masks']]) for d in drawables]


def compare_images(actual, expected, output):
    # Reuse the local validation runtime when Pillow/numpy are not on PATH.
    for candidate in [ROOT/'target/buildenv/lib/python3.14/site-packages', ROOT/'target/kasane/buildenv/lib/python3.14/site-packages']:
        if candidate.is_dir():
            sys.path.insert(0, str(candidate))
    import numpy as np
    from PIL import Image
    a = np.asarray(Image.open(actual).convert('RGBA'), dtype=np.float64) / 255
    b = np.asarray(Image.open(expected).convert('RGBA'), dtype=np.float64) / 255
    if a.shape != b.shape:
        raise RuntimeError('Image dimensions differ')
    diff = np.abs(a-b)
    metrics = {'mean_absolute_error': float(diff.mean()), 'bad_pixel_fraction': float((diff.max(axis=2) > .05).mean())}
    Image.fromarray((diff*255).clip(0,255).astype('uint8'), 'RGBA').save(output)
    if metrics['mean_absolute_error'] > .005 or metrics['bad_pixel_fraction'] > .01:
        raise RuntimeError(f'GPU comparison failed: {metrics}; {output}')
    return metrics


def runtime_images(godot, package, samples, summary, output):
    output.mkdir(parents=True)
    shutil.copytree(package, output/'package')
    source = ROOT/'modules/gd-cubism/addons/gd_cubism'
    addon = output/'addons/gd_cubism'
    shutil.copytree(source/'res', addon/'res')
    framework = 'libgd_cubism.cubism.macos.release.framework'
    shutil.copytree(source/'bin'/framework, addon/'bin'/framework)
    (addon/'gd_cubism.gdextension').write_text('[configuration]\nentry_symbol="gd_cubism_library_init"\ncompatibility_minimum="4.3"\n[libraries]\nmacos.debug.arm64="res://addons/gd_cubism/bin/'+framework+'"\n')
    (output/'.godot').mkdir()
    (output/'.godot/extension_list.cfg').write_text('res://addons/gd_cubism/gd_cubism.gdextension\n')
    (output/'project.godot').write_text('config_version=5\n[application]\nconfig/name="M5 independent runtime"\n[rendering]\nrenderer/rendering_method="gl_compatibility"\n')
    shutil.copy2(ROOT/'tests/editor_runtime_capture.gd', output/'capture.gd')
    for i, sample in enumerate(samples):
        sample['reference_path'] = str(output/f'reference-{i}.png')
    config = {'size': samples[0]['observation']['image_size'], 'origin': summary['canvas_origin'], 'samples': samples}
    config_path = output/'capture.json'
    config_path.write_text(json.dumps(config))
    # Texture import runs in the real renderer; the known headless import shutdown
    # bug is avoided by requesting a bounded editor run, then validating the files.
    command([godot, '--path', output, '--editor', '--import'], output/'import.log')
    command([godot, '--path', output, '--script', 'res://capture.gd', '--', config_path], output/'runtime.log')
    return [compare_images(s['observation']['path'], s['reference_path'], output/f'difference-{i}.png') for i,s in enumerate(samples)]


def exercise(directory, output, flow, godot, probes):
    output.mkdir(parents=True)
    def send(source, observe=False, **kwargs):
        result = submit(directory, source, observe=observe, timeout=90, **kwargs)
        if not result.get('ok'):
            raise RuntimeError(json.dumps({k:v for k,v in result.items() if k!='business'},ensure_ascii=False) + '\nBusiness failure: ' + str([r for r in result.get('business',{}).get('operations',[]) if not r.get('ok')])[:1000])
        return result
    if flow == 'png':
        source = (ROOT/'tests/editor_author_png.gd').read_text().replace('__OUTPUT__', str(output))
    else:
        source = (ROOT/'tests/editor_edit_external.gd').read_text().replace('__MODEL__', str(output.parent/'external-input/model.model3.json'))
    authored = send(source, True)
    (output/'authored.json').write_text(json.dumps(authored, indent=2))
    summary = authored['business']['summary']
    if flow == 'png':
        mesh_id = authored['business']['objects'][0]['mesh']
    else:
        mesh_id = authored['business']['mesh_id']
    # A second script consumes IDs returned by the first, preserving the model.
    second = send(script(f'return w.document.rename_mesh({json.dumps(mesh_id)}, "Continued by second script")'))
    (output/'continued.json').write_text(json.dumps(second, indent=2))
    params = summary['parameters']
    defaults = [p['default_value'] for p in params]
    selections = [defaults]
    for i, parameter in enumerate(params):
        keys = {parameter['minimum'], parameter['maximum'], parameter['default_value']}
        for binding in summary['bindings'] + summary['scene_bindings']:
            for axis in binding['axes']:
                if axis['parameter_id'] == parameter['id']:
                    keys.update(axis['keys'])
        keys = sorted(keys)
        keys = sorted(set(keys + [(a+b)/2 for a,b in zip(keys,keys[1:])]))
        for value in keys:
            values = defaults.copy(); values[i] = value
            if values not in selections:
                selections.append(values)
    # Capture all numeric samples; GPU covers default, both extrema and midpoints
    # for newly authored parameters, plus the edited external parameter.
    samples = []
    for values in selections:
        mapping = {p['id']: value for p,value in zip(params,values)}
        source = script('var result = w.document.set_preview_values('+json.dumps(mapping)+')\nreturn result')
        result = send(source)
        samples.append({'values': values, 'frame': result['business']})
    image_values = [defaults]
    index = 0 if flow == 'png' else len(params)-1
    for value in [-1, -.5, .5, 1]:
        values = defaults.copy(); values[index] = value
        if values not in image_values: image_values.append(values)
    observations = []
    crop_result = send(script('return {"ok": true}'), True, object_id=mesh_id)
    full_result = send(script('return {"ok": true}'), True)
    for candidate in [ROOT/'target/buildenv/lib/python3.14/site-packages', ROOT/'target/kasane/buildenv/lib/python3.14/site-packages']:
        if candidate.is_dir(): sys.path.insert(0,str(candidate))
    from PIL import Image
    crop = crop_result['observation']['crop']
    with Image.open(full_result['observation']['path']) as image:
        image.crop((crop[0],crop[1],crop[0]+crop[2],crop[1]+crop[3])).save(output/'expected-crop.png')
    crop_metrics = compare_images(crop_result['observation']['path'], output/'expected-crop.png', output/'crop-difference.png')
    (output/'crop.json').write_text(json.dumps(crop_result,indent=2))
    for values in image_values:
        mapping = {p['id']: value for p,value in zip(params,values)}
        result = send(script('var frame = w.document.set_preview_values('+json.dumps(mapping)+')\nreturn frame'), True)
        observations.append({'values': values, 'observation': result['observation']})
    # Return to default before the save/reopen equivalence comparison.
    mapping = {p['id']: p['default_value'] for p in params}
    project = output/'project.json'; package = output/'package'
    result = send(script('w.document.set_preview_values('+json.dumps(mapping)+')\nvar before = w.document.get_frame()\nvar saved = w.save_project('+json.dumps(str(project))+')\nif not saved.ok: return saved\nvar reopened = w.open_project('+json.dumps(str(project))+')\nif not reopened.ok: return reopened\nvar after = w.document.get_frame()\nvar exported = w.export_model('+json.dumps(str(package))+')\nreturn {"ok": exported.ok, "before": before, "after": after, "export": exported, "summary": w.document.get_document_summary()}'))
    (output/'roundtrip.json').write_text(json.dumps(result,indent=2))
    a = normalize_drawables(result['business']['before']['drawables']); b = normalize_drawables(result['business']['after']['drawables'])
    roundtrip = compare_samples([a], [b], summary['pixels_per_unit'], flow+'/save-reopen')
    model = json.loads((package/'model.model3.json').read_text())
    expected = [normalize_drawables(s['frame']['drawables']) for s in samples]
    core = {}
    for provider in ['official','purism']:
        probe = probes/f'kasane_document_{provider}_probe'
        actual = read_probe(probe, package/model['FileReferences']['Moc'], selections, output/f'{provider}.json')
        core[provider] = dict(compare_samples(expected, actual['samples'], summary['pixels_per_unit'], flow+'/'+provider), core_version=actual['core_version'])
    (output/'samples.json').write_text(json.dumps(samples))
    (output/'observations.json').write_text(json.dumps(observations,indent=2))
    # Import/capture uses a separate player process with no editor running logic.
    return {'status': 'passed', 'samples': len(samples), 'roundtrip': roundtrip, 'cores': core,
            'gpu': {'status': 'not_run'}, 'crop':crop_metrics, 'summary': summary,
            'report_directory': str(output), 'inputs': [{'path': str(p), 'sha256': sha(p)} for p in output.glob('layer-*.png')]}


def protocol(directory, output):
    output.mkdir()
    checks = []
    def check(name, condition):
        checks.append({'name':name,'expected':True,'actual':bool(condition),'status':'passed' if condition else 'failed'})
    status = json.loads((directory/'status.json').read_text())
    generation = status['generation']
    once = script('var id=w.new_id()\nvar edit=w.document.create_rotation(id,"Exactly once",Vector2.ZERO,0)\nreturn {"ok":edit.ok,"id":id}')
    result = submit(directory, once)
    duplicate = submit(directory, once, execution_id=result['id'])
    check('duplicate ID returns original result', duplicate == result)
    bad = submit(directory, 'extends RefCounted\nfunc run(w)\n return {}\n')
    check('compile failure has line evidence and no writes', bad.get('code') == 'COMPILE_ERROR' and not bad['executed'] and bad['errors'][0]['line'] > 0 and bad['start_revision'] == bad['end_revision'])
    runtime = submit(directory, script('var id = w.new_id()\nw.document.create_rotation(id,"Partial",Vector2.ZERO,0)\nvar invalid = {}\ninvalid.missing()\nreturn {"ok":true}'))
    check('packaged runtime failure retains writes and line evidence', runtime.get('code') == 'RUNTIME_ERROR' and runtime['executed'] and runtime['end_revision'] > runtime['start_revision'] and runtime['errors'][0]['line'] == 6)
    request_id = str(uuid.uuid4())
    source = directory/'scripts'/f'{request_id}.gd'; source.write_text(script('return w.new_project()'))
    atomic_write(directory/'requests'/f'{request_id}.json', json.dumps({'id':request_id,'app_id':status['app_id'],'generation':generation-1,'script_path':str(source)}))
    deadline = time.monotonic()+10
    result_path = directory/'results'/f'{request_id}.json'
    while not result_path.exists() and time.monotonic()<deadline: time.sleep(.1)
    stale = json.loads(result_path.read_text())
    check('old generation cannot target current document', stale['code']=='STALE_DOCUMENT' and not stale['executed'])
    interrupted_id = str(uuid.uuid4())
    atomic_write(directory/'claims'/f'{interrupted_id}.json', json.dumps({'id':interrupted_id,'app_id':status['app_id'],'generation':generation}))
    interrupted = submit(directory, script('return w.new_project()'), execution_id=interrupted_id)
    check('interrupted claim is not replayed', interrupted['code']=='EXECUTION_OUTCOME_UNKNOWN' and interrupted['executed'] is None and interrupted['start_revision']==interrupted['end_revision'])
    missing = submit(directory, script('return {"ok":true}'), observe=True, object_id=str(uuid.uuid4()))
    check('missing object cannot produce stale observation', not missing['ok'] and missing.get('phase')=='observation' and missing['observation']['code']=='NO_DRAWABLE' and 'path' not in missing['observation'])
    unwritable_id = str(uuid.uuid4())
    (directory/'observations'/f'{unwritable_id}.png').mkdir()
    unwritable = submit(directory, script('return {"ok":true}'), observe=True, execution_id=unwritable_id)
    check('image write failure is explicit', not unwritable['ok'] and unwritable.get('phase')=='observation' and unwritable['code']=='IMAGE_WRITE_FAILED')
    locking = submit(directory,script('var other=load("res://agent_queue.gd").new()\nvar surface=w.surface.get_ref()\nsurface.add_child(other)\nvar result=other.start(w,surface,'+json.dumps(str(directory))+')\nother.free()\nreturn result'))
    check('second consumer cannot share queue', locking.get('code')=='QUEUE_IN_USE')
    report = {'status':'passed' if all(c['actual'] for c in checks) else 'failed','checks':checks,'results':[bad,runtime,stale,interrupted,missing,unwritable,locking]}
    (output/'report.json').write_text(json.dumps(report,indent=2))
    return report


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--application',type=Path,required=True,help='Exported macOS zip')
    p.add_argument('--godot',type=Path,default=ROOT/'target/godot-tools/standard/Godot.app/Contents/MacOS/Godot')
    p.add_argument('--probes',type=Path,default=ROOT/'target/probes')
    p.add_argument('--output-dir',type=Path,default=ROOT/'target/kasane/m5')
    p.add_argument('--agent-review',type=Path,default=ROOT/'target/kasane/m5-agent-review/review.json')
    args=p.parse_args(); output=args.output_dir.resolve(); output.mkdir(parents=True,exist_ok=True)
    standalone=Path(tempfile.mkdtemp(prefix='kasane-m5-acceptance-'))
    report={'milestone':'M5','status':'failed','application':str(args.application.resolve()),'sha256':sha(args.application),
            'standalone_directory':str(standalone),'git_revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
            'git_status':subprocess.check_output(['git','status','--short'],cwd=ROOT,text=True),
            'git_submodules':subprocess.check_output(['git','submodule','status'],cwd=ROOT,text=True),
            'platform':sys.platform,'build_config':'cargo build --release -p kasane-godot; Godot export-debug (required runtime script diagnostics)',
            'gates':{name:{'status':'not_run'} for name in ['application','png','external','protocol','contracts','agent_review']}}
    process=None
    try:
        subprocess.run(['ditto','-x','-k',str(args.application.resolve()),str(standalone)],check=True)
        executable=next((next(standalone.glob('*.app'))/'Contents/MacOS').iterdir())
        (standalone/'home').mkdir()
        external=standalone/'external-input'; external.mkdir()
        source=ROOT/'modules/purism-core/testdata/moc3/3d8e869a678a1dac.moc3'
        shutil.copy2(source,external/'model.moc3')
        shutil.copy2(ROOT/'tests/fixtures/gpu/gpu-package/textures/0.png',external/'texture.png')
        (external/'model.model3.json').write_text(json.dumps({'Version':3,'FileReferences':{'Moc':'model.moc3','Textures':['texture.png']}}))
        report['external_input']={'path':str(source),'sha256':sha(source),'moc_version':source.read_bytes()[4]}
        env={k:v for k,v in os.environ.items() if not k.startswith(('GODOT','DYLD','CARGO','RUST','DOTNET'))}
        env.update(HOME=str(standalone/'home'),PATH='/usr/bin:/bin')
        with (output/'application.log').open('w') as log:
            process=subprocess.Popen([str(executable),'--','--agent-dir='+str(standalone/'agent')],cwd=standalone,env=env,stdout=log,stderr=subprocess.STDOUT)
            deadline=time.monotonic()+30
            while not (standalone/'agent/status.json').exists():
                if process.poll() is not None or time.monotonic()>deadline: raise RuntimeError('Packaged app did not start')
                time.sleep(.1)
            report['gates']['application']={'status':'passed','executable':str(executable),'cwd':str(standalone),'PATH':env['PATH'],'HOME':env['HOME'],'native_libraries':[str(p) for p in standalone.rglob('*.dylib')]}
            report['engine']=json.loads((standalone/'agent/status.json').read_text())['engine']
            for flow in ['png','external']:
                report['gates'][flow]=exercise(standalone/'agent',standalone/flow,flow,args.godot.resolve(),args.probes.resolve())
                (output/'report.json').write_text(json.dumps(report,indent=2))
            report['gates']['protocol']=protocol(standalone/'agent',standalone/'protocol')
        contract_dir=standalone/'contracts'; contract_dir.mkdir()
        contract_source=(ROOT/'tests/editor_contract.gd').read_text()
        contract_source += '\nfunc run(workspace):\n\tvar separate_host = load("res://script_host.gd").new(workspace)\n\tvar application = workspace.surface.get_ref().get_tree().root.get_child(0)\n\tvar report = verify(workspace, application, ' + json.dumps(str(contract_dir)) + ', separate_host)\n\tseparate_host.close()\n\treturn {"ok": report.status == "passed", "report": report}\n'
        contract_result=submit(standalone/'agent',contract_source,timeout=90)
        (output/'contracts-result.json').write_text(json.dumps(contract_result,indent=2))
        contract=json.loads((contract_dir/'editor-contract-report.json').read_text())
        if contract.get('status')!='passed' or contract_result.get('code')!='RUNTIME_ERROR' or len(contract_result.get('errors',[]))!=2:
            raise RuntimeError('Packaged application contract checks failed; see contracts-result.json')
        report['gates']['contracts']={'status':'passed','checks':contract['checks'],'report':str(contract_dir/'editor-contract-report.json')}
        process.terminate()
        process.wait(timeout=5)
        report['editor_stopped_before_runtime_capture'] = True
        for flow in ['png','external']:
            gate=report['gates'][flow]
            observations=json.loads((standalone/flow/'observations.json').read_text())
            gate['gpu']={'status':'passed','comparisons':runtime_images(args.godot.resolve(),standalone/flow/'package',observations,gate.pop('summary'),standalone/flow/'runtime')}
        if args.agent_review.is_file():
            review=json.loads(args.agent_review.read_text())
            if review.get('kind')=='interactive_agent_review' and len(review.get('evidence',[]))>=2 and all(Path(e['image']).is_file() and Path(e['result']).is_file() for e in review['evidence']):
                report['gates']['agent_review']={'status':'passed','record':str(args.agent_review.resolve()),'sha256':sha(args.agent_review),'provenance':'separate interactive review; not generated by this validator'}
        report['status']='passed' if all(g['status']=='passed' for g in report['gates'].values()) else 'failed'
    except Exception as error:
        report['error']=str(error)
        print(error,file=sys.stderr)
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
        (output/'report.json').write_text(json.dumps(report,indent=2))
    print(json.dumps({'status':report['status'],'report':str(output/'report.json'),'artifacts':str(standalone)}))
    return 0 if report['status']=='passed' else 1


if __name__=='__main__':
    raise SystemExit(main())

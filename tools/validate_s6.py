#!/usr/bin/env python3
"""S6 real-GPU acceptance against the pinned official Native Framework."""
import argparse
import acceptance_evidence as evidence
import hashlib
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
PYTHON_BUNDLE = Path.home()/'.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3'
GODOT = Path('/Applications/Godot_mono.app/Contents/MacOS/Godot')
SDK = ROOT/'third_party/CubismSdkForNative-5-r.5'
REQUIRED_GATES = ('build', 'blend_matrix', 'destination_copy', 'surface_lifecycle',
                  'ren_resources', 'm4_data', 'm4_gpu', 'm3b_numerical', 'editor_official')


def run(command, log, timeout=180, memory=None):
    with log.open('w') as stream:
        process = subprocess.Popen(list(map(str, command)), cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
        try:
            if memory is None:
                process.wait(timeout=timeout)
            else:
                started = time.monotonic()
                while process.poll() is None:
                    if time.monotonic()-started > timeout:
                        raise subprocess.TimeoutExpired(command, timeout)
                    sample = subprocess.run(['ps', '-o', 'rss=', '-p', str(process.pid)], capture_output=True, text=True)
                    if sample.stdout.strip():
                        memory.append({'seconds': time.monotonic()-started, 'rss_bytes': int(sample.stdout)*1024})
                    time.sleep(.1)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
    text = log.read_text()
    if process.returncode or 'SCRIPT ERROR:' in text or 'ERROR:' in text:
        raise RuntimeError(str(log)+':\n'+text[-3500:])
    return text


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def compare_images(reference, actual, output, regions=True, object_regions=()):
    import numpy as np
    from PIL import Image
    a = np.asarray(Image.open(reference).convert('RGBA'),dtype=float)/255
    b = np.asarray(Image.open(actual).convert('RGBA'),dtype=float)/255
    if a.shape != b.shape: raise RuntimeError(f'Image dimensions differ: {a.shape}, {b.shape}')
    diff = np.abs(a-b)
    Image.fromarray(np.uint8(np.rint(diff*255))).save(output)
    areas = [('whole',diff)]
    if regions:
        ys,xs = np.where(np.maximum(a[:,:,3],b[:,:,3])>.01)
        if not len(xs): raise RuntimeError('Empty render')
        x0,x1,y0,y1=int(xs.min()),int(xs.max())+1,int(ys.min()),int(ys.max())+1
        areas.append(('foreground',diff[y0:y1,x0:x1]))
        for i,name in enumerate(['upper','middle','lower']):
            lo=y0+(y1-y0)*i//3; hi=y0+(y1-y0)*(i+1)//3
            areas.append((name,diff[lo:hi,x0:x1]))
    for name,(x0,y0,x1,y1) in object_regions:
        areas.append((name,diff[y0:y1,x0:x1]))
    checks=[]
    for name,data in areas:
        mean=float(data.mean());bad=float((data.max(2)>.05).mean())
        checks.append(dict(region=name,mean_absolute_error=mean,bad_pixel_fraction=bad,maximum=float(data.max()),status='passed' if mean<=.005 and bad<=.01 else 'failed'))
    return dict(status='passed' if all(c['status']=='passed' for c in checks) else 'failed',reference=str(reference),actual=str(actual),difference=str(output),checks=checks)


def offscreen_regions(frame, observation):
    """Each visible surface gets its own evaluated geometry ROI, including pupils.

    Large whole-body averages can otherwise hide completely missing eye groups.
    Use model geometry, never the difference image, to select these regions.
    """
    import math
    groups = {g['id']: g for g in frame['offscreens']}
    meshes = {d['id']: d for d in frame['drawables']}
    points = {id: [] for id in groups}
    stack = []
    for command in frame['render_plan']:
        kind = command['command']
        if kind == 'begin_offscreen': stack.append(command['id'])
        elif kind == 'end_offscreen': stack.pop()
        else:
            mesh = meshes[command['id']]
            if mesh['visible'] and mesh['opacity'] > 0 and mesh['indices'] and all(groups[id]['enabled'] and groups[id]['opacity'] > 0 for id in stack):
                for id in stack: points[id].extend(mesh['positions'])
    canvas = frame['canvas']; width,height = observation['image_size']
    zoom = observation['camera']['zoom']; ox,oy = observation['camera']['offset']
    cx,cy = canvas['origin']; ppu = canvas['pixels_per_unit']
    regions = []
    for id,vertices in points.items():
        if not vertices: continue
        xs = [width/2+ox+(p[0]*ppu+cx)*zoom for p in vertices]
        ys = [height/2+oy+(cy-p[1]*ppu)*zoom for p in vertices]
        box = (max(0,math.floor(min(xs))-2),max(0,math.floor(min(ys))-2),
               min(width,math.ceil(max(xs))+2),min(height,math.ceil(max(ys))+2))
        if box[2]>box[0] and box[3]>box[1]: regions.append((groups[id]['runtime_id'],box))
    return regions


def matrix_cases(output):
    # Byte-exact RGBA8 inputs: neutral/chromatic, burn/dodge endpoints, zero,
    # partial and full alpha; opacity and normal/inverted mask coverage.
    inputs=[([51,153,230,128],[204,77,26,102],1,1),
            ([230,51,153,204],[26,179,77,255],.4,1),
            ([0,255,128,255],[255,128,0,128],1,1),
            ([128,128,128,255],[128,128,128,255],1,1),
            ([0,0,0,255],[255,255,255,255],1,1),
            ([255,255,255,255],[0,0,0,255],1,1),
            ([240,80,160,0],[80,240,160,128],1,1),
            ([240,80,160,128],[0,0,0,0],1,1),
            ([51,153,230,204],[204,77,26,102],1,128/255),
            ([230,51,153,128],[26,179,77,204],.4,128/255),
            ([51,153,230,128],[204,77,26,102],1,0),
            ([230,51,153,204],[26,179,77,255],.4,0)]
    samples=[]
    for index,(src,dst,opacity,mask) in enumerate(inputs):
        for premul in (0,1):
            source=[round(c*src[3]/255)/255 if premul else c/255 for c in src[:3]]+[src[3]/255]
            destination=[round(c*dst[3]/255)/255 for c in dst[:3]]+[dst[3]/255]
            samples.append(dict(source=source,destination=destination,opacity=opacity,mask=mask,premultiplied=premul,inverted=index%2==1))
    (output/'matrix-cases.json').write_text(json.dumps(samples,indent=2))
    (output/'matrix-cases.txt').write_text(str(len(samples))+'\n'+'\n'.join(' '.join(map(str,s['source']+s['destination']+[s['opacity'],s['mask'],s['premultiplied']])) for s in samples)+'\n')
    return samples


def matrix_gate(output, probe, godot, sdk):
    samples=matrix_cases(output)
    run([probe,sdk/'Framework/src/Rendering/OpenGL/Shaders/Standard','--matrix',output/'matrix-cases.txt',output/'matrix-reference.png'],output/'matrix-reference.log')
    run([godot,'--path',ROOT/'tests','--rendering-method','gl_compatibility','--script','res://m3c_blend_matrix.gd','--',output/'matrix-cases.json',output/'matrix-actual.png'],output/'matrix-actual.log')
    result=compare_images(output/'matrix-reference.png',output/'matrix-actual.png',output/'matrix-difference.png',False)
    # Every pair/sample must meet the same thresholds; never average failures
    # away across the 90 mode combinations or the two source representations.
    import numpy as np
    from PIL import Image
    a=np.asarray(Image.open(output/'matrix-reference.png')).astype(float)/255
    b=np.asarray(Image.open(output/'matrix-actual.png')).astype(float)/255
    result['cases']=[]
    for color in range(18):
        for alpha in range(5):
            row=color*5+alpha
            for i,s in enumerate(samples):
                d=np.abs(a[row*8:(row+1)*8,i*8:(i+1)*8]-b[row*8:(row+1)*8,i*8:(i+1)*8])
                mean=float(d.mean());bad=float((d.max(2)>.05).mean())
                result['cases'].append(dict(color=color,alpha=alpha,sample=i,source='offscreen' if s['premultiplied'] else 'mesh',mean=mean,bad=bad,status='passed' if mean<=.005 and bad<=.01 else 'failed'))
    result['status']='passed' if all(c['status']=='passed' for c in result['cases']) else 'failed'
    return result


def script(body):
    return 'extends RefCounted\nfunc run(w) -> Dictionary:\n' + '\n'.join('\t'+line.replace('    ', '\t') for line in body.splitlines()) + '\n'


def editor_gate(output, probe, godot, sdk):
    from editor_agent import submit
    stage=output/'editor';agent=output/'agent'
    for generated in [agent, output/'saved', output/'exported', output/'mao-saved', output/'mao-exported', output/'joint-exported']:
        if generated.exists(): shutil.rmtree(generated)
    shutil.copytree(ROOT/'apps/editor',stage,dirs_exist_ok=True,ignore=shutil.ignore_patterns('.godot','native'))
    (stage/'native').mkdir(exist_ok=True)
    shutil.copy2(ROOT/'target/debug/libkasane_godot.dylib',stage/'native/libkasane_godot.dylib')
    (stage/'.godot').mkdir(exist_ok=True);(stage/'.godot/extension_list.cfg').write_text('res://kasane.gdextension\n')
    source=output/'source';shutil.copytree(sdk/'Samples/Resources/Ren',source,dirs_exist_ok=True)
    comparisons=[]; workflow=[]
    log=(output/'editor.log').open('w')
    process=subprocess.Popen([str(godot),'--path',str(stage),'--resolution','1600x1200','--','--agent-dir='+str(agent)],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT)
    try:
        deadline=time.monotonic()+30
        while not (agent/'status.json').exists():
            if process.poll() is not None or time.monotonic()>deadline: raise RuntimeError('Editor failed to start')
            time.sleep(.1)
        def send(name,body,observe=False):
            result=submit(agent,body if body.startswith("extends ") else script(body),observe=observe,timeout=120)
            (output/(name+'.json')).write_text(json.dumps(result,indent=2))
            if not result.get('ok') or not result.get('business',{}).get('ok',True): raise RuntimeError(name+': '+str(result)[:2500])
            workflow.append(name);return result
        def compare(name,result,moc,atlas):
            o=result['observation'];width,height=o['image_size'];camera=o['camera'];ox,oy=camera['offset']
            path=output/(name+'-reference.png')
            command=[probe,sdk/'Framework/src/Rendering/OpenGL/Shaders/Standard',moc,atlas,path,f'{width}x{height}',1,'--camera',width/2+ox,height/2+oy,camera['zoom']]
            for index,param in enumerate(o['parameters']): command += [index,param['value']]
            run(command,output/(name+'-reference.log'))
            frame=send(name+'-geometry','return w.document.get_frame()')['business']
            assert frame['revision'] == o['revision'] and frame['parameters'] == o['parameters'], 'State changed before ROI evaluation'
            rois=offscreen_regions(frame,o)
            c=compare_images(path,o['path'],output/(name+'-difference.png'),object_regions=rois);c['object_regions']=rois;comparisons.append(c)
            (output/(name+'-comparison.json')).write_text(json.dumps(c,indent=2))
        send('editor-test-input', 'w.surface.get_ref().get_tree().root.set_disable_input(true)\nreturn {"ok":true}')
        result=send('editor-import',f'var r = w.import_model({json.dumps(str(source/"Ren.model3.json"))})\nif not r.ok: return r\nw.fit_view()\nreturn {{"ok": true, "summary":w.document.get_document_summary(), "stats":w.surface.get_ref().preview.get_render_stats()}}',True)
        summary=result['business']['summary']; assert len(summary['offscreens'])==24
        original_moc=sdk/'Samples/Resources/Ren/Ren.moc3';original_atlas=sdk/'Samples/Resources/Ren/Ren.2048/texture_00.png'
        compare('editor-import',result,original_moc,original_atlas)
        parameter=next(p for p in summary['parameters'] if p['runtime_id']=='ParamAngleX')
        result=send('editor-parameter',f'return w.document.set_preview_values({{{json.dumps(parameter["id"])}: 15.0}})',True)
        compare('editor-parameter',result,original_moc,original_atlas)
        send('editor-reset','return w.document.set_preview_values({})')
        joint = send('editor-joint-keyform-edit', (ROOT/'tests/editor_m3c_joint_edit.gd').read_text())
        joint_package = output/'joint-exported'
        send('editor-joint-export', f'return w.export_model({json.dumps(str(joint_package))})')
        joint_model = json.loads(next(joint_package.glob('*.model3.json')).read_text())['FileReferences']
        for index, value in enumerate(joint['business']['samples']):
            name = 'editor-joint-sample-' + str(index)
            sample = send(name, f'return w.document.set_preview_values({{{json.dumps(joint["business"]["parameter_id"])}: {value}}})', True)
            compare(name, sample, joint_package/joint_model['Moc'], joint_package/joint_model['Textures'][0])
        send('editor-joint-restore', f'var r=w.import_model({json.dumps(str(source/"Ren.model3.json"))})\nif not r.ok: return r\nw.fit_view()\nreturn w.document.set_preview_values({{}})')
        result=send('editor-edit',\
"""var d = w.document
var before: Dictionary = d.get_document_summary()
var value: Dictionary = {}
for item in before.offscreens:
    if item.runtime_id == "Offscreen_PartFace": value = item.duplicate(true)
if value.is_empty(): return {"ok":false,"message":"face missing"}
var results = [w.begin_action("S6 Offscreen edit")]
for keyform in value.keyforms:
    keyform.opacity *= 0.8
    keyform.multiply = [0.9, 0.95, 1.0]
results.append(d.write_offscreen(value, true))
var ended: Dictionary = w.end_action()
var edited: Dictionary = d.get_document_summary()
var denied_undo: Dictionary = w.undo()
var denied_redo: Dictionary = w.redo()
var history_barrier_ok: bool = ended.code == "NO_ACTION" and denied_undo.code == "NO_UNDO" and denied_redo.code == "NO_REDO" and edited == d.get_document_summary()
var application = w.surface.get_ref().get_tree().root.get_child(0)
application._on_object_selected(value.id)
var inspect_ok: bool = application.inspector_dock.id_val.text == value.id
application._on_object_selected("")
return {"ok": results.all(func(r): return r.ok) and history_barrier_ok and inspect_ok, "history_barrier_ok":history_barrier_ok,"inspect_ok":inspect_ok,"edited":value,"summary":d.get_document_summary()}""",True)
        project=output/'saved/project.json';package=output/'exported'
        send('editor-save',f'return w.save_project({json.dumps(str(project))})')
        shutil.rmtree(source)
        reopened=send('editor-reopen',f'var r=w.open_project({json.dumps(str(project))})\nif not r.ok: return r\nw.fit_view()\nreturn {{"ok":true,"summary":w.document.get_document_summary()}}',True)
        assert reopened['business']['summary']['offscreens']==result['business']['summary']['offscreens']
        send('editor-export',f'return w.export_model({json.dumps(str(package))})')
        model3=next(package.glob('*.model3.json'));model=json.loads(model3.read_text())['FileReferences']
        compare('editor-edited-reopened',reopened,package/model['Moc'],package/model['Textures'][0])
        parameters=reopened['business']['summary']['parameters'];parameter=next(p for p in parameters if p['runtime_id']=='ParamAngleX')
        sampled=send('editor-edited-parameter',f'return w.document.set_preview_values({{{json.dumps(parameter["id"])}: -15.0}})',True)
        compare('editor-edited-parameter',sampled,package/model['Moc'],package/model['Textures'][0])
        # M3B's existing semantic-edit script runs in the same current Editor.
        mao_source=output/'mao-source'
        shutil.copytree(ROOT/'demos/gd-cubism-demo/assets/live2d/mao/runtime',mao_source,dirs_exist_ok=True)
        mao=send('mao-import',f'var r=w.import_model({json.dumps(str(mao_source/"mao_pro.model3.json"))})\nif not r.ok: return r\nw.fit_view()\nreturn {{"ok":true,"summary":w.document.get_document_summary()}}',True)
        compare('mao-import',mao,mao_source/'mao_pro.moc3',mao_source/'mao_pro.4096/texture_00.png')
        edits=send('mao-semantic-edits',(ROOT/'tests/editor_m3b_features.gd').read_text(),True)
        mao_project=output/'mao-saved/project.json';mao_package=output/'mao-exported'
        send('mao-save',f'return w.save_project({json.dumps(str(mao_project))})')
        shutil.rmtree(mao_source)
        mao=send('mao-detached-reopen',f'var r=w.open_project({json.dumps(str(mao_project))})\nif not r.ok: return r\nw.fit_view()\nreturn {{"ok":true,"summary":w.document.get_document_summary()}}',True)
        for field in ('blend_key_tables', 'blend_constraints', 'blend_bindings', 'glues', 'parameters'):
            assert mao['business']['summary'][field] == edits['business']['summary'][field], field+' changed after detached reopen'
        send('mao-export',f'return w.export_model({json.dumps(str(mao_package))})')
        mao_model=json.loads(next(mao_package.glob('*.model3.json')).read_text())['FileReferences']
        compare('mao-edited-reopened',mao,mao_package/mao_model['Moc'],mao_package/mao_model['Textures'][0])
        mao=send('mao-edited-parameter',f'return w.document.set_preview_values({{{json.dumps(edits["business"]["glue_parameter"])}: 0.65}})',True)
        compare('mao-edited-parameter',mao,mao_package/mao_model['Moc'],mao_package/mao_model['Textures'][0])
        return dict(status='passed' if all(c['status']=='passed' for c in comparisons) else 'failed',workflow=workflow,comparisons=comparisons,exported_moc_sha256=sha(package/model['Moc']),library_sha256=sha(stage/'native/libkasane_godot.dylib'),source_detached=not source.exists())
    finally:
        process.terminate()
        try:process.wait(timeout=10)
        except subprocess.TimeoutExpired:process.kill();process.wait()
        log.close()


def all_gates_passed(report):
    """Missing, skipped or a successful subset never satisfies S6."""
    gates = report.get('gates', {})
    return all(gates.get(name, {}).get('status') == 'passed' for name in REQUIRED_GATES)


def source_manifest(sdk):
    paths = []
    for directory in ('modules/kasane-core', 'modules/kasane-moc3', 'modules/kasane-project',
                      'modules/kasane-godot', 'apps/editor', 'tools', 'tests'):
        paths.extend(p for p in (ROOT/directory).rglob('*')
                     if p.is_file() and p.suffix in ('.rs', '.gd', '.gdshader', '.gdshaderinc', '.py', '.cpp', '.hpp', '.toml', '.godot', '.gdextension')
                     and not any(part in ('.godot', '__pycache__', 'native') for part in p.parts))
    paths.extend([ROOT/'Cargo.lock', ROOT/'tools/probes/CMakeLists.txt'])
    paths.extend(p for p in (sdk/'Framework/src').rglob('*') if p.is_file())
    paths.extend([sdk/'Core/lib/macos/arm64/libLive2DCubismCore.a', sdk/'Samples/Resources/Ren/Ren.moc3'])
    for directory in (sdk/'Samples/Resources/Ren', ROOT/'demos/gd-cubism-demo/assets/live2d/mao/runtime'):
        paths.extend(p for p in directory.rglob('*') if p.is_file() and p.suffix in ('.moc3', '.json', '.png'))
    return {str(p.relative_to(ROOT)) if p.is_relative_to(ROOT) else str(p): sha(p) for p in sorted(set(paths))}


def acceptance(output, sdk, godot, matrix_only=False):
    output.mkdir(parents=True, exist_ok=True)
    report_path = output/('matrix-report.json' if matrix_only else 'report.json')
    names = ('build', 'blend_matrix') if matrix_only else REQUIRED_GATES
    report = {'stage': 'S6-matrix' if matrix_only else 'S6', 'status': 'failed',
              'gates': {name: {'status': 'not_run'} for name in names},
              'checks': [evidence.missing(name, 'gpu' if name not in ('build','m4_data','m3b_numerical') else 'runtime', 'Not executed') for name in names],
              'thresholds': {'rgba_mae': .005, 'bad_pixel_fraction': .01, 'bad_pixel_threshold': .05},
              'system': {'platform': platform.platform(), 'godot': str(godot)},
              'editor_configuration': 'Current apps/editor with current debug GDExtension; standalone packaging belongs to S7'}
    def persist():
        report_path.write_text(json.dumps(report, indent=2)+'\n')
    def gate(name, operation):
        print('S6: '+name, flush=True)
        started = time.monotonic()
        try:
            result = operation()
            result['elapsed_seconds'] = time.monotonic()-started
            report['gates'][name] = result
            evidence_path = output / ('evidence-' + name + '.json')
            evidence_path.write_text(json.dumps(result, indent=2) + '\n')
            artifacts = [evidence.artifact(evidence_path, 'gate_result')]
            def collect(value):
                if isinstance(value, dict):
                    for key, child in value.items():
                        if key in ('report', 'reference', 'actual', 'difference') and isinstance(child, str) and Path(child).is_file():
                            artifacts.append(evidence.artifact(child, key))
                        else: collect(child)
                elif isinstance(value, list):
                    for child in value: collect(child)
            collect(result)
            item = next(c for c in report['checks'] if c['id'] == name)
            item.update(status=result.get('status', 'failed'), evidence=artifacts, reason='')
            if result.get('status') != 'passed':
                raise RuntimeError(name+' failed: '+str(result)[:1000])
        except Exception as exc:
            report['gates'][name].update(status='failed', error=str(exc))
            next(c for c in report['checks'] if c['id'] == name).update(status='failed', reason=str(exc))
            raise
        finally:
            persist()
    def external(name, command, field='status'):
        path = output/name/'report.json'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.unlink(missing_ok=True)
        run(command, output/(name+'.log'), timeout=1800)
        result = json.loads(path.read_text())
        return {'status': result.get(field, 'missing_report'), 'report': str(path), 'sha256': sha(path)}
    persist()  # Invalidate any previous passed report before doing work.
    try:
        if sys.platform != 'darwin':
            raise RuntimeError('This pinned official GPU harness currently requires macOS OpenGL')
        manifest = source_manifest(sdk)
        report['source_sha256'] = manifest
        report['provenance'] = evidence.source_identity(ROOT)
        report['git_revision'] = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
        build = ROOT/'target/s6-probes'
        probe = build/'kasane_framework_gpu_probe'
        library = ROOT/'target/debug/libkasane_godot.dylib'
        def build_gate():
            run(['cargo', 'build', '-p', 'kasane-godot', '--locked'], output/'build-rust.log', 1200)
            run(['cmake', '-S', ROOT/'tools/probes', '-B', build, '-DKASANE_CUBISM_ROOT='+str(sdk),
                 '-DKASANE_BUILD_GPU_PROBE=ON', '-DCMAKE_POLICY_VERSION_MINIMUM=3.5', '-DCMAKE_BUILD_TYPE=Release'], output/'configure-probes.log', 300)
            run(['cmake', '--build', build, '--target', 'kasane_framework_gpu_probe', 'kasane_document_official_probe', '-j6'], output/'build-probes.log', 600)
            return {'status': 'passed', 'library_sha256': sha(library), 'official_gpu_probe_sha256': sha(probe)}
        gate('build', build_gate)
        gate('blend_matrix', lambda: matrix_gate(output, probe, godot, sdk))
        if not matrix_only:
            def gpu_script(name, filename, report_name, arguments=(), memory=None):
                directory = output/name
                directory.mkdir(exist_ok=True)
                path = directory/report_name if report_name else None
                if path: path.unlink(missing_ok=True)
                log = output/(name+'.log')
                run([godot, '--path', ROOT/'tests', '--rendering-method', 'gl_compatibility', '--script',
                     'res://'+filename, '--', *arguments, directory], log, memory=memory)
                result = json.loads(path.read_text()) if path else {'status': 'passed'}
                if memory is not None:
                    result['process_rss_samples'] = memory
                    result['peak_process_rss_bytes'] = max(s['rss_bytes'] for s in memory)
                    result['memory_scope'] = 'RenderingServer reports engine-owned GPU allocations; RSS samples cover this process, not total driver memory'
                    path.write_text(json.dumps(result, indent=2)+'\n')
                return {'status': result['status'], 'report': str(path or log), 'sha256': sha(path or log)}
            gate('destination_copy', lambda: gpu_script('destination-copy', 'm3c_viewport_chain_probe.gd', None))
            gate('surface_lifecycle', lambda: gpu_script('lifecycle', 'm3c_surface_lifecycle.gd', 'report.json'))
            gate('ren_resources', lambda: gpu_script('ren-resources', 'm3c_offscreen_gpu.gd', 'godot-report.json',
                                                    [sdk/'Samples/Resources/Ren/Ren.model3.json'], []))
            gate('m4_data', lambda: external('m4-data', [sys.executable, ROOT/'tools/validate_godot.py', '--godot', godot,
                 '--library', library, '--official-probe', build/'kasane_document_official_probe', '--output-dir', output/'m4-data']))
            gate('m4_gpu', lambda: external('m4-gpu', [sys.executable, ROOT/'tools/validate_gpu.py', '--godot', godot,
                 '--library', library, '--output-dir', output/'m4-gpu']))
            gate('m3b_numerical', lambda: external('m3b', [sys.executable, ROOT/'tools/validate_m3b.py', '--numerical-only',
                 '--sdk', sdk, '--probe-dir', build, '--output-dir', output/'m3b'], 'numerical_status'))
            gate('editor_official', lambda: editor_gate(output, probe, godot, sdk))
        if (manifest != source_manifest(sdk) or evidence.source_identity(ROOT) != report['provenance']
                or sha(library) != report['gates']['build']['library_sha256']):
            raise RuntimeError('Source or runtime changed during acceptance; rerun on a stable checkout')
        report['status'] = ('passed' if all(g['status']=='passed' for g in report['gates'].values()) else 'failed') if matrix_only else ('passed' if all_gates_passed(report) else 'failed')
    except Exception as exc:
        report['error'] = str(exc)
        report['checks'].append(evidence.check('run_integrity', 'provenance', 'failed', reason=str(exc)))
    evidence.finalize(report)
    persist()
    return report


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,default=ROOT/'target/kasane/m3c/s6')
    parser.add_argument('--sdk',type=Path,default=SDK)
    parser.add_argument('--godot',type=Path,default=GODOT)
    parser.add_argument('--matrix-only',action='store_true')
    args=parser.parse_args()
    report=acceptance(args.output.resolve(), args.sdk.resolve(), args.godot.resolve(), args.matrix_only)
    print(report['status'],report.get('error',''))
    return 0 if report['status']=='passed' else 1

if __name__=='__main__':
    raise SystemExit(main())

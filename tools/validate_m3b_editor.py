#!/usr/bin/env python3
"""M3B acceptance in a standalone packaged Editor, including textures and GPU."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

from editor_agent import submit
from validate_m5 import script, runtime_images, sha, normalize_drawables
from validate_m3b import compare_samples, read_probe, get_git_info

ROOT = Path(__file__).resolve().parents[1]


def compare_foreground_regions(samples):
    # Crops follow the union of visible pixels; transparent margins cannot dilute
    # errors in the face, torso or lower body. Keep actual/reference/diff artifacts.
    import numpy as np
    from PIL import Image
    result = []
    for i, sample in enumerate(samples):
        a = np.asarray(Image.open(sample['observation']['path']).convert('RGBA'), dtype=np.float64)/255
        b = np.asarray(Image.open(sample['reference_path']).convert('RGBA'), dtype=np.float64)/255
        ys, xs = np.where(np.maximum(a[:,:,3], b[:,:,3]) > .01)
        if not len(xs): raise RuntimeError('Empty model rendering')
        x0, x1, y0, y1 = int(xs.min()), int(xs.max())+1, int(ys.min()), int(ys.max())+1
        bounds = [('foreground', y0, y1), ('upper', y0, y0+(y1-y0)//3),
                  ('middle', y0+(y1-y0)//3, y0+2*(y1-y0)//3), ('lower', y0+2*(y1-y0)//3, y1)]
        for name, lo, hi in bounds:
            difference = np.abs(a[lo:hi,x0:x1]-b[lo:hi,x0:x1])
            metrics = {'sample':i, 'region':name, 'rect':[x0,lo,x1-x0,hi-lo],
                       'mean_absolute_error':float(difference.mean()),
                       'bad_pixel_fraction':float((difference.max(axis=2)>.05).mean())}
            if metrics['mean_absolute_error']>.005 or metrics['bad_pixel_fraction']>.01:
                raise RuntimeError(f'Foreground GPU comparison failed: {metrics}')
            result.append(metrics)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--application', type=Path, default=ROOT/'dist/editor/Kasane-Editor.zip')
    parser.add_argument('--output-dir', type=Path, default=ROOT/'target/kasane/m3b-editor')
    parser.add_argument('--godot', type=Path, default=ROOT/'target/godot-tools/standard/Godot.app/Contents/MacOS/Godot')
    args = parser.parse_args()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    standalone = Path(tempfile.mkdtemp(prefix='kasane-m3b-'))
    report = {'status': 'failed', 'git':get_git_info(), 'renderer':'gl_compatibility',
              'thresholds':{'position_pixels':.05, 'float_absolute':1e-5, 'float_relative':1e-5, 'rgba_mae':.005, 'bad_pixel_fraction':.01, 'bad_pixel_threshold':.05},
              'build_config':'cargo release; Godot export-debug with runtime script diagnostics', 'application_sha256': sha(args.application), 'artifacts': str(standalone),
              'gates': {name: {'status': 'not_run'} for name in ['packaged_editor_workflow', 'detached_texture_project', 'new_feature_edit_roundtrips', 'gpu_comparison']}}
    process = None
    observations = []
    try:
        subprocess.run(['ditto', '-x', '-k', str(args.application.resolve()), str(standalone)], check=True)
        executable = next((next(standalone.glob('*.app'))/'Contents/MacOS').iterdir())
        source = standalone/'source'
        shutil.copytree(ROOT/'demos/gd-cubism-demo/assets/live2d/mao/runtime', source)
        report['source_sha256'] = sha(source/'mao_pro.moc3')
        report['input_inventory'] = {str(p.relative_to(source)):sha(p) for p in source.rglob('*') if p.is_file() and p.suffix in ['.moc3','.json','.png']}
        (standalone/'home').mkdir()
        env = {k:v for k,v in os.environ.items() if not k.startswith(('GODOT','DYLD','CARGO','RUST','DOTNET'))}
        env.update(HOME=str(standalone/'home'), PATH='/usr/bin:/bin')
        log = (output/'application.log').open('w')
        process = subprocess.Popen([str(executable), '--resolution', '1600x1200', '--', '--agent-dir='+str(standalone/'agent')], cwd=standalone, env=env, stdout=log, stderr=subprocess.STDOUT)
        deadline = time.monotonic()+30
        while not (standalone/'agent/status.json').exists():
            if process.poll() is not None or time.monotonic()>deadline:
                raise RuntimeError('Packaged application did not start')
            time.sleep(.1)
        report['engine'] = json.loads((standalone/'agent/status.json').read_text())['engine']
        def send(name, text, observe=False):
            result = submit(standalone/'agent', text, observe=observe, timeout=120)
            (output/(name+'.json')).write_text(json.dumps(result, indent=2))
            if not result.get('ok') or not result.get('business',{}).get('ok',True):
                raise RuntimeError(f'{name} failed: '+str({k:result.get(k) for k in ['code','errors','observation']})[:1600]+'; operations='+str([r for r in result.get('business',{}).get('operations',[]) if not r.get('ok')]))
            return result
        project = standalone/'saved/project.json'
        package = standalone/'package'
        initial = send('import', script(f'var result = w.import_model({json.dumps(str(source/"mao_pro.model3.json"))})\nif not result.ok: return result\nw.fit_view()\nreturn {{"ok": true, "summary": w.document.get_document_summary()}}'), True)
        original_summary = initial['business']['summary']
        assert len(original_summary['meshes']) == 260 and len(original_summary['glues']) == 7
        send('save', script(f'return w.save_project({json.dumps(str(project))})'))
        # Remove the copied source tree completely. The user's fixture is untouched.
        shutil.rmtree(source)
        reopened = send('detached', script(f'var result = w.open_project({json.dumps(str(project))})\nif not result.ok: return result\nw.fit_view()\nreturn {{"ok": true, "resources": w.files.diagnose_resources(w.document), "summary": w.document.get_document_summary()}}'), True)
        assert initial['observation']['sha256'] == reopened['observation']['sha256'], 'Detached preview changed'
        report['gates']['detached_texture_project'] = {'status':'passed', 'project':str(project), 'source_removed':True, 'image_sha256':reopened['observation']['sha256']}
        edited = send('semantic-edits', (ROOT/'tests/editor_m3b_features.gd').read_text())['business']
        summary = edited['summary']
        send('save-edited', script(f'return w.save_project({json.dumps(str(project))})'))
        saved = send('reopen-edited', script(f'var result = w.open_project({json.dumps(str(project))})\nif not result.ok: return result\nvar exported = w.export_model({json.dumps(str(package))})\nreturn {{"ok": exported.ok, "summary": w.document.get_document_summary()}}'))['business']['summary']
        for group in ['blend_key_tables','blend_constraints','blend_bindings','glues']:
            assert summary[group] == saved[group], f'{group} lost on save/reopen'
        params = saved['parameters']
        selections = [{}, {'ParamA':1.0, 'ParamMouthDown':.5}, {'ParamEyeLOpen':0, 'ParamEyeROpen':0, 'ParamRabbitSize':1.0}, {'ParamAngleX':30.0, 'ParamAngleY':-30.0}, {'M3BGlue':.5}, {'M3BGlue':1.0}, {}]
        frames, values = [], []
        for i, overrides in enumerate(selections):
            value = [overrides.get(p['runtime_id'],p['default_value']) for p in params]
            mapping = {p['id']: v for p,v in zip(params,value)}
            result = send(f'sample-{i}', script('var started = Time.get_ticks_usec()\nvar frame = w.document.set_preview_values('+json.dumps(mapping)+')\nframe.preview_update_ms = (Time.get_ticks_usec() - started) / 1000.0\nframe.render_stats = w.surface.get_ref().preview.get_render_stats()\nreturn frame'), True)
            values.append(value)
            frames.append(normalize_drawables(result['business']['drawables']))
            observations.append({'values':value, 'observation':result['observation'], 'render_stats':result['business']['render_stats'], 'preview_update_ms':result['business']['preview_update_ms']})
        assert observations[0]['observation']['sha256'] == observations[-1]['observation']['sha256'], 'A-B-A preview is stateful'
        numerical = []
        for provider in ['official','purism']:
            reference = read_probe(ROOT/f'target/probes/kasane_document_{provider}_probe', package/'model.moc3', values, output/(provider+'.json'))
            numerical.append({'provider':provider, **compare_samples(frames, reference['samples'], saved['pixels_per_unit'], provider+'/edited-export')})
        report['gates']['new_feature_edit_roundtrips'] = {'status':'passed', 'kinds':edited['edited_kinds'], 'history_barrier':edited['history_barrier_ok'], 'invalid_write_atomic':edited['invalid_ok'], 'numerical':numerical}
        report['gates']['packaged_editor_workflow'] = {'status':'passed', 'executable':str(executable), 'cwd':str(standalone), 'PATH':env['PATH']}
        process.terminate(); process.wait(timeout=10)
        process = None
        metrics = runtime_images(args.godot.resolve(), package, observations, saved, standalone/'runtime')
        roi = compare_foreground_regions(observations)
        for key in ['mesh_views', 'mask_viewports', 'materials', 'shaders']:
            assert observations[0]['render_stats'][key] == observations[-1]['render_stats'][key], f'Resource growth: {key}'
        report['gates']['gpu_comparison'] = {'status':'passed', 'comparisons':metrics, 'regions':roi,
            'resource_counts':observations[-1]['render_stats'], 'preview_update_ms':[s['preview_update_ms'] for s in observations], 'samples':len(observations)}
        report['status'] = 'passed'
    except Exception as exc:
        report['error'] = str(exc)
        print(exc, flush=True)
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try: process.wait(timeout=10)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
        (output/'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps({'status':report['status'], 'report':str(output/'report.json')}))
    return 0 if report['status']=='passed' else 1


if __name__ == '__main__':
    raise SystemExit(main())

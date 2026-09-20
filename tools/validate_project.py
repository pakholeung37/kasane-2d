#!/usr/bin/env python3
"""M2 acceptance: native persistence first, then Godot binding and real-GPU checks."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from validate_native_project import validate, ROOT

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--godot',type=Path,default=Path('/Applications/Godot_mono.app/Contents/MacOS/Godot'))
    parser.add_argument('--core-build',type=Path,default=ROOT/'target/kasane/core-regression/build')
    parser.add_argument('--output-dir',type=Path,default=ROOT/'target/kasane/project-files')
    args=parser.parse_args()
    args.output_dir.mkdir(parents=True,exist_ok=True)
    base=Path(tempfile.mkdtemp(prefix='run-',dir=args.output_dir)).resolve()
    report=dict(status='failed',checks=[],native_report=str(base/'native/report.json'))
    def run(command,label):
        result=subprocess.run(list(map(str,command)),text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=120)
        (base/(label+'.log')).write_text(result.stdout)
        if result.returncode or 'ERROR:' in result.stdout:raise RuntimeError(f'{label}: {result.stdout[-5000:]}')
    try:
        native=validate(args.core_build.resolve(),base/'native')
        report['native']=dict(status=native['status'],checks=len(native['checks']),source_sha256=native['source_sha256'])
        if native['status']!='passed':raise RuntimeError('Native project acceptance failed')
        project=base/'harness';project.mkdir()
        library=ROOT/'modules/kasane-gd/build/bin/libkasane_gd.macos.template_debug.arm64.dylib'
        report['library_sha256']=hashlib.sha256(library.read_bytes()).hexdigest()
        shutil.copyfile(library,project/library.name)
        shutil.copyfile(ROOT/'modules/kasane-gd/tests/project_files.gd',project/'test.gd')
        shutil.copyfile(ROOT/'modules/kasane-gd/tests/project_capture.gd',project/'capture.gd')
        (project/'project.godot').write_text('config_version=5\n[application]\nconfig/name="Native Project Adapter"\n[rendering]\nrenderer/rendering_method="gl_compatibility"\n')
        (project/'kasane.gdextension').write_text('[configuration]\nentry_symbol="kasane_gd_library_init"\ncompatibility_minimum="4.3"\n[libraries]\nmacos.debug.arm64="res://'+library.name+'"\n')
        (project/'.godot').mkdir();(project/'.godot/extension_list.cfg').write_text('res://kasane.gdextension\n')
        adapter=base/'adapter';adapter.mkdir()
        run([args.godot,'--headless','--path',project,'--script','res://test.gd','--',base/'native/moved',adapter],'adapter')
        child=json.loads((adapter/'godot-report.json').read_text());report['checks'].extend(child['checks']);report['godot']=child['godot']
        if child['status']!='passed':raise RuntimeError('Godot adapter checks failed')
        original=base/'gpu-original';shutil.copytree(base/'native/moved',original)
        for label,source in [('before',original),('after',base/'gpu-moved')]:
            if label=='after':
                original.rename(source)
                shutil.rmtree(project/'.godot');(project/'.godot').mkdir();(project/'.godot/extension_list.cfg').write_text('res://kasane.gdextension\n')
            run([args.godot,'--path',project,'--rendering-method','gl_compatibility','--resolution','640x480','--script','res://capture.gd','--',source,base/('gpu-'+label)],'gpu-'+label)
        from PIL import Image,ImageChops
        import numpy as np
        for index in range(3):
            a=Image.open(base/f'gpu-before-{index}.png').convert('RGBA');b=Image.open(base/f'gpu-after-{index}.png').convert('RGBA')
            bbox=a.getchannel('A').getbbox()
            if bbox is None:raise RuntimeError('GPU rendered an empty image')
            ImageChops.difference(a,b).save(base/f'gpu-diff-{index}.png')
            for label,left,right in [('whole',a,b),('foreground',a.crop(bbox),b.crop(bbox))]:
                error=abs(np.asarray(left,dtype=float)-np.asarray(right,dtype=float))/255
                mean,bad=float(error.mean()),float((error.max(axis=2)>0.05).mean())
                if mean>0.005 or bad>0.01:raise RuntimeError('Moved project GPU image changed')
                report['checks'].append(dict(name=f'GPU {index}/{label}',status='passed',expected=dict(mean_max=0.005,bad_pixel_fraction_max=0.01),actual=dict(mean=mean,bad_pixel_fraction=bad),crop=bbox))
                if label=='foreground':left.save(base/f'gpu-crop-before-{index}.png');right.save(base/f'gpu-crop-after-{index}.png')
        report['gpu']=json.loads((base/'gpu-after.json').read_text());report['status']='passed'
    except Exception as exc:report['error']=str(exc);print(exc)
    finally:
        report['files']=[dict(path=str(p.relative_to(base)),sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in sorted(base.rglob('*')) if p.is_file()]
        (base/'report.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n')
        (args.output_dir/'latest.json').write_text(json.dumps(dict(report=str(base/'report.json'),status=report['status']),indent=2)+'\n')
    print(f'M2 {report["status"]}: {len(report["checks"])} adapter/GPU checks; {base / "report.json"}')
    return 0 if report['status']=='passed' else 1
if __name__=='__main__':raise SystemExit(main())

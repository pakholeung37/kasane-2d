"""Image acceptance thresholds from docs/editor/VALIDATION.md."""
import json
from pathlib import Path
import numpy as np
from PIL import Image


def compare(project: Path):
    capture = json.loads((project/'capture.json').read_text())
    checks, samples = [], []
    for i, state in enumerate(capture['states']):
        images = [np.asarray(Image.open(project/f'{name}-{i}.png').convert('RGBA'), dtype=np.float64)/255
                  for name in ('reference', 'actual')]
        difference = np.abs(images[0]-images[1])
        # Raw difference, not an amplified presentation image.
        Image.fromarray(np.uint8(np.rint(difference*255))).save(project/f'difference-{i}.png')
        def metrics(name, data):
            mean = float(data.mean())
            bad = float(np.mean(np.max(data, axis=2) > 0.05))
            checks.append(dict(name=name, expected={'mean_max':0.005,'bad_pixel_fraction_max':0.01},
                               actual={'mean':mean,'bad_pixel_fraction':bad,'maximum':float(data.max())},
                               status='passed' if mean <= 0.005 and bad <= 0.01 else 'failed'))
        metrics(f'frame-{i}/whole', difference)
        scale = state['scale']
        offset = np.asarray(state['offset'])
        for row in range(3):
            for col in range(3):
                origin = np.array([40+190*col,40+135*row])
                lo = np.floor(origin*scale+offset).astype(int)
                hi = np.ceil((origin+[160,100])*scale+offset).astype(int)
                region = difference[lo[1]:hi[1],lo[0]:hi[0]]
                name = f'frame-{i}/row-{row}/blend-{col}'
                metrics(name, region)
                for label, im in zip(('reference','actual','difference'), (*images,difference)):
                    crop = im[lo[1]:hi[1],lo[0]:hi[0]]
                    Image.fromarray(np.uint8(np.rint(crop*255))).save(project/f'{label}-{i}-r{row}-b{col}.png')
                # Two 5x5 regions well inside each half. Analytical texture and
                # blend equations independently check both rendering paths.
                for point in ([30,25],[130,75]):
                    center = np.floor((origin+point)*scale+offset).astype(int)
                    errors = []
                    for dy in range(-2,3):
                        for dx in range(-2,3):
                            pixel = center+[dx,dy]
                            local = (pixel+0.5-offset)/scale-origin
                            texel = np.clip(local/[160,100]*8-0.5,0,7)
                            color = np.array([texel[0]*31/255,texel[1]*31/255,240/255])
                            color *= [0.8,0.7,0.9]
                            screen = np.array([0.1,0.05,0.15])
                            color = color+screen-color*screen
                            mask = state['value']*20 <= local[0] < state['value']*20+80
                            alpha = 0.65*(1 if row == 0 else mask if row == 1 else not mask)
                            background = np.array([64,128,192])/255
                            rgb = (color*alpha+background*(1-alpha) if col == 0 else
                                   color*alpha+background if col == 1 else
                                   background*(color*alpha+1-alpha))
                            expected = np.r_[np.clip(rgb,0,1),1]
                            actual = [im[pixel[1],pixel[0]] for im in images]
                            error = float(max(np.abs(a-expected).max() for a in actual))
                            errors.append(error)
                            samples.append(dict(case=name,pixel=pixel.tolist(),expected=expected.tolist(),
                                                reference=actual[0].tolist(),actual=actual[1].tolist(),error=error))
                    checks.append(dict(name=name+f'/region-{point}',expected={'max_channel_error':2/255},
                                       actual={'max_channel_error':max(errors)},
                                       status='passed' if max(errors) <= 2/255 else 'failed'))
    (project/'pixel-samples.json').write_text(json.dumps(samples,indent=2)+'\n')
    report = dict(capture, checks=checks, status='passed' if capture['status']=='passed' and all(c['status']=='passed' for c in checks) else 'failed')
    (project/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    if report['status'] != 'passed':
        raise RuntimeError('GPU comparison failed: '+', '.join(c['name'] for c in checks if c['status']!='passed'))
    return report

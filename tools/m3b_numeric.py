"""Vectorized equivalent of the strict M3B scalar comparator for large samples."""
import bisect
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
for candidate in [ROOT/'target/buildenv/lib/python3.14/site-packages', ROOT/'target/kasane/buildenv/lib/python3.14/site-packages']:
    if candidate.is_dir():
        sys.path.insert(0, str(candidate))
        break
try:
    import numpy as np
except ImportError:
    np = None


def compare(expected, actual, ppu, label):
    if len(expected) != len(actual):
        raise RuntimeError(f'{label}: sample count mismatch')
    metrics = {'max_pixel_error':0.0, 'max_uv_error':0.0, 'max_float_error':0.0}
    for sample_index, (exp, raw) in enumerate(zip(expected, actual)):
        exp_by_id = {d['runtime_id']:d for d in exp}
        act_by_id = {d['runtime_id']:d for d in raw}
        if len(exp_by_id)!=len(exp) or len(act_by_id)!=len(raw) or exp_by_id.keys()!=act_by_id.keys():
            raise RuntimeError(f'{label}/sample {sample_index}: drawable IDs differ or are duplicated')
        act = [act_by_id[d['runtime_id']] for d in exp]
        for e, a in zip(exp, act):
            context = f"{label}/sample {sample_index}/{e['runtime_id']}"
            for attr in ['texture_slot','double_sided','inverted_mask','blend_mode','indices','draw_order','render_order','visible']:
                if e[attr]!=a[attr]: raise RuntimeError(f'{context}/{attr}: expected={e[attr]}, actual={a[attr]}')
            em = list(dict.fromkeys(exp[i]['runtime_id'] for i in e['mask_indices']))
            am = list(dict.fromkeys(raw[i]['runtime_id'] for i in a['mask_indices']))
            if em!=am: raise RuntimeError(f'{context}/masks: expected={em}, actual={am}')
        for attr in ['opacity','multiply_color','screen_color','uvs','positions']:
            if attr=='opacity':
                offsets = list(range(len(exp)+1))
                e = np.asarray([d[attr] for d in exp], dtype=np.float64)
                a = np.asarray([d[attr] for d in act], dtype=np.float64)
            else:
                offsets = [0]
                for ed, ad in zip(exp,act):
                    if len(ed[attr])!=len(ad[attr]): raise RuntimeError(f'{label}/{ed["runtime_id"]}/{attr}: array length mismatch')
                    offsets.append(offsets[-1]+len(ed[attr]))
                e = np.asarray([v for d in exp for v in d[attr]], dtype=np.float64)
                a = np.asarray([v for d in act for v in d[attr]], dtype=np.float64)
            if e.shape!=a.shape: raise RuntimeError(f'{label}/{attr}: component count mismatch')
            if not np.isfinite(e).all() or not np.isfinite(a).all(): raise RuntimeError(f'{label}/{attr}: non-finite value')
            if not e.size: continue
            difference = np.abs(e-a)
            maximum = float(difference.max())
            metrics['max_float_error'] = max(metrics['max_float_error'],maximum)
            if attr=='positions': metrics['max_pixel_error'] = max(metrics['max_pixel_error'],maximum*ppu)
            if attr=='uvs': metrics['max_uv_error'] = max(metrics['max_uv_error'],maximum)
            bad = difference > 1e-5 + 1e-5*np.maximum(np.abs(e),np.abs(a))
            if attr=='positions': bad |= difference*ppu > .05
            locations = np.argwhere(bad)
            if len(locations):
                location = tuple(locations[0]); row = int(location[0])
                mesh_index = bisect.bisect_right(offsets,row)-1
                context = f"{label}/sample {sample_index}/{exp[mesh_index]['runtime_id']}/{attr}"
                if attr!='opacity': context += '/'+str(row-offsets[mesh_index])
                if len(location)>1: context += '/'+str(location[1])
                raise RuntimeError(f'{context}: expected={e[location]}, actual={a[location]}, abs_error={difference[location]}, pixel_error={difference[location]*ppu if attr=="positions" else None}')
    return metrics

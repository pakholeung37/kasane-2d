#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeySearchResult {
    pub index: i32,
    pub weight: f32,
    pub is_outside: bool,
    pub needs_check: bool,
}

pub fn find_key_segment(
    value: f32,
    keys: &[f32],
    snap_eps: f32,
    interp_eps: f32,
) -> KeySearchResult {
    let mut r = KeySearchResult {
        index: 0,
        weight: 0.0,
        is_outside: false,
        needs_check: false,
    };

    let key_count = keys.len();
    if key_count == 0 {
        r.needs_check = true;
        return r;
    }

    if key_count == 1 {
        let key0 = keys[0];
        r.is_outside = value <= key0 - snap_eps || value >= key0 + snap_eps;
        r.needs_check = !r.is_outside;
        return r;
    }

    let key0 = keys[0];
    if value < key0 - snap_eps {
        r.is_outside = true;
        return r;
    }
    if value < key0 + snap_eps {
        r.needs_check = true;
        return r;
    }

    let mut key1 = keys[1];
    if value < key1 - snap_eps {
        let key_diff = key1 - key0;
        if key_diff >= interp_eps {
            r.weight = (value - key0) / key_diff;
        }
        return r;
    }
    if value < key1 + snap_eps {
        r.index = 1;
        r.needs_check = true;
        return r;
    }

    for (k, &key) in keys.iter().enumerate().skip(2) {
        let prev = key1;
        key1 = key;
        if value < key1 - snap_eps {
            r.index = (k - 1) as i32;
            let key_diff = key1 - prev;
            if key_diff >= interp_eps {
                r.weight = (value - prev) / key_diff;
            }
            return r;
        }
        if value < key1 + snap_eps {
            r.index = k as i32;
            r.needs_check = true;
            return r;
        }
    }

    r.index = (key_count - 1) as i32;
    r.is_outside = true;
    r
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KeyAxis {
    pub index: i32,
    pub key_count: i32,
    pub weight: f32,
}

pub fn key_combinations(axes: &[KeyAxis], indices: &mut [i32], weights: &mut [f32]) -> usize {
    let mut active = 0usize;
    for axis in axes {
        if axis.weight != 0.0 {
            active += 1;
        }
    }
    let count = 1usize << active;

    indices[..count].fill(0);
    weights[..count].fill(1.0);

    let mut index_stride = 1i32;
    let mut combo_stride = 1usize;

    for axis in axes {
        let offset = axis.index * index_stride;
        if axis.weight != 0.0 {
            let next_offset = (axis.index + 1) * index_stride;
            let inverse = 1.0 - axis.weight;
            for j in 0..count {
                if (j & combo_stride) == 0 {
                    indices[j] += offset;
                    weights[j] *= inverse;
                } else {
                    indices[j] += next_offset;
                    weights[j] *= axis.weight;
                }
            }
            combo_stride *= 2;
        } else {
            for item in indices.iter_mut().take(count) {
                *item += offset;
            }
        }
        index_stride *= axis.key_count;
    }

    count
}

pub fn blend_vectors(targets: &[&[f32]], weights: &[f32], element_count: usize, out: &mut [f32]) {
    out[..element_count].fill(0.0);
    for (j, &src) in targets.iter().enumerate() {
        let w = weights[j];
        if w != 0.0 {
            for k in 0..element_count {
                out[k] += src[k] * w;
            }
        }
    }
}

use super::AlphaMask;

/// A bounded, conservative speck filter. This is an explicit authoring policy,
/// not an inferred Cubism implementation and not a global alpha threshold.
pub(super) fn clean(mask: AlphaMask<'_>) -> Vec<u8> {
    let mut alpha = mask.alpha.to_vec();
    if !alpha.iter().any(|&a| a >= 128) {
        return alpha;
    }
    let width = mask.width as usize;
    let mut seen = vec![false; alpha.len()];
    let mut stack = Vec::new();
    // A removable component has <= 1020 nonzero pixels. Never retain a full
    // large component's index list just to decide whether it is significant.
    let mut small = Vec::new();
    for start in 0..alpha.len() {
        if alpha[start] == 0 || seen[start] {
            continue;
        }
        stack.push(start);
        seen[start] = true;
        small.clear();
        let mut mass = 0u64;
        let mut peak = 0;
        while let Some(i) = stack.pop() {
            mass += alpha[i] as u64;
            peak = peak.max(alpha[i]);
            if mass <= 4 * 255 && peak <= 64 {
                small.push(i);
            }
            let x = i % width;
            let neighbors = [
                (x > 0).then(|| i - 1),
                (x + 1 < width).then(|| i + 1),
                (i >= width).then(|| i - width),
                (i + width < alpha.len()).then(|| i + width),
            ];
            for j in neighbors.into_iter().flatten() {
                if alpha[j] != 0 && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if mass <= 4 * 255 && peak <= 64 {
            for &i in &small {
                alpha[i] = 0;
            }
        }
    }
    alpha
}

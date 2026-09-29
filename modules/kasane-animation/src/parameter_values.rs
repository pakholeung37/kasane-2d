//! Reuse UUID keys and tree nodes when only parameter values change.
use std::collections::BTreeMap;

pub(crate) fn copy_values(target: &mut BTreeMap<String, f32>, source: &BTreeMap<String, f32>) {
    if target.len() == source.len()
        && target
            .iter_mut()
            .zip(source)
            .all(|((key, value), (source_key, source_value))| {
                if key != source_key {
                    return false;
                }
                *value = *source_value;
                true
            })
    {
        return;
    }
    // Reset/import or a changed virtual-control set can change the keys.
    // Replace the whole map in that case, including removal of stale keys.
    target.clone_from(source);
}

pub(crate) fn set_value(target: &mut BTreeMap<String, f32>, id: &str, value: f32) {
    if let Some(slot) = target.get_mut(id) {
        *slot = value;
    } else {
        target.insert(id.to_owned(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_copies_match_replacement_even_when_keys_change() {
        let mut target = BTreeMap::new();
        for pairs in [
            vec![("a", 1.), ("b", 2.)],
            vec![("a", -0.), ("b", 3.)],
            vec![("a", 4.), ("c", 5.)],
            vec![("c", 6.)],
            vec![],
        ] {
            let source: BTreeMap<_, _> =
                pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
            copy_values(&mut target, &source);
            assert_eq!(target, source);
            for (id, value) in &source {
                assert_eq!(target[id].to_bits(), value.to_bits());
            }
        }
        set_value(&mut target, "a", 1.);
        let key = target.keys().next().unwrap().as_ptr();
        set_value(&mut target, "a", 2.);
        copy_values(&mut target, &[("a".to_owned(), 3.)].into());
        assert_eq!(target["a"], 3.);
        assert_eq!(key, target.keys().next().unwrap().as_ptr());
    }
}

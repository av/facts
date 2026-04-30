/// Fact ID generation via label hashing.
use std::collections::HashMap;

/// Assign IDs to all facts, extending on collision.
/// Returns a vec of assigned IDs parallel to the input.
pub fn assign_ids(facts: &[(String, Option<String>)]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::with_capacity(facts.len());
    let mut used: HashMap<String, Vec<usize>> = HashMap::new();

    // Pre-compute hashes
    let hashes: Vec<u128> = facts.iter().map(|(label, _)| full_hash(label)).collect();

    // First pass: assign default 3-char IDs
    for (i, (_, explicit_id)) in facts.iter().enumerate() {
        let id = if let Some(eid) = explicit_id {
            eid.clone()
        } else {
            encode_base36(hashes[i], 3)
        };
        used.entry(id.clone()).or_default().push(i);
        ids.push(id);
    }

    // Resolve collisions by extending hash length.
    // Max 25 base-36 digits covers the full 128-bit hash space.
    const MAX_LEN: usize = 25;

    let mut changed = true;
    while changed {
        changed = false;
        let collisions: Vec<(String, Vec<usize>)> = used
            .iter()
            .filter(|(_, indices)| indices.len() > 1)
            .map(|(id, indices)| (id.clone(), indices.clone()))
            .collect();

        for (_, indices) in collisions {
            let extendable: Vec<usize> = indices
                .iter()
                .filter(|&&i| facts[i].1.is_none())
                .copied()
                .collect();

            if extendable.len() <= 1 {
                continue;
            }

            // Check if all colliding facts have identical hashes.
            // If so, extending the ID length will never resolve the collision —
            // skip directly to counter suffix disambiguation.
            let all_same_hash = extendable
                .iter()
                .all(|&i| hashes[i] == hashes[extendable[0]]);

            let current_len = ids[extendable[0]].len();
            if current_len >= MAX_LEN || all_same_hash {
                // Append a counter suffix to disambiguate
                for (counter, &i) in extendable.iter().enumerate().skip(1) {
                    let old = ids[i].clone();
                    ids[i] = format!("{old}{counter}");
                    let entry = used.get_mut(&old).unwrap();
                    entry.retain(|&x| x != i);
                    used.entry(ids[i].clone()).or_default().push(i);
                    changed = true;
                }
                continue;
            }

            changed = true;
            for &i in &extendable {
                let new_len = ids[i].len() + 1;
                let new_id = encode_base36(hashes[i], new_len);
                let old = ids[i].clone();
                ids[i] = new_id;

                let entry = used.get_mut(&old).unwrap();
                entry.retain(|&x| x != i);
                if entry.is_empty() {
                    used.remove(&old);
                }
                used.entry(ids[i].clone()).or_default().push(i);
            }
        }
    }

    ids
}

/// 128-bit FNV-1a hash for maximum collision resistance.
fn full_hash(s: &str) -> u128 {
    let mut hash: u128 = 0x6c62272e07bb0142_62b821756295c58d;
    for byte in s.bytes() {
        hash ^= byte as u128;
        hash = hash.wrapping_mul(0x0000000001000000_000000000000013b);
    }
    hash
}

/// Encode a hash value as a base-36 string of exactly `len` characters.
/// Uses the lowest-order digits of the base-36 representation.
fn encode_base36(value: u128, len: usize) -> String {
    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut digits = Vec::with_capacity(len);
    let mut v = value;
    for _ in 0..len {
        digits.push(CHARS[(v % 36) as usize]);
        v /= 36;
    }
    digits.reverse();
    String::from_utf8(digits).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_deterministic() {
        let h1 = full_hash("some fact");
        let h2 = full_hash("some fact");
        assert_eq!(encode_base36(h1, 3), encode_base36(h2, 3));
        assert_eq!(encode_base36(h1, 3).len(), 3);
    }

    #[test]
    fn test_different_labels_different_hashes() {
        let h1 = full_hash("fact one");
        let h2 = full_hash("fact two");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_assign_ids_no_collision() {
        let facts = vec![
            ("fact one".to_string(), None),
            ("fact two".to_string(), None),
        ];
        let ids = assign_ids(&facts);
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn test_explicit_id_preserved() {
        let facts = vec![
            ("fact one".to_string(), Some("abc".to_string())),
            ("fact two".to_string(), None),
        ];
        let ids = assign_ids(&facts);
        assert_eq!(ids[0], "abc");
    }

    #[test]
    fn test_duplicate_labels_get_unique_ids() {
        // Duplicate labels with same hash — tests the fallback disambiguator
        let facts = vec![
            ("same label".to_string(), None),
            ("same label".to_string(), None),
        ];
        let ids = assign_ids(&facts);
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn test_encode_length_is_exact() {
        let h = full_hash("test");
        assert_eq!(encode_base36(h, 3).len(), 3);
        assert_eq!(encode_base36(h, 5).len(), 5);
        // Extending should produce a prefix relationship
        let short = encode_base36(h, 3);
        let long = encode_base36(h, 5);
        // The last 3 chars of the 5-char encoding should match the 3-char one
        // (because we use least-significant digits, reversed)
        // Actually, the 3-char version is the last 3 digits reversed.
        // At len=3 we get digits [d2, d1, d0] reversed = [d0, d1, d2]
        // At len=5 we get digits [d4, d3, d2, d1, d0] reversed = [d0, d1, d2, d3, d4]
        // So the 3-char one should be a suffix of the 5-char one (last 3 chars)
        assert_eq!(&long[2..], &short[..]);
    }
}

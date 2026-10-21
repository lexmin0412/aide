//! Content hashing for skills and snapshots.
//!
//! aide needs a stable fingerprint for a skill directory and for individual
//! files, without pulling in a crypto dependency. This module provides a small
//! SHA-256 implementation plus a deterministic tree hash built from the sorted
//! relative file paths and their content hashes.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::fsutil::IGNORED_DIRS;

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 of `data`, lowercase hex encoded.
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = sha256(data);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (index, word) in schedule.iter_mut().take(16).enumerate() {
            let start = index * 4;
            *word = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];

        for index in 0..64 {
            let sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(sigma1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sigma0.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

/// Map of forward-slashed relative path -> sha256 of the file content, sorted by
/// path. Symlinks and build/dependency directories are skipped.
pub(crate) fn file_map(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut map = BTreeMap::new();
    collect(root, root, &mut map)?;
    Ok(map)
}

fn collect(root: &Path, dir: &Path, map: &mut BTreeMap<String, String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        if file_type.is_dir() {
            if IGNORED_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect(root, &path, map)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            map.insert(relative, sha256_hex(&bytes));
        }
    }
    Ok(())
}

/// Deterministic tree hash of a skill directory.
pub(crate) fn tree_hash(root: &Path) -> Result<String, String> {
    Ok(tree_hash_map(&file_map(root)?))
}

/// Tree hash over an already-computed file map.
pub(crate) fn tree_hash_map(map: &BTreeMap<String, String>) -> String {
    let mut buffer = Vec::new();
    for (path, hash) in map {
        buffer.extend_from_slice(path.as_bytes());
        buffer.push(0);
        buffer.extend_from_slice(hash.as_bytes());
        buffer.push(b'\n');
    }
    sha256_hex(&buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aide-hash-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"hello world"),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn tree_hash_is_stable_and_content_sensitive() {
        let dir = temp_dir("tree");
        let skill = dir.join("skill");
        fs::create_dir_all(skill.join("references")).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: skill\n---\n").unwrap();
        fs::write(skill.join("references/notes.md"), "note").unwrap();

        let first = tree_hash(&skill).unwrap();
        assert_eq!(tree_hash(&skill).unwrap(), first);

        // Content change, file addition and removal all change the hash.
        fs::write(skill.join("references/notes.md"), "note!").unwrap();
        assert_ne!(tree_hash(&skill).unwrap(), first);
        let changed = tree_hash(&skill).unwrap();
        fs::write(skill.join("extra.txt"), "x").unwrap();
        assert_ne!(tree_hash(&skill).unwrap(), changed);
        fs::remove_file(skill.join("extra.txt")).unwrap();
        assert_eq!(tree_hash(&skill).unwrap(), changed);

        // Ignored directories never contribute to the hash.
        fs::create_dir_all(skill.join("node_modules/pkg")).unwrap();
        fs::write(skill.join("node_modules/pkg/index.js"), "x").unwrap();
        assert_eq!(tree_hash(&skill).unwrap(), changed);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_map_uses_forward_slashed_relative_paths() {
        let dir = temp_dir("map");
        let skill = dir.join("skill");
        fs::create_dir_all(skill.join("a/b")).unwrap();
        fs::write(skill.join("a/b/c.txt"), "hi").unwrap();
        let map = file_map(&skill).unwrap();
        assert_eq!(map.len(), 1);
        let (path, hash) = map.iter().next().unwrap();
        assert_eq!(path, "a/b/c.txt");
        assert_eq!(hash, &sha256_hex(b"hi"));
        let _ = fs::remove_dir_all(&dir);
    }
}

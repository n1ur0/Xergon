//! Simple binary Merkle tree using blake2b256.
//!
//! Used by the usage proof rollup system to batch multiple proofs
//! into a single Merkle root commitment on-chain.
//!
//! Each leaf = blake2b256(serialized_proof_data)
//! The tree pads to the next power of 2 by duplicating the last leaf.

use blake2::Digest;
use std::fmt;

/// A 32-byte blake2b256 hash.
#[derive(Clone, PartialEq, Eq)]
pub struct MerkleHash(pub [u8; 32]);

impl fmt::Debug for MerkleHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MerkleHash({})", hex::encode(self.0))
    }
}

impl fmt::Display for MerkleHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl MerkleHash {
    /// Compute blake2b256 of the given data.
    pub fn hash(data: &[u8]) -> Self {
        type Blake2b256 = blake2::Blake2b<blake2::digest::consts::U32>;
        let mut hasher = Blake2b256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        MerkleHash(hash)
    }

    /// Create a zero hash (used as padding).
    pub fn zero() -> Self {
        MerkleHash([0u8; 32])
    }

    /// Return as hex string.
    pub fn hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex string.
    pub fn from_hex(hex_str: &str) -> Option<Self> {
        let bytes = hex::decode(hex_str).ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Some(MerkleHash(hash))
    }
}

/// A Merkle proof that a specific leaf is included in the tree.
#[derive(Debug, Clone)]
pub struct MerkleProof {
    /// The index of the leaf this proof is for.
    pub leaf_index: usize,
    /// The total number of leaves in the tree.
    pub leaf_count: usize,
    /// Sibling hashes from leaf to root (left to right = bottom to top).
    pub siblings: Vec<MerkleHash>,
    /// The root hash this proof verifies against.
    pub root: MerkleHash,
}

impl MerkleProof {
    /// Verify that the given leaf hash is included in the tree.
    pub fn verify(&self, leaf_hash: &MerkleHash) -> bool {
        let mut current = leaf_hash.clone();
        let mut idx = self.leaf_index;

        for sibling in &self.siblings {
            let mut buf = Vec::with_capacity(64);
            if idx % 2 == 0 {
                // Current is on the left, sibling on the right
                buf.extend_from_slice(&current.0);
                buf.extend_from_slice(&sibling.0);
            } else {
                // Current is on the right, sibling on the left
                buf.extend_from_slice(&sibling.0);
                buf.extend_from_slice(&current.0);
            }
            current = MerkleHash::hash(&buf);
            idx /= 2;
        }

        current == self.root
    }
}

/// A simple binary Merkle tree.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    /// The leaf hashes.
    leaves: Vec<MerkleHash>,
    /// The root hash.
    root: MerkleHash,
}

impl MerkleTree {
    /// Build a Merkle tree from leaf hashes.
    ///
    /// Pads to the next power of 2 by duplicating the last leaf.
    /// Returns the tree. If leaves is empty, returns a tree with zero root.
    pub fn from_leaves(leaves: Vec<MerkleHash>) -> Self {
        if leaves.is_empty() {
            return Self {
                leaves: vec![],
                root: MerkleHash::zero(),
            };
        }

        let root = Self::compute_root(&leaves);
        Self { leaves, root }
    }

    /// Build a Merkle tree from raw leaf data (hashes each entry first).
    pub fn from_data(data: &[&[u8]]) -> Self {
        let leaves: Vec<MerkleHash> = data.iter().map(|d| MerkleHash::hash(d)).collect();
        Self::from_leaves(leaves)
    }

    /// Get the root hash.
    pub fn root(&self) -> &MerkleHash {
        &self.root
    }

    /// Get the number of leaves.
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    /// Generate a Merkle proof for the leaf at the given index.
    pub fn proof(&self, leaf_index: usize) -> Option<MerkleProof> {
        if leaf_index >= self.leaves.len() {
            return None;
        }

        let padded_count = next_power_of_2(self.leaves.len());
        let mut current_level: Vec<MerkleHash> = self.leaves.clone();

        // Pad to next power of 2
        if current_level.len() < padded_count {
            if let Some(last) = current_level.last().cloned() {
                while current_level.len() < padded_count {
                    current_level.push(last.clone());
                }
            }
        }

        let mut siblings = Vec::new();
        let mut idx = leaf_index;
        let mut level_size = current_level.len();

        while level_size > 1 {
            let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
            if sibling_idx < current_level.len() {
                siblings.push(current_level[sibling_idx].clone());
            }

            // Compute next level
            let mut next_level = Vec::with_capacity(level_size / 2);
            for i in (0..level_size).step_by(2) {
                let left = &current_level[i];
                let right = if i + 1 < current_level.len() {
                    &current_level[i + 1]
                } else {
                    left
                };
                let mut buf = Vec::with_capacity(64);
                buf.extend_from_slice(&left.0);
                buf.extend_from_slice(&right.0);
                next_level.push(MerkleHash::hash(&buf));
            }

            current_level = next_level;
            idx /= 2;
            level_size = current_level.len();
        }

        Some(MerkleProof {
            leaf_index,
            leaf_count: self.leaves.len(),
            siblings,
            root: self.root.clone(),
        })
    }

    /// Compute the root hash from a set of leaves.
    fn compute_root(leaves: &[MerkleHash]) -> MerkleHash {
        if leaves.is_empty() {
            return MerkleHash::zero();
        }

        let padded_count = next_power_of_2(leaves.len());
        let mut current_level: Vec<MerkleHash> = leaves.to_vec();

        // Pad to next power of 2 by duplicating last leaf
        if let Some(last) = current_level.last().cloned() {
            while current_level.len() < padded_count {
                current_level.push(last.clone());
            }
        }

        while current_level.len() > 1 {
            let mut next_level = Vec::with_capacity(current_level.len() / 2);
            for i in (0..current_level.len()).step_by(2) {
                let left = &current_level[i];
                let right = &current_level[i + 1];
                let mut buf = Vec::with_capacity(64);
                buf.extend_from_slice(&left.0);
                buf.extend_from_slice(&right.0);
                next_level.push(MerkleHash::hash(&buf));
            }
            current_level = next_level;
        }

        current_level
            .into_iter()
            .next()
            .unwrap_or(MerkleHash::zero())
    }
}

/// Compute the next power of 2 >= n.
fn next_power_of_2(n: usize) -> usize {
    if n <= 1 {
        return 1;
    }
    n.next_power_of_two()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_leaf() {
        let tree = MerkleTree::from_data(&[b"hello"]);
        assert_eq!(tree.leaf_count(), 1);
        // Root should be hash("hello")
        assert_eq!(*tree.root(), MerkleHash::hash(b"hello"));
    }

    #[test]
    fn test_two_leaves() {
        let tree = MerkleTree::from_data(&[b"hello", b"world"]);
        assert_eq!(tree.leaf_count(), 2);
        let left = MerkleHash::hash(b"hello");
        let right = MerkleHash::hash(b"world");
        let mut combined = [0u8; 64];
        combined[..32].copy_from_slice(&left.0);
        combined[32..].copy_from_slice(&right.0);
        let expected_root = MerkleHash::hash(&combined);
        assert_eq!(*tree.root(), expected_root);
    }

    #[test]
    fn test_three_leaves_padded() {
        let tree = MerkleTree::from_data(&[b"a", b"b", b"c"]);
        assert_eq!(tree.leaf_count(), 3);
        // Should pad to 4 (duplicate last leaf "c")
        let a = MerkleHash::hash(b"a");
        let b = MerkleHash::hash(b"b");
        let c = MerkleHash::hash(b"c");
        let mut buf1 = [0u8; 64];
        buf1[..32].copy_from_slice(&a.0);
        buf1[32..].copy_from_slice(&b.0);
        let left_root = MerkleHash::hash(&buf1);
        let mut buf2 = [0u8; 64];
        buf2[..32].copy_from_slice(&c.0);
        buf2[32..].copy_from_slice(&c.0);
        let right_root = MerkleHash::hash(&buf2);
        let mut buf3 = [0u8; 64];
        buf3[..32].copy_from_slice(&left_root.0);
        buf3[32..].copy_from_slice(&right_root.0);
        let expected_root = MerkleHash::hash(&buf3);
        assert_eq!(*tree.root(), expected_root);
    }

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::from_leaves(vec![]);
        assert_eq!(*tree.root(), MerkleHash::zero());
        assert!(tree.proof(0).is_none());
    }

    #[test]
    fn test_proof_verify() {
        let tree = MerkleTree::from_data(&[b"proof1", b"proof2", b"proof3", b"proof4"]);
        for i in 0..4 {
            let proof = tree.proof(i).expect("proof should exist");
            let leaf_hash = MerkleHash::hash(&[b"proof", i.to_string().as_bytes()].concat());
            // We need to use the actual leaf hash, not a recomputed one
            let actual_leaf = &tree.leaves[i];
            assert!(proof.verify(actual_leaf), "proof for leaf {} should verify", i);
        }
    }

    #[test]
    fn test_proof_verify_tampered() {
        let tree = MerkleTree::from_data(&[b"good", b"data"]);
        let proof = tree.proof(0).expect("proof should exist");
        let bad_hash = MerkleHash::hash(b"tampered");
        assert!(!proof.verify(&bad_hash), "tampered proof should not verify");
    }

    #[test]
    fn test_hash_roundtrip() {
        let hash = MerkleHash::hash(b"test data");
        let hex_str = hash.hex();
        let parsed = MerkleHash::from_hex(&hex_str).expect("parse should succeed");
        assert_eq!(hash, parsed);
    }

    #[test]
    fn test_merkle_hash_zero() {
        let zero = MerkleHash::zero();
        assert_eq!(zero.hex(), "0000000000000000000000000000000000000000000000000000000000000000");
    }

    #[test]
    fn test_large_tree() {
        // 100 leaves should work fine
        let data: Vec<Vec<u8>> = (0..100)
            .map(|i| format!("proof_data_{}", i).into_bytes())
            .collect();
        let refs: Vec<&[u8]> = data.iter().map(|d| d.as_slice()).collect();
        let tree = MerkleTree::from_data(&refs);
        assert_eq!(tree.leaf_count(), 100);

        // Verify a few proofs
        for i in [0, 50, 99] {
            let proof = tree.proof(i).expect("proof should exist");
            assert!(proof.verify(&tree.leaves[i]));
        }
    }
}

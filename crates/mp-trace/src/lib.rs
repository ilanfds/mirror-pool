//! The anonymity ruler.
//!
//! A behavioral pool advertises its anonymity as `k`, the number of members. On
//! a public ledger that number is almost always a lie, because *where each
//! member's funds came from* is itself public. An adversary who groups members
//! by funding source shrinks the crowd: two wallets funded from the same
//! exchange withdrawal are correlated, and in the worst case one funder behind
//! most of the pool leaves a victim nearly alone.
//!
//! This module scores the gap. Given a pool sampled as members tagged with a
//! funding source, it reports the advertised `k`, the **effective-k** an adversary
//! is left with (two standard entropy measures), and the **self-fill floor**: the
//! independent cover that survives if the single largest funder is adversarial
//! (a Sybil, or a whale funding many notes). The metric is protocol-agnostic —
//! it scores any pool, mirror-pool's own included — and it never asks you to
//! trust the advertised number; you measure it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One pool member and the source that funded it.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Member {
    /// The member wallet (opaque; used only for reporting).
    #[serde(default)]
    pub wallet: String,
    /// An identifier for where this member's funds came from — an exchange
    /// withdrawal, a common upstream wallet, etc. Members sharing it are
    /// correlated.
    pub funding_source: String,
}

/// A sampled pool.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PoolSample {
    pub members: Vec<Member>,
}

/// The scored anonymity of a pool.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Score {
    /// The advertised anonymity: the member count.
    pub advertised_k: usize,
    /// Number of distinct funding sources.
    pub distinct_sources: usize,
    /// Effective-k under min-entropy: `1 / p_max`, where `p_max` is the fraction
    /// of members in the largest funding class. This is the worst-case anonymity
    /// set an adversary faces.
    pub effective_k_min_entropy: f64,
    /// Effective-k under Shannon entropy: `2^H`. A softer, average-case measure.
    pub effective_k_shannon: f64,
    /// Independent cover left if the single largest funder is adversarial:
    /// `k - (size of the largest funding class)`.
    pub self_fill_floor: usize,
}

impl PoolSample {
    /// Sizes of each funding class, keyed by source (sorted for determinism).
    fn class_sizes(&self) -> BTreeMap<&str, usize> {
        let mut sizes = BTreeMap::new();
        for m in &self.members {
            *sizes.entry(m.funding_source.as_str()).or_insert(0) += 1;
        }
        sizes
    }

    /// Score the pool.
    pub fn score(&self) -> Score {
        let n = self.members.len();
        let sizes = self.class_sizes();

        if n == 0 {
            return Score {
                advertised_k: 0,
                distinct_sources: 0,
                effective_k_min_entropy: 0.0,
                effective_k_shannon: 0.0,
                self_fill_floor: 0,
            };
        }

        let n_f = n as f64;
        let largest = *sizes.values().max().unwrap_or(&0);
        let p_max = largest as f64 / n_f;

        // Shannon entropy of the funding-source distribution, in bits.
        let shannon_bits: f64 = sizes
            .values()
            .map(|&c| {
                let p = c as f64 / n_f;
                -p * p.log2()
            })
            .sum();

        Score {
            advertised_k: n,
            distinct_sources: sizes.len(),
            effective_k_min_entropy: 1.0 / p_max,
            effective_k_shannon: 2f64.powf(shannon_bits),
            self_fill_floor: n - largest,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(sources: &[&str]) -> PoolSample {
        PoolSample {
            members: sources
                .iter()
                .enumerate()
                .map(|(i, s)| Member {
                    wallet: format!("wallet{i}"),
                    funding_source: s.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn diverse_funding_delivers_the_advertised_k() {
        // 30 members, each from a distinct source: nothing to cluster.
        let sources: Vec<String> = (0..30).map(|i| format!("src{i}")).collect();
        let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
        let s = pool(&refs).score();
        assert_eq!(s.advertised_k, 30);
        assert_eq!(s.distinct_sources, 30);
        assert!((s.effective_k_min_entropy - 30.0).abs() < 1e-9);
        assert!((s.effective_k_shannon - 30.0).abs() < 1e-9);
        assert_eq!(s.self_fill_floor, 29);
    }

    #[test]
    fn concentrated_funding_collapses_the_effective_k() {
        // 30 members, but half come from one exchange and the rest are distinct.
        let mut v: Vec<String> = vec!["exchangeA".to_string(); 15];
        v.extend((0..15).map(|i| format!("s{i}")));
        let refs: Vec<&str> = v.iter().map(String::as_str).collect();
        let s = pool(&refs).score();
        assert_eq!(s.advertised_k, 30);
        // p_max = 15/30 = 0.5 -> effective-k = 2.
        assert!((s.effective_k_min_entropy - 2.0).abs() < 1e-9);
        // Largest funder adversarial leaves 15 of independent cover.
        assert_eq!(s.self_fill_floor, 15);
    }

    #[test]
    fn a_self_filled_pool_leaves_you_alone() {
        // A whale funds all but one slot from a single source.
        let mut v = vec!["whale".to_string(); 29];
        v.push("victim".to_string());
        let refs: Vec<&str> = v.iter().map(String::as_str).collect();
        let s = pool(&refs).score();
        assert_eq!(s.advertised_k, 30);
        // Only one slot is not the whale's.
        assert_eq!(s.self_fill_floor, 1);
        // p_max = 29/30 -> effective-k ~= 1.03, i.e. essentially one.
        assert!(s.effective_k_min_entropy < 1.05);
    }

    #[test]
    fn empty_pool_scores_zero() {
        let s = PoolSample::default().score();
        assert_eq!(s.advertised_k, 0);
        assert_eq!(s.effective_k_min_entropy, 0.0);
    }
}
